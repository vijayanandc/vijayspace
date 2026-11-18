# AGENTS.md: rules for AI agents working on the Weft web client
<!-- CLAUDE.md is a symlink to this file. Changes to this client are made by AI agents: keep the rules exact. -->

## What this is
The React web client for Weft, a Zoho Flow–style integration platform. This is the `client` branch; the Rust server is on the `backend` branch.
- **API contract:** the backend's `docs/api/README.md`, mirrored by `src/api/types.ts` and `src/api/definition.ts`.
- **Plan:** the backend's `docs/plan/` (doc 10 covers client UX intent; doc 13 is the current MVP).

## Stack
React 18 + TypeScript (strict, `noUncheckedIndexedAccess`), Vite, React Router 6, TanStack Query 5, React Flow (`@xyflow/react` 12), `lucide-react` icons, `@logto/react` for sign-in, plain CSS with design tokens (`src/styles.css` imports one file per area from `src/styles/`). Tests: Vitest + Testing Library (jsdom).

## Commands
```
npm run dev        # Vite on http://127.0.0.1:3000 (the Logto redirect URI); proxies /api, /hooks, /oauth to http://localhost:8080
                   # (WEFT_BACKEND, WEFT_CLIENT_HOST and WEFT_CLIENT_PORT override)
npm test           # unit + component tests
npm run arch       # architecture check (scripts/check-arch.mjs): layers, feature APIs, cycles, restricted APIs, headers, sizes
npm run check      # typecheck + lint + arch + tests + build. MUST pass before you finish.
```

## Architecture
Code is arranged in layers. A file imports only from its own layer or the layers below it. `npm run arch` enforces this.
```
src/main.tsx, App.tsx   composition: providers and routes (pages come from each feature's index.ts)
src/features/<name>/    one product area: pages, dialogs, hooks. Its index.ts is its only public API.
   <name>/model/        pure state logic used only by that feature (no React, no HTTP)
src/forms/              FieldSpec forms: DynamicForm and its inputs (used by connections and the builder)
src/components/         UI primitives (Button, Dialog, Field, Badge…): props in, events out, no data fetching
src/domain/             pure flow-definition logic shared by features (tree ops, keys, factories, variables,
                        values, scripts, trigger kinds). No React, no HTTP.
src/api/                client.ts (the only fetch), endpoints.ts, tenancy.ts, hooks.ts (query keys and hooks),
                        queryClient.ts; types.ts + definition.ts are the contract (importable from every layer)
src/lib/                small helpers: storage (the only browser storage), safePath, format, sort, clipboard
src/test/               render helper, fixtures (tests only)
```
| Layer | May import |
|---|---|
| `features/<f>` | its own files, other features **only through `features/<g>/index.ts`**, `forms`, `components`, `domain`, `api`, `lib` |
| `features/<f>/model` | its own model, `domain`, `lib`, the contract types. No runtime package imports (`import type` is fine). |
| `forms` | `components`, `domain`, `api`, `lib` |
| `components` | `lib`, the contract types |
| `domain` | `lib`, the contract types. No runtime package imports. |
| `api` | `lib` |
| `lib` | `lib` (and React for small hooks) |
| tests (`*.test.ts(x)`) | anything their layer may, plus `src/test` |

Feature dependencies form a tree, never a cycle (checked). Today: every feature uses `auth` (session, role, `can`); `builder` and `recipes` use `connections` (ConnectionDialog, ConnectionPicker); `history` uses `settings` (retention note); `platform` (staff: servers, queue lag, alerts) uses only `auth`; `App.tsx` uses every feature. When two features need the same thing, move it **down** (pure logic → `domain`, FieldSpec inputs → `forms`, presentational UI → `components`, helpers → `lib`) or pass it as a prop. Never import another feature's internal file.

## Map
| Need to… | Go to |
|---|---|
| Call an endpoint | `api/endpoints.ts` (one function per contract row; `api.*(…, { org })` pins a request to an org), `api/client.ts` (fetch wrapper, `ApiError`) |
| Shared queries | `api/hooks.ts` (`qk` query keys, `mk` mutation keys, connectors, connections, org). The cache is partitioned by org (`api/queryClient.ts`). |
| Contract types | `api/types.ts`, `api/definition.ts`: change only when the backend contract changes |
| Sign-in, session, orgs, roles | `features/auth/` (see "Sign-in" below), public API in `features/auth/index.ts` |
| Edit the flow definition (insert, move, delete, keys, variables) | `domain/*.ts` with a unit test next to it |
| Builder | `features/builder/`: `model/` (undo history, layout, selection, rename, paths, schedule, conditions), `canvas/` (React Flow nodes and edges), `panel/` (Configure / Settings / Test), `condition/` |
| Dynamic forms | `forms/DynamicForm.tsx` (`FieldInput` switch), mapping input, variable picker, `CodeEditor` |
| UI primitives | `components/` |
| App shell | `features/shell/` (header, global search, nav rail in `nav.ts`, org switcher) |
| Flows list / history / connections (usage, limits, OAuth) / recipes / settings / platform (staff) | `features/{flows,history,connections,recipes,settings,platform}/` |

