# 01: Product scope

## 1. What we are cloning

Zoho Flow is an integration platform (iPaaS). A user connects apps by building **flows**. Each flow is **one trigger** followed by a sequence of **steps** (app actions and logic). Every run is logged and can be inspected and re-run. Our product, Weft, keeps the same mental model and adds a few things that are standard in this category (loops, a public API, AI-assisted building).

### Glossary (use these words everywhere: code, UI, docs)

| Term | Meaning |
|---|---|
| **Flow** | One trigger plus an ordered tree of steps. It has a mutable *draft*, immutable published *versions*, and an on/off switch. |
| **Trigger** | What starts an execution: `webhook`, `schedule`, `app` (polling or instant), or `manual`. |
| **Step** | One node in a flow: `action`, `decision`, `delay`, `set_variable`, `stop`, and later `loop`, `script`, `call_subflow`. |
| **Connector** (UI: "App") | An integration package: auth method, triggers, actions, and dynamic option sources. |
| **Action** | One operation of a connector, such as *create record*, *send message* or *fetch record*. |
| **Connection** | Encrypted credentials for one connector, owned by an org and reusable across flows. |
| **Execution** (UI: "History entry") | One run of a flow, pinned to one flow version. |
| **Step run** | The record of one attempt of one step, with its input, output, error and timing. |
| **Task** | The usage and billing unit: one executed action. Logic steps are free. |
| **Version** | An immutable snapshot of a published definition. |
| **Template** | A reusable flow blueprint with placeholders for connections. |

---

## 2. Feature inventory and parity matrix

The legend for the **Phase** column:
- **MVP** = Phase 1, **P2** = Phase 2, **P3** = Phase 3, **P4** = Phase 4 (see [12-roadmap-and-backlog.md](12-roadmap-and-backlog.md)).
- *Zoho* means the feature is documented by Zoho (sources at the end). *Extra* means it is common in the category but not confirmed as a Zoho feature.

### 2.1 Triggers

| Feature | Source | Phase | Notes |
|---|---|---|---|
| Webhook trigger with a unique URL; accepts JSON, form data and plain text | Zoho | **MVP** | Plus XML in P2 and multipart files in P3 |
| Webhook payload that is an array becomes one run per entry | Zoho | **MVP** | Configurable, capped at 100 entries per request |
| Schedule trigger: one-time | Zoho | **MVP** | Time zone aware |
| Schedule trigger: recurring daily/weekly/monthly/yearly with time zone | Zoho | **MVP** | Plus every N minutes/hours, and cron in advanced mode |
| App trigger, polling (e.g. "new row in Sheet") | Zoho | P2 | Baseline on the first poll; deduplication |
| App trigger, instant (subscribe a webhook at the source app) | Zoho | P2 | Subscribe on enable, unsubscribe on disable |
| Manual trigger ("Run now" with a payload) | Extra | **MVP** | Also used for testing |
| Webhook signature verification (HMAC or shared secret) | Extra | P2 | |
| Email-in trigger (a unique inbox address) | Extra | P4 | Needs an inbound mail service |
| Synchronous webhook response ("return response" step) | Extra | P3 | |

### 2.2 Logic (Zoho groups these as "Flow control", "Notification" and "Custom functions")

| Feature | Source | Phase | Notes |
|---|---|---|---|
| **Decision**: several branches with conditions, plus *otherwise* | Zoho | **MVP** | Conditions use AND/OR groups |
| **If-else**: true and false paths | Zoho | **MVP** | Stored as a decision with one branch plus otherwise |
| **Delay for** a duration | Zoho | **MVP** | Up to 30 days |
| **Delay until** a date and time | Zoho | **MVP** | The date can be a mapped variable |
| **Set variable** | Zoho | **MVP** | Flow-scoped variables |
| Stop flow (with success or failure) | Extra | **MVP** | |
| **Notification** (email to org members) | Zoho | **MVP** | In-app in P2, SMS and push through connectors |
| **Custom function** (Zoho uses Deluge; we use sandboxed JavaScript) | Zoho | P2 | QuickJS sandbox with no network in P2 and a guarded fetch in P3 |
| Loop / iterate over a list, plus aggregate | Extra | P2 | Capped iteration count |
| **Subflow**: call it and wait, or call it without waiting; a *Return data* step | Zoho | P3 | |
| Error-handler path per step ("on error, go to…") | Extra | P3 | MVP has `on_error: fail_flow | continue` |
| Approval (human in the loop, approve or deny links) | Zoho (as a subflow pattern) | P4 | Built on "wait for callback" |

### 2.3 Actions and utilities

