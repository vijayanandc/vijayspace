# Zoho Invoice and Zoho Books

Weft connects to both through **Any app (OAuth 2.0 sign-in)** (`http_oauth`). The two products share one API (v3).
They differ in the base path and in how each call names the organisation. The building blocks below are tested
against a mock of Zoho's API (`crates/server/tests/scenarios/zoho_books.rs`), not against a live Zoho account.

## 1. Register an OAuth client at Zoho

1. Open the Zoho API console of the data centre your organisation is on (`https://api-console.zoho.com`,
   `api-console.zoho.eu`, `api-console.zoho.in`, …).
2. Add a **Server-based application**.
3. Set the redirect URI to `https://<your Weft host>/oauth/callback`.
4. Copy the client ID and client secret.

## 2. Create the connection

Connections → New connection → **Any app (OAuth 2.0 sign-in)**:

| Field | Zoho Invoice | Zoho Books |
|---|---|---|
| Base URL | `https://www.zohoapis.com/invoice/v3` | `https://www.zohoapis.com/books/v3` |
| Authorization URL | `https://accounts.zoho.com/oauth/v2/auth` | same |
| Token URL | `https://accounts.zoho.com/oauth/v2/token` | same |
| Client ID / secret | from step 1 | from step 1 |
| Scopes (space-separated) | e.g. `ZohoInvoice.invoices.ALL ZohoInvoice.contacts.ALL ZohoInvoice.customerpayments.ALL ZohoInvoice.settings.READ` | e.g. `ZohoBooks.invoices.ALL ZohoBooks.contacts.ALL ZohoBooks.customerpayments.ALL ZohoBooks.settings.READ` |
| Extra sign-in parameters | `access_type` = `offline`, `prompt` = `consent` | same |
| **Token prefix** | `Zoho-oauthtoken` | same |
| Headers on every call | `X-com-zoho-invoice-organizationid` = your organisation ID | (none) |
| Query parameters on every call | (none) | `organization_id` = your organisation ID |
| Path to test the connection | `/organizations` | `/organizations` |

**Notes on the settings:**

- **Data centre.** Use the accounts and API domains of your organisation's data centre in both URLs, e.g.
  `accounts.zoho.eu` with `www.zohoapis.eu`, or `accounts.zoho.in` with `www.zohoapis.in`.
- **Refresh token.** `access_type = offline` is what makes Zoho return one. Without it the connection stops working
  after an hour. `prompt = consent` makes Zoho send it again when you reconnect.
- **Organisation ID.** Find it in Zoho under Settings → Organisation profile, or in the answer of `GET /organizations`
  after connecting.
- **PKCE.** Leave it on. If Zoho refuses the sign-in, turn it off under the advanced settings.
- **Limits.** Zoho limits API calls per minute and per day, by plan. Set the connection's limits to match under
  Connections → Limits & usage: runs then wait instead of failing.

## 3. Flows

Every step below is "Make an API call" on the Zoho connection, or an Appblocks action on the FSM connection. Paths
are relative to the base URL, so the same flow works for Invoice and Books.

### FSM → Zoho

