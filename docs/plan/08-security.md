# 08: Security

An iPaaS holds **other systems' credentials** and makes **outbound requests on users' behalf**. That makes it a high-value target. The main risks are:

| Threat | Example | Primary control |
|---|---|---|
| Cross-tenant data access | Org A reads org B's executions | Tenant scoping in every port signature, plus isolation tests |
| Credential theft | A DB dump exposes OAuth tokens | AES-256-GCM encryption with AAD binding; secrets never returned or logged |
| SSRF | An HTTP action calls `169.254.169.254` or internal services | A DNS-resolving IP filter in the HTTP client; redirect checks |
| Webhook abuse | Flooding a flow; guessing tokens | 256-bit tokens, rate limits, size limits, optional HMAC |
| Script escape | A custom function reads files or the network | Rhai sandbox: only registered functions exist; operation, depth and size limits |
| Account takeover | Credential stuffing; stolen refresh tokens | argon2id, login rate limits, refresh rotation with reuse detection, 2FA (P3) |
| Template injection | User data rendered as template code | Templates come only from the definition; payload values are data, never templates |
| Supply chain | A malicious crate | cargo-deny (advisories, licenses, sources), lockfile, minimal dependencies |

## 1. Authentication
- Passwords: **argon2id** (default parameters, tuned so hashing takes about 50 ms), 12 characters minimum, checked against a top-10k breached-password list.
- **Access token:** a JWT (HS256 with a server secret, or EdDSA) with a **15-minute** TTL. Claims: `sub` (user), `sid` (session family), `iat`, `exp`. No roles in the token: the role is loaded per request through a cached membership lookup, so revoking a role takes effect quickly.
- **Refresh token:** opaque, 32 random bytes, stored as a SHA-256 hash, valid 30 days, **rotated on every use**. Re-using a rotated token revokes the whole family (theft detection).
- Web client: the refresh token lives in an `HttpOnly; Secure; SameSite=Strict` cookie scoped to `/api/v1/auth`. Native clients use platform secure storage. CSRF is not a concern for bearer-authenticated APIs; the cookie is only sent to the refresh endpoint, which also requires an `X-Requested-With` header.
- Rate limits: login is limited to 5 attempts per minute per IP+email, with exponential lockout. Forgot-password always returns 202.
- P3: TOTP 2FA, OIDC SSO (Google and Microsoft), session list with "log out everywhere".

## 2. Authorization: RBAC as a pure, exhaustively tested function

`weft-domain::permissions::allows(role: Role, p: Permission) -> bool` is a `match` with no wildcard arms. A table-driven test asserts **every cell** of this matrix:

| Permission | owner | admin | member | viewer |
|---|:-:|:-:|:-:|:-:|
| `org.manage` (rename, settings, delete) | ✓ | | | |
| `org.billing` | ✓ | | | |
| `members.manage` | ✓ | ✓ | | |
| `flow.view` | ✓ | ✓ | ✓ | ✓ |
| `flow.edit` / `flow.publish` / `flow.toggle` | ✓ | ✓ | ✓ | |
| `flow.delete` | ✓ | ✓ | own | |
| `execution.view` | ✓ | ✓ | ✓ | ✓ (payloads redacted) |
| `execution.rerun` / `execution.cancel` | ✓ | ✓ | ✓ | |
| `connection.create` | ✓ | ✓ | ✓ | |
| `connection.use` | ✓ | ✓ | own and shared | |
| `connection.delete` | ✓ | ✓ | own | |
| `audit.view` | ✓ | ✓ | | |
| `api_keys.manage` | ✓ | ✓ | | |

Every use case begins with `actor.require(Permission::…)`. An API-level test matrix (doc 09 §7.3) checks each endpoint against each role.

## 3. Tenant isolation
- Every repository method on tenant data takes `OrgId` as a parameter, and the SQL always includes `WHERE org_id = ?`. The single exception is cross-tenant "system" methods used by workers (claiming jobs, firing schedules). These are named `*_system`, live in a separate port (`SystemRepo`), and the API layer never receives that port.
- A resource that exists in another org returns **404**, never 403, so its existence is not revealed.
- **Isolation tests:** for every resource type, create it in org A, then call every endpoint as a member of org B, and expect 404 with no data in the body. A generic harness iterates over the route table.

