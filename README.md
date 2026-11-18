# Weft web client

The browser UI for Weft (a Zoho Flow–style integration platform): flows list, a drag-and-drop flow builder, connections, run history and prebuilt recipes. The server lives in [`vijayanandc/appblocks_flow`](https://github.com/vijayanandc/appblocks_flow).

## Run with the backend
```sh
# 1. In the backend worktree (Rust server + MySQL), start the API on :8080 — see its README.
# 2. Here:
npm install
npm run dev            # http://127.0.0.1:3000 — proxies /api, /hooks, /oauth, /healthz to http://localhost:8080
```
Use `WEFT_BACKEND=http://host:port npm run dev` to point the proxy elsewhere, or build with `VITE_API_URL=https://api.example.com npm run build` to call the API directly (the server must allow the origin via CORS). Sign-in follows the server: `/api/v1/auth/config` turns Logto on (the app's redirect URI is `http://127.0.0.1:3000/callback`), or off for local development (`WEFT_AUTH=off` on the server).

## Features
- **Flows**: list with trigger type, version, unpublished-changes badge, last run, on/off toggle; create and delete.
- **Builder**:
  - React Flow canvas with the trigger at the top and steps below. Decisions fan out into branch columns plus *otherwise*.
  - **Drag** logic items or app actions from the palette onto a **+**. Drag a step onto another **+** to move it. Or click a **+** and then a palette item.
  - Side panel per node. Forms are generated from connector field specs, including dynamic dropdowns and record fields.
  - A variable picker inserts `{{ … }}`. There is a condition builder with AND/OR groups, schedule presets, and a webhook sample listener.
  - "Test step" with "Use as sample".
  - Debounced autosave with revision conflicts, undo/redo (Ctrl+Z / Ctrl+Shift+Z), a validation issues panel (click to jump), Test run, Publish, enable/disable, and a versions list.
- **Connections**: credential forms or the OAuth redirect (with a `/connections/callback` landing page), test, delete.
- **History**: filterable, auto-refreshing list with “Load more”. The execution detail page has a step timeline (attempts grouped, decision branches, delays), JSON viewers, and retry / re-run / cancel.
- **Recipes**: install prebuilt integrations by choosing (or creating) a connection per app.

## Development
```sh
npm test          # Vitest
npm run lint
npm run typecheck
npm run arch      # architecture check (layers, feature APIs, restricted APIs)
npm run check     # all of the above + production build
```
See `AGENTS.md` for the architecture, recipes for common changes and the rules (changes are made by AI agents).