| Feature | Source | Phase | Notes |
|---|---|---|---|
| App actions: create, update, send | Zoho | **MVP** (Slack, Telegram, email); P2 adds more apps | |
| **Fetch** (search) actions, optionally creating the record when it is not found | Zoho | P2 | A manifest flag connects the search action to its create action |
| Outgoing webhook / generic HTTP request | Zoho | **MVP** | `core.http.request`, with SSRF protection |
| Formatter: text, number, date utilities | Extra (common) | **MVP** | `core.formatter.*` |
| Send email through the platform mailer | Extra | **MVP** | `core.email.send` |
| Database query (MySQL/Postgres) | Extra | P2 | Connects outward only; private networks go through the on-prem agent (P4) |
| AI step (LLM prompt, extract, classify) | Extra | P4 | Provider-pluggable connector |

### 2.4 Builder

| Feature | Source | Phase |
|---|---|---|
| Visual builder (vertical step list with branch columns) | Zoho | **MVP** |
| Variable picker and field mapping with format filters | Zoho | **MVP** |
| Test a single step; test the whole flow with a sample payload | Zoho | **MVP** |
| Capture a webhook sample ("listening" mode) | Zoho | **MVP** |
| Inline validation issues; cannot publish while errors remain | Extra | **MVP** |
| Draft vs published, with version history | Extra | **MVP** (history list); P3 (diff and restore UI) |
| Folders, search, duplicate a flow | Extra | P2 |
| Export and import a flow as JSON | Extra | P2 |
| Templates gallery (Zoho advertises 1,300+ templates) | Zoho | P3 |
| AI: describe a flow in words and get a draft | Zoho has AI settings; ours is Extra | P4 |

### 2.5 Connections and connectors

| Feature | Source | Phase |
|---|---|---|
| Connections: API key, basic, bearer, OAuth2 (auth code with PKCE) | Zoho | **MVP** |
| Reuse a connection across flows; re-authorize it; test it | Zoho | **MVP** |
| Dynamic dropdowns (e.g. list Slack channels) | Zoho | **MVP** |
| **Custom apps** (user-defined connectors with no code) | Zoho | P2 |
| Connector catalog with categories and search | Zoho (850+ apps) | MVP (small); grows every phase |
| **On-premise agent** for apps inside private networks | Zoho | P4 |

### 2.6 History and reliability

| Feature | Source | Phase |
|---|---|---|
| Per-flow history and org-wide history, filtered by status and date | Zoho | **MVP** |
| A step-by-step timeline with each step's input, output and error | Zoho | **MVP** |
| **Auto rerun** for internal or app API errors (Zoho retries up to 8 times, from 5 min out to 6 h) | Zoho | **MVP** |
| No auto rerun for configuration or mapping errors (rerun by hand after fixing) | Zoho | **MVP** |
| Manual rerun of a single execution | Zoho | **MVP** |
| **Bulk rerun** (up to 100 selected executions) | Zoho | P2 |
| Retry from the failed step on the *same pinned version* (Zoho cannot do this after major edits; we can because versions are immutable) | Extra | **MVP** |
| Email notification when an execution fails | Zoho | **MVP** |
| Live run updates in the UI (SSE) | Extra | P2 |
| History retention settings | Zoho | P3 |

### 2.7 Organisation and platform

| Feature | Source | Phase |
|---|---|---|
| Sign-up, login, password reset, email verification | — | **MVP** |
| Organisations, invitations, roles (owner/admin/member/viewer) | Zoho | P3 (MVP creates one org per user) |
| Sharing flows and connections inside an org | Zoho | P3 |
| Audit log | Zoho | P3 |
| Security settings (2FA, SSO/OIDC, IP allowlist for the UI) | Zoho | P3 |
| Usage dashboard (tasks per month) | Zoho | **MVP** (counter); P3 (dashboard, limits) |
| Plans and billing | Zoho | P3 (optional) |
| Public REST API with API keys | Extra (Zoho users have asked for it) | P3 |
| MCP server (flows as tools for AI agents) | Extra | P4 |
| Mobile app: monitor runs, toggle flows, push alerts on failure | Zoho (mobile apps exist) | P3 |
| Environments (dev/prod) and promotion | Extra | P4 |
| Data stores (key-value tables shared between flows) | Extra | P4 |

---

## 3. MVP definition (Phase 1)

### Personas
- **Builder**: an ops, marketing or support person, or a developer, who automates work between apps.
- **Admin**: owns the org, its connections and its usage. In the MVP the builder and the admin are the same person.

### MVP user stories

