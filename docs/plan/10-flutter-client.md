# 10: Flutter client

## 1. Targets and priorities
| Target | Phase | Primary use |
|---|---|---|
| **Web** (CanvasKit/Skwasm) | MVP | Everything, especially the flow builder (desktop browsers) |
| Desktop (macOS, Windows, Linux) | P2 | The same as web; mostly free with the same code |
| Mobile (iOS, Android) | P3 | Monitoring: history, failure alerts (push), toggling flows, rerun; read-only or simplified builder |

The layout is responsive from day one, with three breakpoints: compact (< 600), medium (600–1200) and expanded (> 1200). The builder is designed for the expanded size first.

## 2. Workspace layout (the `client` branch, a Dart pub workspace)
The client lives on its own `client` branch, separate from the Rust `backend` branch. It receives the API contract by running `just sync-contract` (README, "Repository layout").
```
<client branch root>/
├── pubspec.yaml                 # workspace root (Dart pub workspaces)
├── justfile                     # sync-contract, gen, flutter-check
├── contract/                    # SYNCED from origin/backend: openapi/*.json + fixtures/flows/. Never edit.
├── app/                         # the Flutter application
│   └── lib/
│       ├── main.dart
│       ├── app/                 # router.dart, theme.dart, bootstrap.dart, env.dart
│       ├── core/                # api_provider.dart, session/, errors/ (code→message), widgets/, utils/
│       ├── features/
│       │   ├── auth/            # login, signup, verify, reset
│       │   ├── flows/           # flow list, create, folders
│       │   ├── builder/         # canvas, step panel, variable picker, condition builder, test runner
│       │   ├── connections/     # list, create (form/OAuth), reconnect
│       │   ├── history/         # execution list, execution detail (timeline, JSON viewer)
│       │   ├── usage/
│       │   └── settings/
│       └── l10n/                # ARB files (English first)
└── packages/
    ├── weft_api/                # GENERATED from openapi.json (dart-dio). Never edit.
    ├── weft_flow_model/         # FlowDefinition model (sealed classes) + canonical JSON + fixture tests
    └── weft_ui/                 # design system: tokens, components, dynamic form renderer, JSON viewer
```
Each feature folder mirrors the backend slice names and has the same shape:
```
features/<feature>/
├── data/          # repository wrapping weft_api calls; maps DTO → view models; maps problem codes
├── application/   # Riverpod notifiers/providers (state + commands)
└── presentation/  # screens/ and widgets/ (no API calls here)
```

## 3. Core choices
| Concern | Choice | Why |
|---|---|---|
| State | **Riverpod** (notifiers plus code generation) | Compile-safe, testable with `ProviderContainer` overrides, popular (good AI coverage) |
| Routing | **go_router** | Deep links (OAuth return, email links), web URLs, nested shells |
| API | Generated `weft_api` (dio) plus an auth interceptor (attach token, refresh on 401, retry once) | No hand-written HTTP |
| Models | `freezed` + `json_serializable` for view models; `weft_flow_model` hand-written with sealed classes, verified by the shared fixtures | The definition is a rich tagged union; generators handle it poorly |
| Code generation | `build_runner`; **generated files are committed**; `just flutter-gen` regenerates; CI checks drift | Agents can analyze the code without running codegen first |
| Errors | `ApiFailure { code, message, issues }` parsed from problem+json; `code` maps exhaustively to localized text | The same codes as the backend |
| Lints | `flutter_lints` plus stricter rules (`always_use_package_imports`, `avoid_dynamic_calls`, `prefer_final_locals`, `public_member_api_docs` for packages) | |
| Auth storage | Web: the refresh token stays in the httpOnly cookie and the access token in memory. Native: `flutter_secure_storage`. | Doc 08 §1 |

## 4. The flow builder (the hardest UI)

