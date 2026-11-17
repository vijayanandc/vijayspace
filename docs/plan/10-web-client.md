# 10: Web client (React)

> **Changed:** this doc used to plan a Flutter client. For the MVP the client is a **web app in React**, because the core screen is a drag-and-drop node editor, and the web has a mature, well-tested library for that (React Flow). Mobile is out of scope for now. A later mobile app would mostly need history and on/off switches, not the builder.

## 1. Stack
| Concern | Choice | Why |
|---|---|---|
| Language | TypeScript, `strict` + `noUncheckedIndexedAccess` | Agents get fast, precise feedback from the type checker |
| UI | React 18 | Largest ecosystem; React Flow is React-native |
| Builder canvas | `@xyflow/react` (React Flow 12) | Custom nodes, HTML5 drag-and-drop from a palette, pan/zoom, minimal glue |
| Server state | TanStack Query 5 | Caching, polling (history refresh), mutation invalidation |
| Routing | React Router 6 | |
| Build | Vite | Dev proxy to the Rust API (`/api`, `/hooks`, `/oauth`) |
| Styling | Plain CSS with variables (`src/styles.css`) | No runtime dependency, easy to theme |
| Tests | Vitest + Testing Library (jsdom); Playwright for end-to-end checks | |
| Auth | None yet. With Logto (doc 13 §6): `@logto/react`, access token for the Weft API resource, `X-Org-ID` for the selected org | |

## 2. Layout (`client` branch)
```
src/
  api/         client.ts (fetch wrapper, ApiError from problem+json), endpoints.ts (one function per
               contract row), types.ts (mirrors docs/api/README.md), hooks.ts (shared queries, keys)
  components/  Button, Dialog, Field, Badge, JsonView, Tabs, Toast, Empty
  features/
    builder/   model/ (pure: tree ops, keys, rename, paths, layout, history, variables, conditions,
               schedule), canvas/ (React Flow nodes), panel/ (trigger, action, decision, logic
               panels), forms/ (DynamicForm from FieldSpec, mapping input, variable picker),
               condition/, BuilderPage, Toolbar, IssuesPanel, useAutosave
    flows/ history/ connections/ recipes/ shell/
```

## 3. Builder design
- **Canvas:** a vertical spine of step cards. Decision branches render as columns, each ending at a shared join. Insert points (`+`) sit between cards. The layout is computed from the definition by a pure function (`model/layout.ts`), so nodes are never dragged freely: the definition is the only source of truth.
- **Palette:** logic steps (decision, delay, set variable, stop) and every connector action. Drag an item onto a `+`, or click a `+` and then click an item. With no `+` selected, a click inserts at the end.
- **Side panel:** trigger, action, decision and logic panels. Action fields render from the connector's `FieldSpec`s (`forms/DynamicForm.tsx`). Dynamic options call `POST /api/v1/connectors/{key}/options/{source}` and reload when `depends_on` fields change.
- **Mapping input:** a text input that stores the raw template (`{{ steps.x.y }}`) and shows a preview with highlighted tokens. The **variable picker** lists the trigger sample, earlier steps in scope on this path, and variables.
- **Condition builder:** stacked rows (value / operator / compare-to) with AND/OR groups. It serialises to the condition JSON in doc 04 §3.4, and has an "Edit as JSON" escape hatch.
- **State:** an immutable `FlowDefinition` in an undo/redo reducer. Every edit is a pure function in `model/`, which is unit-tested.
- **Autosave:** debounced by 1.5 s, sending the draft revision. On 409 a conflict banner offers "Reload". The builder validates when a flow opens and after every save, and the issues panel maps JSON Pointer paths back to nodes.
- **Testing from the builder:** listen for a webhook sample, test a step, and test-run the draft. A test run opens the execution timeline.

## 4. Other screens
- **Flows:** list with status, version, last run, and an on/off switch. New flow dialog chooses the trigger type.
- **History:** filter by flow and status, cursor pagination, refreshes every 5 s. The detail page shows a step timeline with attempts, the decision branch taken, collapsible JSON for input/output/error, and Retry / Re-run / Cancel.
- **Connections:** field-based connections (e.g. an Appblocks API token) and OAuth connections (Zoho CRM, Autodesk Forma) via `/oauth/callback`. Each connection has a Test button. Re-authorising an existing OAuth connection is not in the UI yet; the API supports it (`connection_id` on `oauth/start`).
- **Recipes:** catalog cards with apps and flows, and an install wizard that asks for one connection per slot and creates drafts.

## 5. Client testing
| Kind | Tooling | What |
|---|---|---|
| Unit | Vitest | Model functions: tree edits, key generation and rename (references updated), layout, conditions, schedules, variables, timeline grouping, autosave state machine |
| Component | Testing Library | Dynamic forms per field kind, condition builder |
| End to end | Playwright against `weft serve` + `vite` | Create → drag steps → configure → publish → enable → webhook → history; recipe install → drafts flagged by validation |

## 6. Rules for AI agents working on the client
The `client` branch's `AGENTS.md` is authoritative. In short:
1. Pure model first: tree edits go in `features/builder/model/` with a unit test.
2. All HTTP goes through `src/api/client.ts`; all server state goes through TanStack Query.
3. Forms render from `FieldSpec`. Never hand-build a form for one connector.
4. `src/api/types.ts` changes only when the backend contract (`docs/api/README.md`) changes.
5. `npm run check` (typecheck, lint, tests, build) must pass.