| ID | Story |
|---|---|
| US-01 | As a visitor I can sign up, verify my email and log in. An org is created for me. |
| US-02 | I can create a flow, rename it, describe it and delete it. |
| US-03 | I can choose a **webhook** trigger, copy its URL, send a test payload and see the captured fields. |
| US-04 | I can choose a **schedule** trigger (once, recurring or cron) in my time zone. |
| US-05 | I can add an **action** step: pick an app, then an action, then a connection, then map fields using variables from earlier steps. |
| US-06 | I can **test a step**. Its real output becomes the sample used for mapping in later steps. |
| US-07 | I can add **decision** and **if-else** steps with AND/OR conditions. |
| US-08 | I can add **delay for** and **delay until** steps. |
| US-09 | I can **set variables** and use them later. |
| US-10 | I can connect **Slack** (OAuth) and **Telegram** (bot token), and send email without connecting anything. |
| US-11 | I can **publish** a flow and switch it **on**. Editing the draft never affects the running version until I publish again. |
| US-12 | I can browse **history**, filter it by status and date, and open an execution to see each step's input and output. |
| US-13 | Transient failures **retry automatically**. If an execution finally fails, I get an email. |
| US-14 | I can **retry from the failed step** or **re-run from the start**. |
| US-15 | I can **run a flow manually** with a JSON payload. |
| US-16 | I can see **how many tasks** I have used this month. |

### MVP exit demo (the acceptance scenario for Phase 1)

> **"Order alert"**: a webhook receives a JSON *array* of 2 orders and splits it into 2 executions. Then:
> - A **decision** checks `amount > 100`.
>   - If true: post to Slack `#sales`.
>   - Otherwise: the **formatter** title-cases the customer name, then **send email** thanks the customer.
> - **Delay** 1 day, then **HTTP POST** to a review-request API.
>
> During the demo:
> - A) Slack returns HTTP 500 once, and the step is retried automatically.
> - B) A worker is killed in the middle of a run, and the run resumes on another worker. No step that had already succeeded runs again.
> - C) The delay fires correctly after 1 day; in tests this is simulated with the manual clock.
> - D) History shows every step with its input and output, and re-running a failed execution works.

### Explicitly out of the MVP
Polling and instant app triggers, custom apps, loops, scripts, subflows, multi-user orgs, templates, billing, the public API, and a builder optimised for mobile.

---

## 4. Product limits (defaults; configurable per plan later)

| Limit | Default | Why |
|---|---|---|
| Steps per flow | 100 | Keeps the UI and the engine bounded |
| Nesting depth (branches and loops) | 8 | Readability; bounds stack depth |
| Webhook request body | 5 MB | Protects ingress |
| Array split per webhook request | 100 executions | Protects against fan-out abuse |
| Stored step input/output | 1 MB each | Row size; larger outputs fail with `step.output_too_large` |
| Delay maximum | 30 days | Keeps executions bounded |
| Loop iterations (P2) | 1,000 | Prevents runaway loops |
| Engine instructions per execution | 10,000 | Guards against infinite loops |
| Action timeout | 30 s (configurable per action, max 120 s) | Keeps workers healthy |
| Auto-retry attempts | 8, backing off from 1 min to 6 h | Matches Zoho's behaviour |
| Minimum polling interval (P2) | 5 min | Rate limits at the source apps |
| History retention | 30 days (free) / 90 days (paid) | Storage |

---

## 5. Sources (Zoho Flow documentation used for the parity matrix)

- [Logic elements in Zoho Flow](https://help.zoho.com/portal/en/kb/flow/user-guide/create-a-flow/articles/logic-elements)
- [Flow Control: Set Variable, Decision, If Else, Delay, Subflow](https://help.zoho.com/portal/en/kb/flow/user-guide/create-a-flow/actions-logic/articles/flow-control)
- [Webhook trigger](https://help.zoho.com/portal/en/kb/flow/user-guide/create-a-flow/typesoftriggers/articles/webhook-trigger)
- [Schedule trigger](https://help.zoho.com/portal/en/kb/flow/user-guide/create-a-flow/typesoftriggers/articles/types-of-triggers-url-trigger)
- [Rerun failed executions](https://help.zoho.com/portal/en/kb/flow/user-guide/troubleshooting/articles/rerun)
- [Fetch actions](https://www.zoho.com/flow/help/kb/how-to-use-fetch-actions-in-your-flows.html)
- [Subflows and outgoing webhooks announcement](https://www.zoho.com/blog/flow/announcing-subflows-and-outgoing-webhooks.html)
- [History, AI and Security settings](https://help.zoho.com/portal/en/kb/flow/user-guide/settings/articles/monitoring-ai-security)
- [Conditional logic feature page](https://www.zoho.com/flow/features/conditional-logic.html)
- [Scheduled automation feature page](https://www.zoho.com/flow/features/scheduled-automation.html)
