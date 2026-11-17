# AGENTS.md: rules for AI agents working on the Weft web client
<!-- CLAUDE.md is a symlink to this file. Keep it short. -->

## What this is
The React web client for Weft, a Zoho Flow–style integration platform. This is the `client` branch; the Rust server is on the `backend` branch.
- **API contract:** the backend's `docs/api/README.md`, mirrored by `src/api/types.ts`.
- **Plan:** the backend's `docs/plan/` (doc 10 covers client UX intent; doc 13 is the current MVP).

## Stack
React 18 + TypeScript (strict, `noUncheckedIndexedAccess`), Vite, React Router 6, TanStack Query 5, React Flow (`@xyflow/react` 12) for the canvas, `lucide-react` icons, plain CSS with design tokens as variables (`src/styles.css` imports one file per area from `src/styles/`). Tests: Vitest + Testing Library (jsdom).

## Commands
```
npm run dev        # Vite dev server on :5173, proxies /api, /hooks, /oauth to http://localhost:8080 (WEFT_BACKEND overrides)
npm test           # unit + component tests
npm run check      # typecheck + lint + tests + build — MUST pass before you finish
```

## Map
| Need to… | Go to |
|---|---|
| Call an endpoint | `src/api/endpoints.ts` (one function per contract row), `src/api/client.ts` (fetch wrapper, `ApiError`) |
| Shared queries | `src/api/hooks.ts` (`qk` query keys, connectors, connections, org) |
| Contract types | `src/api/types.ts` — change only when the backend contract changes |
| UI primitives | `src/components/` (Button, Dialog, Field, Badge, JsonView, Tabs, Toast, Empty, MessageBar, PageHeader, SearchBox, Switch, Popover/OverflowMenu, Table sort header, AppIcon monogram tiles, ErrorBoundary) |
| App shell | `src/features/shell/` (global header, global search, collapsible nav rail) |
| Flow builder | `src/features/builder/` — `model/` (pure: tree ops, keys, layout, variables, conditions, schedule, history, selection, key/value rows, typed values such as integers and number fields), `canvas/` (React Flow nodes and edges; `viewLayout.ts` spaces the model layout out for the wider cards, presentation only), `panel/` (Configure / Settings / Test tabs) (side panels), `forms/` (dynamic FieldSpec forms, mapping input, variable picker), `condition/` |
| Flows list / history / connections / recipes | `src/features/{flows,history,connections,recipes}/` |

## Rules
- **Pure model first:** any change to how the definition tree is edited or laid out goes in `features/builder/model/*.ts` with a unit test. Components only call those functions.
- The builder state is an immutable `FlowDefinition` in an undo/redo reducer (`model/history.ts`). Apply edits through `useBuilder().apply(def, tag?)`. Use a `tag` for typing-style edits so they coalesce into one undo step.
- All server state goes through TanStack Query, and all HTTP goes through `src/api/client.ts`. Never call `fetch` elsewhere.
- No `any`. Use `unknown` and narrow it. Files ≤ 300 lines; split components when they grow.
- Forms render from `FieldSpec` (`forms/DynamicForm.tsx`). Do not hand-build per-connector forms. A new field type means one `case` in `FieldInput` plus a test.
- Accessibility: every input has a label (`Field` or `aria-label`), dialogs use `components/Dialog.tsx`, and interactive elements are buttons.
- Tests: pure logic → `*.test.ts` next to it; components → `*.test.tsx` using `src/test/render.tsx`. Mock HTTP by stubbing `fetch`. No real network.
- Never edit `dist/` or `node_modules/`. Pin exact dependency versions (no `^`/`~`).

## Definition of done
- [ ] `npm run check` passes.
- [ ] New pure logic has unit tests, and new UI behaviour has a component test where feasible.
- [ ] If you needed a contract change, it is noted for the backend (and `src/api/types.ts` matches the backend's `docs/api/README.md`).