| Flow | Trigger | Steps |
|---|---|---|
| **FSM invoice → Zoho invoice** (create once, then update) | Appblocks "Record changed" on `invoices` | `GET /invoices?reference_number={{ trigger.record.invoice_number }}` → decision on `{{ steps.find.body.invoices \| length }} > 0` → `PUT /invoices/{{ steps.find.body.invoices[0].invoice_id }}` or `POST /invoices` (example below) |
| Customer for the invoice | (inside the flow above, first) | `GET /contacts?email={{ … }}` → if none, `POST /contacts` → use `contact_id` as the invoice's `customer_id` |
| Line items | (inside the flow above) | Appblocks "Get record" (or the invoice's child records) → a **Custom function** step builds Zoho's `line_items` list (`name`, `rate`, `quantity`, `tax_id`) |
| FSM customer → Zoho contact | Appblocks "Record changed" on accounts or contacts | find by email → `PUT /contacts/{id}` or `POST /contacts` |
| FSM item → Zoho item | Appblocks "Record changed" on items | find (`GET /items?name=…`) → `PUT /items/{id}` or `POST /items` |
| FSM payment → Zoho payment | Appblocks "Record changed" on payments | `POST /customerpayments` with `invoices: [{invoice_id, amount_applied}]` |
| Send or mark sent | after creating the invoice | `POST /invoices/{id}/email` (Zoho emails the customer) or `POST /invoices/{id}/status/sent` |
| Void | FSM invoice voided | `POST /invoices/{id}/status/void` |

**Linking records.** Instead of searching by reference number, you can link the records with the sync steps:
`core.sync` "Save link" after creating, and "Find linked record" before updating. The FSM record and the Zoho record
then stay paired even if the reference number changes.

### Zoho → FSM

| Flow | Trigger | Steps |
|---|---|---|
| **Invoice paid in Zoho → FSM invoice paid** | **Webhook** (instant): in Zoho, Settings → Automation → Workflow rules → on invoice payment → Webhook to the flow's URL. Or **New items (polling)** on `/invoices` with query `status = paid`, items `invoices`, ID `invoice_id` | Appblocks "Find records" on invoices where `invoice_number = {{ trigger.item.reference_number }}` → "Update record" `status = Paid` |
| Payment recorded → FSM payment | Webhook from a payment workflow rule, or New items on `/customerpayments` (items `customerpayments`, ID `payment_id`) | find the FSM invoice by reference number → create the FSM payment |
| Zoho contact changed → FSM account | **New or changed items (polling)** on `/contacts`, items `contacts`, ID `contact_id`, "Changes with the item" = `last_modified_time` | find by email → update or create in FSM |
| Overdue invoice → FSM note or task | New items on `/invoices` with `status = overdue` | add a note or task to the FSM job |
| Invoice created in Zoho → FSM | New items on `/invoices` | create the FSM invoice (if invoices are also made in Zoho) |

**Polling sees only what the list answer holds.** One page holds at most 200 records (`per_page = 200`). Filter
(e.g. `status = paid`) or sort so the records you care about come first. For instant, complete delivery use Zoho's
workflow-rule webhooks.

### Example: FSM invoice → Zoho invoice (from the tested scenario)

```json
{"trigger": {"type": "manual"}, "steps": [
  {"key": "find", "type": "action", "connector": "http_oauth", "action": "api_call", "connection_id": "cxn_…",
   "input": {"method": "GET", "path": "/invoices", "query": {"reference_number": "{{ trigger.invoice_number }}"}}},
  {"key": "exists", "type": "decision", "branches": [
    {"key": "found", "condition": {"left": "{{ steps.find.body.invoices | length }}", "op": "gt", "right": 0},
     "steps": [{"key": "update", "type": "action", "connector": "http_oauth", "action": "api_call", "connection_id": "cxn_…",
       "input": {"method": "PUT", "path": "/invoices/{{ steps.find.body.invoices[0].invoice_id }}",
         "body": {"customer_id": "{{ trigger.customer_id }}", "reference_number": "{{ trigger.invoice_number }}", "line_items": "{{ trigger.lines }}"}}}]}
  ], "otherwise": [
    {"key": "create", "type": "action", "connector": "http_oauth", "action": "api_call", "connection_id": "cxn_…",
     "input": {"method": "POST", "path": "/invoices",
       "body": {"customer_id": "{{ trigger.customer_id }}", "reference_number": "{{ trigger.invoice_number }}", "line_items": "{{ trigger.lines }}"}}}
  ]}
]}
```

A `{{ … }}` value on its own keeps its JSON type, so `line_items` arrives as a list. In a live flow, the trigger is
the Appblocks invoice trigger and the values come from `trigger.record`.

## 4. Things to know

- **Repeated runs are safe.** A run that is retried after a timeout repeats only GET, PUT and DELETE calls by itself.
  A POST that timed out fails with `app.outcome_unknown`, so check Zoho before retrying it.
- **Secrets never reach the run log.** Tokens, secret headers and the client secret are encrypted in the connection.
  The run log shows each call's method, URL (without secrets), status and timing.
- **Editing the connection.** Edit renames it. To change any setting (organisation ID, data centre, scopes, prefix)
  or the Zoho account, use Reconnect, which signs in again with the new settings.
