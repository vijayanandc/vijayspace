# 07: Connectors

**Everything except control flow is a connector.** That covers HTTP, email, the formatter, Slack, Google Sheets and every user-defined custom app. The engine never learns about any particular app, which makes connectors the main extension point and the place where most AI-written code will live. They must therefore be uniform, isolated and mechanically testable.

## 1. Concepts

| Concept | Definition |
|---|---|
| Connector | `key` (`slack`, `core.http`, `custom.acme.crm`), name, icon, categories, auth spec, actions, triggers, option sources |
| Action | `key`, name, description, input **fields**, output sample/schema, `idempotent` flag, timeout |
| Trigger (P2) | `polling` or `instant`, input fields (e.g. which sheet), output sample |
| Option source | A named function that returns dropdown options (`list_channels`), possibly depending on other fields |
| Auth spec | `none`, `api_key`, `basic`, `bearer`, `oauth2`, or `custom` (fields plus how they are injected) |
| Connection | An org's encrypted credentials for one connector (doc 05 §3.3) |

Namespaces: `core.*` holds built-in utilities that need no connection; a bare key such as `slack` is a third-party app; `custom.<org>.<key>` is a user-defined app (P2).

## 2. Field schema (drives the Flutter form renderer)

| Field type | Flutter widget | Notes |
|---|---|---|
| `string` | Single-line mapping field | `format`: email, url, … |
| `text` | Multi-line mapping field | |
| `number`, `integer` | Numeric mapping field | min, max |
| `boolean` | Switch, or a mapping field in "map" mode | |
| `select` | Dropdown: static `options` or dynamic `options_from: <source>` | `multiple: true` allowed |
| `datetime`, `date` | Picker or mapping field | |
| `json` | Code editor with mapping | |
| `key_value` | Repeating key/value rows | Headers, query parameters |
| `object` | Nested group of fields | |
| `array` | Repeating group of `item` fields | e.g. line items |
| `code` (P2) | Script editor | For `core.script` |

Every field has: `key`, `label`, `help`, `required`, `default`, `placeholder`, `mappable` (default true), `depends_on` (re-fetch options when these fields change), `advanced` (collapsed by default) and `strict` (undefined variables are errors).

## 3. Native Rust connectors (`weft-connector-sdk` traits)

```rust
#[async_trait]
pub trait Action: Send + Sync {
    fn spec(&self) -> ActionSpec;
    async fn run(&self, ctx: &ActionCtx, input: Value) -> Result<Value, ConnectorError>;
}

pub struct ActionCtx {
    pub http: Arc<dyn HttpSender>,            // SSRF-guarded; base URL overridable in tests
    pub credentials: Option<Credentials>,     // decrypted, Debug-redacted (secrecy)
    pub idempotency_key: String,              // sha256(execution ‖ step ‖ iteration)
    pub org_id: OrgId,
    pub timeout: Duration,
    pub logger: StepLogger,                   // structured notes shown in step history
}

pub struct Connector {
    pub manifest: Manifest,                   // key, name, icon, auth spec, categories
    pub actions: Vec<Arc<dyn Action>>,
    pub triggers: Vec<Arc<dyn Trigger>>,      // P2
    pub option_sources: Vec<Arc<dyn OptionSource>>,
    pub auth: Arc<dyn AuthHandler>,           // test_connection, oauth config, refresh
}

pub enum ConnectorError {
    Transient { message: String },
    RateLimited { retry_after: Option<Duration> },
    AuthExpired,
    InvalidInput { field: Option<String>, message: String },
    NotFound { message: String },
    Permanent { status: Option<u16>, message: String },
}
```
`ConnectorError::from_http(status, headers, body)` maps HTTP responses consistently, which gives one place to fix mapping bugs.

### 3.1 Layout of one connector (copy the `slack/` folder)
```
crates/connectors/src/
├── lib.rs                   # builtin_connectors(): Vec<Connector>, the ONLY registration list
├── core/
│   ├── http/   { mod.rs, request.rs }
│   ├── email/  { mod.rs, send.rs }
│   └── formatter/ { mod.rs, text.rs, number.rs, date.rs }
├── slack/
│   ├── mod.rs               # pub fn connector() -> Connector
│   ├── auth.rs              # oauth2 config + test_connection (auth.test)
│   ├── client.rs            # thin typed wrapper over HttpSender (base_url injectable)
│   ├── actions/post_message.rs
│   ├── options/list_channels.rs
│   ├── fixtures/            # recorded JSON responses used by wiremock tests
│   └── README.md            # how to create the Slack app, scopes, quirks
└── declarative/             # YAML runtime (see §4)
```