### 4.1 Interaction model (Zoho-like vertical builder)
```
┌──────────────── Flow: Order alert ───────── [Unpublished changes] [Test] [Publish] [On ◉] ┐
│                                                                                            │
│   ┌──────────────────────────────┐            ┌─────────── Step panel ────────────────┐   │
│   │ ⚡ Webhook trigger           │            │ Slack · Post message                   │   │
│   │   https://hooks…/h/9fK…  [⧉] │            │ Connection  [ Acme Slack ▼ ] [+ New]  │   │
│   └──────────────┬───────────────┘            │ Channel*    [ #sales ▼ ]               │   │
│                  ⊕                            │ Message*    [ Big order from ⟦email⟧ ] │   │
│   ┌──────────────┴───────────────┐            │             ( + Insert variable )      │   │
│   │ ◇ Is it a big order?         │            │ ▸ Advanced                             │   │
│   └───┬──────────────────────┬───┘            │ [ Test step ]   Output ▸ { ts: … }     │   │
│  Amount > 100            Otherwise            └───────────────────────────────────────┘   │
│ ┌──────┴───────┐     ┌───────┴───────┐                                                    │
│ │ Slack: post  │     │ Format name   │                                                    │
│ └──────┬───────┘     ├───────────────┤                                                    │
│        ⊕             │ Send email    │                                                    │
│                      └───────┬───────┘                                                    │
│                  ┌───────────┴───────────┐                                                │
│                  │ ⏱ Delay 1 day         │   ⚠ 2 issues                                   │
│                  └───────────────────────┘                                                │
└────────────────────────────────────────────────────────────────────────────────────────────┘
```
- **Canvas:** a vertical spine of step cards with ⊕ insert points. Decision branches render as columns, with horizontal scrolling when there are many. On compact screens, branches become tabs.
- **Step panel:** a right-side panel (or a bottom sheet on compact screens). The flow is: choose the app, choose the action, choose the connection, fill the fields, then **Test step**.
- **Validation:** issues from `PUT …/draft` (JSON Pointer paths) map to cards and fields as badges and inline errors. Publish is disabled while errors exist.
- **Editing operations:** add, delete, move up and down (drag in P2), duplicate a step, and collapse a branch.
- **Undo/redo:** the builder state is an **immutable `FlowDefinition` plus a command stack**. Every edit is a pure function `(def) → def'`, which makes it easy to unit-test.
- **Autosave:** saves are debounced (1.5 s) and use `If-Match`. On 412, a banner offers "Reload" or "Overwrite".

### 4.2 Dynamic form renderer (`weft_ui/forms/`)
One widget renders **any** connector action from its field schema (doc 07 §2). Field type → widget is a single exhaustive `switch` over a sealed `FieldSpec` class. Adding a field type means one new widget, one switch case and one golden test, following the recipe `add-connector-field-type.md`. Dynamic options call `POST …/connectors/{key}/options/{source}`, re-fetch when `depends_on` fields change, and are cached per connection.

### 4.3 Mapping field and variable picker
- **Mapping field:** a text field whose controller renders `{{ steps.x.y }}` tokens as **chips** (`WidgetSpan`) showing the step icon and a label. The stored value stays the raw template string, which matches the backend exactly.
- **Variable picker:** a popover tree loaded from `GET …/flows/{id}/variables?before_step=key`. It lists trigger fields, earlier steps (only those in scope on this path) and variables, with sample values. A search box filters it. A secondary "Format" menu inserts filters such as `| upper` or `| date("%d %b %Y")`.
- **Condition builder:** rows of `[mapping field] [operator ▼] [value]`, with AND/OR groups (nesting depth 2 in the UI). It serializes to the condition JSON in doc 04 §3.4.

### 4.4 Testing from the builder
- **Listen for a webhook sample:** a dialog with the URL, a copy button and a curl example. It polls `GET …/trigger/sample` and shows the captured fields.
- **Test step:** shows the rendered input, then the output or error, and "Use as sample".
- **Test flow:** a payload editor, then runs the test and opens a live timeline (polling in the MVP, SSE in P2).

## 5. History screens
- **List:** status chips, flow, trigger time, duration and task count. Filters: status, flow, date range, mode. Infinite scroll (cursor). Multi-select for bulk rerun (P2).
- **Detail:** a vertical timeline of step runs (attempts grouped together, decision outcomes shown as "took branch *Amount > 100*"). Each step expands to a collapsible **JSON viewer** for input, output and error, with copy-path and copy-value. The action bar has Retry from failed step, Re-run and Cancel. Badges show "possible duplicate" and "retry 3/8 at 14:05".

## 6. Client testing
| Kind | Tooling | What |
|---|---|---|
| Unit | `test` | `weft_flow_model` canonical round-trip over the **shared fixtures**; builder commands (pure edits); error code mapping (exhaustive); template token parsing |
| Provider | `ProviderContainer` with overridden repositories | Builder notifier (autosave, conflicts, undo/redo), session refresh logic |
| Widget | `flutter_test` | Every field widget; mapping field chip rendering and editing; condition builder; step panel flow |
| Golden | `matchesGoldenFile` (fonts pinned, run on Linux CI only) | Step cards, a branch layout, the form renderer for each field type, the history timeline |
| Integration | `integration_test` against the docker-compose stack | Sign up, then create a webhook flow, then send the webhook, then see the run in history |

## 7. Rules for AI agents working on the client
1. Never edit `packages/weft_api` or `contract/`. Change the backend first, then run `just sync-contract` here.
2. Presentation widgets never call the API; only `data/` repositories do.
3. One screen per file; widgets ≤ ~200 lines; extract sub-widgets into the feature's `widgets/` folder.
4. All user-visible strings go in ARB files. All colours and spacing come from `weft_ui` tokens.
5. Every new field type or builder interaction needs a widget test, and a golden test if it is visual.
6. `just flutter-check` (format, analyze, test) must pass.