## 4. Secrets
- Connection credentials, the PKCE verifier and (P3) secret org variables are encrypted with **AES-256-GCM**:
  - a random 96-bit nonce per encryption;
  - **AAD = `org_id ‖ connection_id`**, so a ciphertext copied to another row fails to decrypt;
  - stored as `nonce ‖ ciphertext ‖ tag`, together with `secret_key_id`.
- Master keys come from `WEFT__CRYPTO__MASTER_KEYS` (a key ID → base64 key map) with an `ACTIVE_KEY_ID`. Rotation: add a new key, make it active, run the `reencrypt_secrets` maintenance job, then retire the old key. P3 adds an optional KMS adapter for envelope encryption behind the same `SecretBox` port.
- Decrypted secrets are wrapped in `secrecy::SecretString`/`SecretBox` (`Debug` prints `[REDACTED]`, and the memory is zeroized). They are never serialized into DTOs, logs, step inputs or errors.
- Step run inputs are stored **after** template rendering but **before** auth injection, so credentials never land in history.
- Log redaction: a tracing layer scrubs known secret header names and JSON keys (`authorization`, `password`, `token`, `secret`, `api_key`) from structured fields.

## 5. SSRF protection (HTTP action, custom apps, YAML connectors, webhook subscriptions)
- **Only** `weft-infra::http` creates outbound clients. It is a reqwest client with a custom **DNS resolver** that resolves the host and rejects any address in these ranges:
  - loopback `127.0.0.0/8` and `::1`; unspecified `0.0.0.0/8`
  - private `10/8`, `172.16/12`, `192.168/16`; CGNAT `100.64/10`
  - link-local `169.254/16` (cloud metadata!) and `fe80::/10`; unique-local `fc00::/7`
  - multicast, broadcast and reserved ranges
  - IPv4-mapped IPv6 (`::ffff:a.b.c.d`), which is checked as its IPv4 address
- Because the connection uses the filtered addresses, **DNS rebinding** cannot slip past the check, and redirects go through the same resolver. Redirects are limited to 5, and an `https → http` downgrade is refused.
- Only the `http` and `https` schemes are allowed. Response bodies are capped at 10 MB, with timeouts on connect, headers and the total request.
- The policy is configurable for self-hosters (`allow_private_networks = ["10.20.0.0/16"]`). Tests inject a policy that allows `127.0.0.1` so wiremock is reachable. **The default policy is tested to block localhost.**
- The on-prem agent (P4) is the *supported* way to reach private networks.

## 6. Webhook ingress
- The token is 32 random bytes (256 bits) from the OS CSPRNG. Users can rotate a flow's URL.
- Limits: 5 MB body, 100 array splits, a per-token rate limit (MVP default 50 req/s with bursts) and per-org limits (P3).
- Optional verification (P2): a shared-secret header, or an HMAC-SHA256 signature over the raw body with a timestamp tolerance (replay protection).
- Ingress logs never record full bodies, only the size, content type and token prefix.