### 3.2 One action file (canonical example)
```rust
//! Slack · Post message (chat.postMessage).
use serde::Deserialize;
use weft_connector_sdk::prelude::*;

pub struct PostMessage;

#[derive(Debug, Deserialize)]
struct Input { channel: String, text: String, thread_ts: Option<String> }

#[async_trait]
impl Action for PostMessage {
    fn spec(&self) -> ActionSpec {
        ActionSpec::new("post_message", "Post message")
            .description("Send a message to a channel")
            .field(Field::select("channel", "Channel").required().options_from("list_channels"))
            .field(Field::text("text", "Message").required())
            .field(Field::string("thread_ts", "Reply in thread (ts)").advanced())
            .output_sample(json!({ "ok": true, "channel": "C0123", "ts": "1700000000.000100" }))
            .idempotent(false)
    }

    async fn run(&self, ctx: &ActionCtx, input: Value) -> Result<Value, ConnectorError> {
        let input: Input = parse_input(input)?;               // → InvalidInput on mismatch
        let client = SlackClient::new(ctx)?;
        client.post_json("/api/chat.postMessage", &json!({
            "channel": input.channel, "text": input.text, "thread_ts": input.thread_ts
        })).await                                              // maps Slack's {ok:false,error} too
    }
}
```

## 4. Declarative (YAML) connectors (P2: the main way to add apps)

Most SaaS APIs are "auth, an HTTP request, then mapping the JSON". A YAML spec, interpreted by `connectors/src/declarative/runtime.rs`, covers about 80% of apps **with no Rust code**. This is ideal for AI agents: one file plus fixtures, validated by schema. The **same format powers user-defined Custom Apps** (§7).

```yaml
key: trello
name: Trello
version: 1
icon: trello.svg
categories: [project-management]
base_url: https://api.trello.com/1

auth:
  type: api_key
  fields:
    - { key: api_key, label: API key }
    - { key: token,   label: Token, secret: true }
  inject:
    query: { key: "{{ auth.api_key }}", token: "{{ auth.token }}" }
  test:
    request: { method: GET, path: /members/me }
    display: { account: "{{ response.username }}" }

option_sources:
  boards:
    request: { method: GET, path: /members/me/boards }
    map: { items: "$", value: "id", label: "name" }
  lists:
    depends_on: [board_id]
    request: { method: GET, path: "/boards/{{ input.board_id }}/lists" }
    map: { items: "$", value: "id", label: "name" }

actions:
  - key: create_card
    name: Create card
    idempotent: false
    fields:
      - { key: board_id, label: Board, type: select, required: true, options_from: boards, mappable: false }
      - { key: list_id,  label: List,  type: select, required: true, options_from: lists }
      - { key: name,     label: Name,  type: string, required: true }
      - { key: desc,     label: Description, type: text }
    request:
      method: POST
      path: /cards
      query: { idList: "{{ input.list_id }}", name: "{{ input.name }}", desc: "{{ input.desc }}" }
    output:
      sample: { id: "65a1f…", name: "Follow up", url: "https://trello.com/c/abc" }

  - key: find_card            # a "fetch" action, as in Zoho
    name: Find card
    kind: search
    create_if_missing: create_card
    fields:
      - { key: query, label: Search text, type: string, required: true }
    request: { method: GET, path: /search, query: { query: "{{ input.query }}", modelTypes: cards } }
    output: { select: "cards[0]", sample: { id: "65a1f…", name: "Follow up" } }

triggers:
  - key: new_card
    type: polling
    fields: [ { key: list_id, label: List, type: select, options_from: lists } ]
    request: { method: GET, path: "/lists/{{ input.list_id }}/cards" }
    items: "$"
    dedupe_key: "{{ item.id }}"
```
The runtime supports templated method, path, query, headers and body; auth injection (header, query or body); response selection (JSONPath-lite); pagination (`cursor`, `page`, `link-header`); error mapping (`errors: [{when_status: 401, class: auth_expired}]`); and the same SSRF guard and limits as native code.