**Sign-in:** `AuthRoot` reads `/api/v1/auth/config`: Logto on, or sign-in off for local development. `LogtoGate` finishes the callback, registers the token hooks on the API client and handles sign-out (it waits for draft saves; afterwards requests fail with `auth.unavailable`). `session.tsx` holds the current org; switches are two-phase (the page closes and its draft save finishes first). `permissions.ts` mirrors the server's role matrix. Plain-HTTP origins: `webCrypto.ts` (SHA-256 fallback) and `logtoClient.ts` (ID tokens checked by claims).
**OAuth connections:** `connections/oauth.ts` builds `return_to` (with `next` and the starting `org`); the provider's answer comes back as `?status=finish`, and `ConnectionCallbackPage` finishes it once with `endpoints.oauthFinish` in that org.

## Recipes
- **New endpoint:** add the types to `api/types.ts` (as in the backend contract), one function in `api/endpoints.ts`, a query key in `qk` and a hook in `api/hooks.ts` if several places read it. Mutations invalidate the keys they change. Test by stubbing `fetch`.
- **New page:** a component in its feature, exported from the feature's `index.ts`, a `<Route>` in `App.tsx`, a nav item in `features/shell/nav.ts` if it is top-level. Add a component test.
- **Role-gated action:** add the permission to `features/auth/permissions.ts` exactly as in the server's `crates/domain/src/permissions.rs`; hide or disable the control with `can(useRole(), 'perm', createdBy, useUserId())`; test with `<TestSession role="viewer">`.
- **New field type:** one `case` in `forms/DynamicForm.tsx` (`FieldInput`) plus a test in `forms/forms.test.tsx`.
- **New definition edit (builder):** a pure function in `domain/tree.ts` (or a new `domain/*.ts`) with a unit test; the panel calls it through `useBuilder().apply(def, tag?)`.
- **Browser storage:** `lib/storage.ts` (`read`/`write`/`remove`, `'local'` or `'session'`): preferences only, never tokens or data.

## Rules
- **Pure first:** logic that can be a function of data is a function of data, in `domain/` or `features/<f>/model/`, with a unit test. Components only call it.
- The builder state is an immutable `FlowDefinition` in an undo/redo reducer (`features/builder/model/history.ts`). Apply edits through `useBuilder().apply(def, tag?)`; a `tag` coalesces typing into one undo step.
- All server state goes through TanStack Query, and all HTTP through `api/client.ts`. Never call `fetch` elsewhere (checked).
- Hide or disable actions the role cannot do; the server decides. Never put an org ID in a request body: the org is the `X-Org-ID` header.
- No `any`; use `unknown` and narrow it. Named exports only. Files ≤ 300 lines (checked); split by responsibility. Every file starts with a comment saying what it is for (checked).
- Forms render from `FieldSpec`; never hand-build per-connector forms.
- Accessibility: every input has a label (`Field` or `aria-label`), dialogs use `components/Dialog.tsx`, interactive elements are buttons, counts and icons have text for screen readers (`sr-only`).
- Tests: pure logic → `*.test.ts` next to it; components → `*.test.tsx` with `src/test/render.tsx`. Stub `fetch`; no real network, no sleeps longer than a few milliseconds.
- Never edit `dist/` or `node_modules/`. Pin exact dependency versions (no `^`/`~`) and say why in the commit.

## Security rules
- No HTML injection (`dangerouslySetInnerHTML`, `innerHTML`), `eval`, or `new Function` (checked). Render text through React.
- Navigate after a redirect only to `lib/safePath.ts` `safeNextPath(…)`; external links need `rel="noopener noreferrer"` (checked).
- Tokens stay inside the Logto SDK: never store, log or put them in URLs.
- 🔒 Human review for changes to `features/auth/{LogtoGate,logtoClient,webCrypto,permissions}.ts(x)`, `api/client.ts`, `lib/safePath.ts` and `scripts/check-arch.mjs`. Never weaken the architecture check to make a change pass.

## How to work
1. Read the backend contract row (`docs/api/README.md`) and the files in the Map for the change.
2. List the files you will change and the tests you will add. Stay inside one feature plus the shared layers it needs.
3. Write the failing test first (pure function or component), then the code.
4. Run `npm run check` and fix everything it reports. An `arch` error names the rule and how to fix it.
5. Update this file if you add a layer, a feature, a pattern or a 🔒 file.
6. Report what changed, the tests added, and any backend contract change needed.

## Definition of done
- [ ] `npm run check` passes (typecheck, lint, architecture, tests, build).
- [ ] New pure logic has unit tests; new UI behaviour has a component test where feasible.
- [ ] A new cross-feature need is met through an `index.ts` export or by moving code down a layer.
- [ ] If you needed a contract change, it is noted for the backend, and `src/api/types.ts` matches its `docs/api/README.md`.