## 7. Script sandbox (P2 custom functions, Rhai)
Custom functions run in **[Rhai](https://rhai.rs)**, an embedded scripting language written in pure Rust with JavaScript-like syntax. In our product it plays the role of Zoho's Deluge. Rhai is sandboxed by construction: a script can only call functions the host registers. There is no file, network, process or environment access to take away, and no `unsafe` FFI boundary. The usage contract is in doc 07 §9; ADR-013 records the decision.

**Engine configuration** (`weft-script`, a pure crate with no IO):
- **Cargo features:**
  - `sync`: the engine and ASTs can be shared across threads.
  - `serde`: JSON ↔ `Dynamic` conversion.
  - `no_time`: no `timestamp()`. Scripts get "now" only from their input, so runs are deterministic.
  - `no_module`: no `import`.
  - **Never enable `unchecked`.** It removes every limit below. `cargo-deny` bans it.
- **Limits** (defaults, configurable per plan):

  | Limit | Default |
  |---|---|
  | `set_max_operations` | 1,000,000 (the CPU budget, deterministic) |
  | `set_max_call_levels` | 32 |
  | `set_max_expr_depths` | 64 / 32 |
  | `set_max_string_size` | 1 MB |
  | `set_max_array_size` | 10,000 |
  | `set_max_map_size` | 10,000 |
  | Source code size | 64 KB |
  | Output size | 1 MB, the same as any step output |
- `disable_symbol("eval")`.
- **Output redirected:** `print` and `debug` go through `on_print`/`on_debug` into the step's history log (at most 100 lines or 10 KB), never to stdout.
- **Registered helpers only:**
  - JSON parse and stringify
  - date formatting and arithmetic on values passed in
  - regex with size limits
  - base64, URL encoding, SHA-256, HMAC

  They reuse the pure implementations behind `weft-expr`'s template filters, so templates and scripts behave the same.
- **Per run:** a fresh `Engine` is built for every run from packages built once at startup. This is cheap because packages are shared, and it keeps log capture and the time guard per run. Compiled ASTs are shared through an LRU cache keyed by the SHA-256 of the source.

**Runtime guards:**
- **Time:** the operation budget is the primary, deterministic limit. As a backup, `on_progress` aborts after 2 s of wall time. This uses monotonic `Instant`; it's a safety net, not business logic.
- **Memory:** Rhai has no direct memory cap. The string, array and map size limits bound what one script can allocate, and the bounded pool below caps how many run at once.
- **Isolation from async work:** the `ScriptRunner` port (in `weft-connector-sdk`) is implemented in `weft-infra` by a pooled runner: a semaphore (default: CPU count) around `spawn_blocking`. Scripts therefore never block async worker threads. This is the one sanctioned use of `spawn_blocking` outside `weft-worker`/`weft-server`.
- **Errors are never auto-retried.** A syntax error or `throw` gives `script.error`, with line and column. Hitting a limit gives `script.limit_exceeded`. Both count as configuration errors, like mapping errors in Zoho.
- **P3:** a guarded `http_request(...)` helper that delegates to the SSRF-protected HTTP sender and counts as a task. Until then scripts have no network access at all.
- **Later option:** JavaScript (QuickJS or Boa) can be added behind the same `ScriptRunner` port if customers ask for it. Flows store `language: "rhai"`, so that addition would be backward compatible.

## 8. Data protection and privacy
- Encryption in transit everywhere: TLS at the load balancer, and TLS to MySQL in production. Encryption at rest comes from the managed database or the disk, plus application-level encryption for secrets.
- Retention per plan (doc 01 §4), enforced by `retention_sweep`. Orgs can lower it.
- **Payload masking (P3):** mark fields as sensitive per step so their values are stored masked in history.
- Data subject requests (P3): export an org's data, delete an org (hard delete after a 30-day grace period), and delete a user.
- Audit log (P3) covers login, member and role changes, connection create and delete, flow publish, toggle and delete, API key creation, and exports.

## 9. Supply chain and platform hygiene
- `cargo-deny`: advisories (fails on vulnerabilities), license allowlist, git sources banned, duplicate core crates banned.
- `Cargo.lock` committed; Dependabot or Renovate weekly, with grouped updates that must pass `just check`.
- Container: distroless or `debian-slim`, non-root, read-only filesystem, a single static-ish binary.
- Secrets in CI come only from the CI secret store. Live connector credentials exist only in the nightly job.

## 10. Security testing checklist (each item maps to a test; see doc 09 §10)
- [ ] The RBAC matrix is checked cell by cell (domain), endpoint by role (HTTP).
- [ ] Tenant isolation is checked across every route.
- [ ] The SSRF guard is checked across every blocked range, IPv6 forms, redirects to private IPs, a DNS rebinding simulation, and non-HTTP schemes.
- [ ] Ciphertext with a wrong AAD fails; key rotation round-trips.
- [ ] `Debug` and log output of structures holding credentials contain no secret.
- [ ] JWTs that are expired, have a wrong signature, `alg: none` or a tampered `sub` are rejected.
- [ ] Refresh token reuse revokes the family.
- [ ] Webhook body limit, rate limit and HMAC verification (P2) are enforced.
- [ ] Script limits are enforced: infinite loops, deep recursion, string and array growth bombs, `eval`, `import`, calls to unregistered functions, print flooding; the `unchecked` feature is banned (P2).
- [ ] Template sandbox: fuel exhaustion and recursion are handled, and a payload that contains `{{ }}` is **not** evaluated.