## 5. Authentication

| Kind | Stored secret | Injection |
|---|---|---|
| `api_key` | `{fields…}` | Header, query or body, per spec |
| `basic` | `{username, password}` | `Authorization: Basic` |
| `bearer` | `{token}` | `Authorization: Bearer` |
| `oauth2` | `{access_token, refresh_token, expires_at, scope, extra}` | `Authorization: Bearer`, or as the spec says |
| `custom` | Declared fields | Template-driven |

**OAuth2 specifics:**
- Authorization-code flow with **PKCE S256**. The state is 32 random bytes, stored hashed with a 10-minute TTL and usable once.
- Client credentials per connector come from server config (`WEFT__OAUTH__SLACK__CLIENT_ID/SECRET`); self-hosters register their own apps.
- **Refresh:** before a call, if `expires_at < now + 2 min`; after a call, on `AuthExpired` (refresh once, then retry once).
- **Refresh race:** `SELECT … FOR UPDATE` on the connection row, re-check the expiry, refresh, update, commit. Concurrent workers wait and then reuse the new token.
- `invalid_grant` sets `status = needs_reauth`, notifies the owner, and fails runs with `connection.needs_reauth`. The UI then offers "Reconnect".

## 6. Testing connectors (summary; details in doc 09 §8)
1. **Conformance suite** (automatic for every registered connector, native and YAML) checks that:
   - keys are unique and well-formed, and an icon exists
   - every field type is valid, and every `options_from` refers to an existing source
   - output samples are JSON objects
   - YAML templates compile
   - **for native actions, an input generated from the spec deserializes into the action's `Input` struct**, which catches drift between spec and struct
   - every connector has a README
2. **Per-action wiremock tests:** the exact request shape (method, path, headers, body), output mapping, and error mapping for 400/401/404/429 with `Retry-After`/500.
3. **Option source tests:** request plus mapping to `[{value, label}]`.
4. **OAuth tests:** callback exchange, refresh, refresh race, `invalid_grant` leading to `needs_reauth`.
5. **Live smoke tests** (nightly, opt-in, sandbox accounts): `WEFT_LIVE_SLACK_TOKEN=… just test-live slack`. They never run in PR CI.

## 7. Custom Apps (P2: user-defined connectors, Zoho's "custom apps")
- Org-scoped declarative specs stored in `custom_connectors`, using the YAML format in §4. Authors write them through a form-based UI or a raw YAML/JSON editor with live validation.
- The same validator and runtime as the built-in YAML connectors. They are namespaced `custom.<org_key>.<key>` and are **always** behind the SSRF guard.
- Versioned. A flow pins the custom app version through its flow version, so editing a custom app does not break published flows until they are republished.
- A test console runs any action with sample input and shows the raw request and response, with secrets redacted.

## 8. Connector roadmap

| Phase | Connectors | Exercises |
|---|---|---|
| **MVP** | `core.http` (request; doubles as outgoing webhook), `core.email` (send via platform SMTP), `core.formatter` (text, number, date), **Slack** (OAuth2: post message, list channels), **Telegram** (bot token: send message) | The native SDK, OAuth2, API key, dynamic options |
| **P2** | **Google Sheets** and **Gmail** (Google OAuth; native), **GitHub** (instant trigger with HMAC), **Discord** (webhook URL), **RSS** (polling), **MySQL/Postgres** query, and YAML connectors: **Trello, Airtable, Notion, Stripe, Twilio, Mailchimp, HubSpot, Jira, Asana, ClickUp** | Polling, instant triggers, declarative runtime, fetch-or-create |
| **P3** | Microsoft 365 (Outlook, Teams, OneDrive), Shopify, WooCommerce, Salesforce, Zendesk, Freshdesk, Calendly, Typeform, Google Forms, Dropbox, Box, Zoom, plus **Weft** itself (call a subflow) | Breadth |
| **P4** | LLM providers (prompt, extract, classify, summarize), on-prem agent connectors (local DB, file share) | AI steps, agent channel |

Choose the Phase 2 order based on your target users (open question 3 in the README).
