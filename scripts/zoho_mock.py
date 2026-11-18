#!/usr/bin/env python3
"""Mock Zoho CRM (accounts server, CRM API v8, notification channels) for testing Weft end to end.

LOCAL TESTING ONLY. This is not Zoho. It holds made-up data in memory, binds 127.0.0.1 only, and its
/_mock/ admin endpoints need no auth, so never expose it. Python 3.11+, standard library only.

    python3 -I scripts/zoho_mock.py --port 9200 --client-id weft-test --client-secret "$SECRET" \
        --weft-public http://127.0.0.1:8080 [--state-file zoho-mock.json] [--token-ttl 3600] \
        [--limit-per-minute 100] [--limit-per-day 25000] [--no-seed]

The secret can come from ZOHO_MOCK_CLIENT_SECRET instead (keeps it out of `ps`). The accounts server and the
API domain are both http://127.0.0.1:<port>. The only redirect URI accepted is <--weft-public>/oauth/callback.
The request and response shapes follow crates/connectors/src/zoho_crm/ and crates/connector-sdk/src/oauth.rs.

  OAuth  GET  /oauth/v2/auth          consent page (Accept / Deny); ?auto=1 accepts at once, ?auto=deny denies.
                                      Accept -> redirect_uri?code=&state=&location=us&accounts-server=<this server>
         POST /oauth/v2/token         authorization_code (single use, 2 min, bound to client and redirect_uri) or
                                      refresh_token (a new access token only). Errors: HTTP 200 {"error": ...}
         POST /oauth/v2/token/revoke  ?token=<refresh or access token>
  CRM    /crm/v8/...                  org, settings/modules, settings/fields?module=, {Module} list / insert / bulk
                                      update / bulk delete, {Module}/{id} get / update / delete, {Module}/upsert,
                                      {Module}/search, {Module}/{id}/Notes, actions/watch (GET POST PATCH PUT DELETE).
                                      Modules: Leads, Contacts, Accounts, Deals, Tasks, Events, Notes, with the
                                      custom fields Weft's recipes write. Header: Authorization: Zoho-oauthtoken <t>.
  Admin  /_mock/records/<Module>[/<id>]  GET, POST (create), PATCH, DELETE: acts as a CRM user, fires notifications
         /_mock/calls (GET, DELETE)   request log (method, path, status, time; CRM query and body, tokens removed)
         /_mock/channels, /_mock/deliveries, /_mock/health, POST /_mock/expire-tokens

Every record change (API or admin) is POSTed in Zoho's notification shape to each matching, unexpired channel's
notify_url, from a background thread with retries and never through a proxy.

Switches for points the connector leaves open (defaults follow Zoho's documentation as far as it is known):
  --if-modified-since-inclusive   If-Modified-Since matches Modified_Time >= header (default: strictly after)
  --not-modified-204              an empty If-Modified-Since list answers 204 (default: 304 Not Modified)
  --write-error-status 202        HTTP status when every record of a write fails (default 400, data[] envelope)
  --lenient-scopes                skip OAuth scope checks (default: enforced; /org needs ZohoCRM.org.READ)

Using it with Weft: start Weft with WEFT_ZOHO_ACCOUNTS_URL=http://127.0.0.1:<port> (sign-in and tokens then use
this mock only), WEFT_OAUTH_ZOHO_CRM_CLIENT_ID / _CLIENT_SECRET equal to --client-id / --client-secret,
WEFT_PUBLIC_URL equal to --weft-public, and WEFT_ALLOW_PRIVATE_NETWORKS=true so it may call 127.0.0.1. The token
response's api_domain then sends Weft's CRM calls here too. Without WEFT_ZOHO_ACCOUNTS_URL, Weft signs in at
accounts.zoho.com and ignores this mock's accounts-server. Weft refreshes tokens that expire within 120 s, so a
--token-ttl under about 130 makes it refresh before every call; POST /_mock/expire-tokens tests the
401 -> refresh -> retry path instead. Weft's own scopes omit ZohoCRM.org.READ, which /org (its connection test)
needs here; add --lenient-scopes if that check gets in the way.
Unique fields (Email on Leads and Contacts) reject duplicates with DUPLICATE_DATA; unknown fields are ignored.
"""
import argparse
import hashlib
import heapq
import html
import itertools
import json
import math
import os
import re
import secrets
import signal
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from collections import deque
from datetime import date, datetime, timedelta, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

API_PREFIX = "/crm/v8"
CODE_TTL = 120
CONSENT_TTL = 600
MAX_REFRESH_TOKENS = 20
CHANNEL_MAX = timedelta(days=7)
CHANNEL_DEFAULT = timedelta(hours=1)
PAGE_MAX = 200
DISCRETE_PAGING_LIMIT = 2000
RETRY_DELAYS = (1, 2, 4, 8, 16)
SECRET_KEYS = {"token", "access_token", "refresh_token", "client_secret"}
EMAIL_RE = re.compile(r"^[^@\s]+@[^@\s]+\.[^@\s]+$")


def zid(n):
    """An 18-digit numeric string ID, Zoho style."""
    return f"4876876{n:011d}"


USER = {"name": "Mock User", "id": zid(1), "email": "mock.user@example.com"}
ORG = {"company_name": "Mock Org", "id": "60012345678", "domain_name": "org60012345678", "primary_email": "admin@example.com",
       "currency": "US Dollar - USD", "iso_code": "USD", "currency_symbol": "$", "country": "United States",
       "country_code": "US", "zgid": "60012345678", "primary_zuid": "60012345679", "mc_status": False, "type": "production",
       "license_details": {"paid": True, "paid_type": "enterprise", "users_license_purchased": 10}}

# ---------------------------------------------------------------- module and field schema


def F(api, label, data_type="text", **extra):
    return {"api_name": api, "field_label": label, "data_type": data_type, **extra}


PICK, NONE = "picklist", "-None-"
LEAD_SOURCES = [NONE, "Advertisement", "Cold Call", "Employee Referral", "External Referral", "Online Store", "Partner",
                "Public Relations", "Seminar Partner", "Trade Show", "Web Download", "Web Research", "Chat"]
INDUSTRIES = [NONE, "Construction", "Healthcare", "Manufacturing", "Real Estate", "Technology", "Services"]
# Zoho's standard stages, plus the FSM job stages recipe R3 writes (crates/app/src/recipes/crm_fsm.rs).
STAGES = ["Qualification", "Needs Analysis", "Value Proposition", "Identify Decision Makers", "Proposal/Price Quote",
          "Negotiation/Review", "Closed Won", "Closed Lost", "Closed Lost to Competition", "Scheduled", "In Progress",
          "Job Completed"]
MODULES = {
    "Leads": {"singular": "Lead", "name": "Full_Name", "dup": ["Email"], "fields": [
        F("First_Name", "First Name"), F("Last_Name", "Last Name", mandatory=True),
        F("Full_Name", "Full Name", read_only=True), F("Company", "Company"),
        F("Email", "Email", "email", unique=True), F("Phone", "Phone", "phone"), F("Mobile", "Mobile", "phone"),
        F("Lead_Source", "Lead Source", PICK, values=LEAD_SOURCES),
        F("Lead_Status", "Lead Status", PICK, values=[NONE, "Attempted to Contact", "Contact in Future", "Contacted",
                                                       "Junk Lead", "Lost Lead", "Not Contacted", "Pre-Qualified"]),
        F("Industry", "Industry", PICK, values=INDUSTRIES), F("Annual_Revenue", "Annual Revenue", "currency"),
        F("No_of_Employees", "No. of Employees", "integer"), F("Email_Opt_Out", "Email Opt Out", "boolean"),
        F("Description", "Description", "textarea")]},
    "Contacts": {"singular": "Contact", "name": "Full_Name", "dup": ["Email"], "fields": [
        F("First_Name", "First Name"), F("Last_Name", "Last Name", mandatory=True),
        F("Full_Name", "Full Name", read_only=True), F("Email", "Email", "email", unique=True),
        F("Phone", "Phone", "phone"), F("Mobile", "Mobile", "phone"),
        F("Account_Name", "Account Name", "lookup", lookup="Accounts"), F("Title", "Title"),
        F("Department", "Department"), F("Lead_Source", "Lead Source", PICK, values=LEAD_SOURCES),
        F("Date_of_Birth", "Date of Birth", "date"), F("Mailing_City", "Mailing City"),
        F("Mailing_Country", "Mailing Country"), F("Email_Opt_Out", "Email Opt Out", "boolean"),
        F("UHID", "UHID", custom=True), F("Description", "Description", "textarea")]},
    "Accounts": {"singular": "Account", "name": "Account_Name", "dup": ["Account_Name"], "fields": [
        F("Account_Name", "Account Name", mandatory=True), F("Phone", "Phone", "phone"),
        F("Website", "Website", "website"),
        F("Account_Type", "Account Type", PICK, values=[NONE, "Analyst", "Competitor", "Customer", "Distributor",
                                                         "Integrator", "Investor", "Other", "Partner", "Prospect",
                                                         "Reseller", "Supplier", "Vendor"]),
        F("Industry", "Industry", PICK, values=INDUSTRIES), F("Annual_Revenue", "Annual Revenue", "currency"),
        F("Employees", "Employees", "integer"), F("Billing_City", "Billing City"),
        F("Billing_Country", "Billing Country"), F("Description", "Description", "textarea")]},
    "Deals": {"singular": "Deal", "name": "Deal_Name", "dup": ["Deal_Name"], "fields": [
        F("Deal_Name", "Deal Name", mandatory=True), F("Stage", "Stage", PICK, mandatory=True, values=STAGES),
        F("Amount", "Amount", "currency"), F("Closing_Date", "Closing Date", "date"),
        F("Account_Name", "Account Name", "lookup", lookup="Accounts"),
        F("Contact_Name", "Contact Name", "lookup", lookup="Contacts"),
        F("Probability", "Probability (%)", "integer"),
        F("Type", "Type", PICK, values=[NONE, "Existing Business", "New Business"]),
        F("Lead_Source", "Lead Source", PICK, values=LEAD_SOURCES), F("Next_Step", "Next Step"),
        F("Description", "Description", "textarea"), F("FSM_Request_Number", "FSM Request Number", custom=True),
        F("Payment_Status", "Payment Status", PICK, custom=True, values=[NONE, "Unpaid", "Paid"]),
        F("Paid_On", "Paid On", "date", custom=True), F("Forma_Project_ID", "Forma Project ID", custom=True),
        F("Contract_Value", "Contract Value", "currency", custom=True)]},
    "Tasks": {"singular": "Task", "name": "Subject", "dup": [], "fields": [
        F("Subject", "Subject", mandatory=True), F("Due_Date", "Due Date", "date"),
        F("Status", "Status", PICK, values=["Not Started", "Deferred", "In Progress", "Completed", "Waiting for input"]),
        F("Priority", "Priority", PICK, values=["High", "Highest", "Low", "Lowest", "Normal"]),
        F("What_Id", "Related To", "lookup", lookup="$se_module"),
        F("Who_Id", "Contact Name", "lookup", lookup="Contacts"), F("Description", "Description", "textarea")]},
    "Events": {"singular": "Event", "name": "Event_Title", "dup": [], "fields": [
        F("Event_Title", "Title", mandatory=True), F("Start_DateTime", "From", "datetime", mandatory=True),
        F("End_DateTime", "To", "datetime", mandatory=True), F("All_day", "All day", "boolean"), F("Venue", "Venue"),
        F("Who_Id", "Contact Name", "lookup", lookup="Contacts"),
        F("What_Id", "Related To", "lookup", lookup="$se_module"), F("Description", "Description", "textarea")]},
    "Notes": {"singular": "Note", "name": "Note_Title", "dup": [], "fields": [
        F("Note_Title", "Note Title"), F("Note_Content", "Note Content", "textarea", mandatory=True),
        F("Parent_Id", "Parent ID", "lookup", lookup="$se_module")]},
}
SYSTEM_FIELDS = [F("Owner", "Owner", "ownerlookup"), F("Created_By", "Created By", "ownerlookup", read_only=True),
                 F("Modified_By", "Modified By", "ownerlookup", read_only=True),
                 F("Created_Time", "Created Time", "datetime", read_only=True),
                 F("Modified_Time", "Modified Time", "datetime", read_only=True)]
for _spec in MODULES.values():
    _spec["fields"] = _spec["fields"] + SYSTEM_FIELDS
    _spec["by_name"] = {f["api_name"]: f for f in _spec["fields"]}
MODULE_IDS = {m: zid(100 + i) for i, m in enumerate(MODULES)}
HIDDEN_MODULES = ["Home", "Feeds"]  # listed with api_supported: false, as Zoho does
JSON_TYPES = {"integer": "integer", "currency": "double", "double": "double", "boolean": "boolean",
              "lookup": "jsonobject", "ownerlookup": "jsonobject"}
LENGTHS = {"textarea": 32000, "email": 100, "phone": 30, "integer": 9, "currency": 16, "boolean": 5, "date": 20}
NUMERIC = ("integer", "currency", "double")
EVENT_OF = {"insert": "create", "update": "edit", "delete": "delete"}  # Zoho operation -> channel event


def modules_meta():
    out = [{"api_name": m, "module_name": m, "plural_label": m, "singular_label": s["singular"], "id": MODULE_IDS[m],
            "api_supported": True, "generated_type": "default", "creatable": True, "editable": True,
            "deletable": True, "viewable": True, "visible": True} for m, s in MODULES.items()]
    out += [{"api_name": m, "module_name": m, "plural_label": m, "singular_label": m, "id": zid(150 + i),
             "api_supported": False, "generated_type": "default", "creatable": False, "editable": False,
             "deletable": False, "viewable": True, "visible": True} for i, m in enumerate(HIDDEN_MODULES)]
    return out


def fields_meta(module):
    mi, out = list(MODULES).index(module), []
    for fi, fld in enumerate(MODULES[module]["fields"]):
        fid, dt, ro = zid(10_000 + mi * 100 + fi), fld["data_type"], bool(fld.get("read_only"))
        meta = {"api_name": fld["api_name"], "field_label": fld["field_label"], "display_label": fld["field_label"],
                "data_type": dt, "json_type": JSON_TYPES.get(dt, "string"), "id": fid,
                "system_mandatory": bool(fld.get("mandatory")), "custom_field": bool(fld.get("custom")),
                "read_only": ro, "field_read_only": ro, "visible": True, "length": LENGTHS.get(dt, 255),
                "unique": {"casesensitive": "false"} if fld.get("unique") else {}, "lookup": {},
                "pick_list_values": [{"display_value": v, "actual_value": v, "id": zid(20_000 + mi * 1000 + fi * 30 + vi),
                                      "sequence_number": vi + 1, "type": "used"}
                                     for vi, v in enumerate(fld.get("values", []))]}
        if fld.get("lookup") in MODULES:
            meta["lookup"] = {"display_label": fld["field_label"], "api_name": fld["api_name"], "id": fid,
                              "module": {"api_name": fld["lookup"], "id": MODULE_IDS[fld["lookup"]]}}
        out.append(meta)
    return out

# ---------------------------------------------------------------- helpers


class ZohoError(Exception):
    """A Zoho error object {code, details, message, status: "error"}; `http` is used when it ends a request."""

    def __init__(self, code, message, details=None, http=400, headers=None):
        super().__init__(message)
        self.body = {"code": code, "details": details or {}, "message": message, "status": "error"}
        self.http, self.headers = http, headers or {}


def invalid(api_name, path, **extra):
    return ZohoError("INVALID_DATA", "invalid data", {"api_name": api_name, "json_path": path, **extra})


def sha(value):
    return hashlib.sha256(str(value).encode("utf-8", "replace")).hexdigest()


def new_token():
    return f"1000.{secrets.token_hex(16)}.{secrets.token_hex(16)}"


def split_scopes(text):
    return [s for s in re.split(r"[,\s]+", text or "") if s]


def parse_dt(text):
    value = datetime.fromisoformat(str(text).strip())
    if value.tzinfo is None:
        raise ValueError("a datetime needs an offset")
    return value


def norm(value):
    return str(value).strip().casefold() if value is not None else None


def redact(value):
    """A request body for the call log: secrets removed, Weft hook tokens cut out of notify URLs."""
    if isinstance(value, dict):
        return {k: "[redacted]" if str(k).lower() in SECRET_KEYS
                else redact_url(v) if k == "notify_url" and isinstance(v, str) else redact(v) for k, v in value.items()}
    if isinstance(value, list):
        return [redact(v) for v in value]
    return value


def redact_url(url):
    """The URL without query, credentials or long path segments (Weft hook tokens)."""
    p = urllib.parse.urlsplit(url)
    path = "/".join("…" if len(seg) >= 16 else seg for seg in p.path.split("/"))
    return urllib.parse.urlunsplit((p.scheme, p.netloc.rsplit("@", 1)[-1], path, "", ""))


def split_unescaped(text, sep):
    parts, cur, i = [], "", 0
    while i < len(text):
        if text[i] == "\\" and i + 1 < len(text):
            cur += text[i:i + 2]
            i += 2
            continue
        if text[i] == sep:
            parts.append(cur)
            cur = ""
        else:
            cur += text[i]
        i += 1
    return parts + [cur]


def unescape(text):
    return re.sub(r"\\(.)", r"\1", text)


def paging(q):
    try:
        page, per_page = int(q.get("page", "1")), int(q.get("per_page", str(PAGE_MAX)))
    except ValueError:
        raise ZohoError("INVALID_DATA", "invalid data", {"param": "page / per_page", "expected_data_type": "integer"})
    if page < 1:
        raise ZohoError("INVALID_DATA", "invalid data", {"param": "page", "minimum": 1})
    if not 1 <= per_page <= PAGE_MAX:
        raise ZohoError("INVALID_DATA", "invalid data", {"param": "per_page", "maximum": PAGE_MAX})
    if page * per_page > DISCRETE_PAGING_LIMIT:
        raise ZohoError("DISCRETE_PAGINATION_LIMIT_EXCEEDED", "cannot fetch more than 2000 records with page and "
                        "per_page; use page_token", {"limit": DISCRETE_PAGING_LIMIT})
    return page, per_page


def page_of(rows, q, extra_info):
    page, per_page = paging(q)
    start = (page - 1) * per_page
    chunk = rows[start:start + per_page]
    info = {"per_page": per_page, "count": len(chunk), "page": page, **extra_info,
            "more_records": start + per_page < len(rows)}
    return chunk, info

# ---------------------------------------------------------------- search criteria


class Criteria:
    """Zoho search criteria: (field:operator:value) groups joined by and / or, with backslash escapes."""
    OPS = {"equals", "not_equal", "starts_with", "ends_with", "contains", "not_contains", "in", "not_in", "between",
           "not_between", "greater_than", "greater_equal", "less_than", "less_equal"}
    MULTI = {"in", "not_in", "between", "not_between"}

    def __init__(self, mock, module, text):
        self.mock, self.module, self.s, self.i, self.count = mock, module, text, 0, 0
        self.tree = self.expr()
        self.ws()
        if self.i != len(self.s):
            self.fail("unexpected text after the criteria")
        if self.count > 10:
            raise ZohoError("LIMIT_EXCEEDED", "criteria may have at most 10 conditions", {"limit": 10})

    def fail(self, reason, **extra):
        raise ZohoError("INVALID_QUERY", "Invalid query formed", {"reason": reason, **extra})

    def ws(self):
        while self.i < len(self.s) and self.s[self.i].isspace():
            self.i += 1

    def expr(self):
        node = self.group()
        while True:
            self.ws()
            m = re.match(r"(and|or)\b", self.s[self.i:], re.I)
            if not m:
                return node
            self.i += m.end()
            node = (m.group(1).lower(), node, self.group())

    def group(self):
        self.ws()
        if self.i >= len(self.s) or self.s[self.i] != "(":
            self.fail("expected (")
        self.i += 1
        self.ws()
        if self.i < len(self.s) and self.s[self.i] == "(":
            node = self.expr()
            self.ws()
            if self.i >= len(self.s) or self.s[self.i] != ")":
                self.fail("expected )")
            self.i += 1
            return node
        start = self.i
        while self.i < len(self.s) and self.s[self.i] != ")":
            self.i += 2 if self.s[self.i] == "\\" else 1
        if self.i >= len(self.s):
            self.fail("expected )")
        raw = self.s[start:self.i]
        self.i += 1
        return self.condition(raw)

    def condition(self, raw):
        field, sep1, rest = raw.partition(":")
        op, sep2, value = rest.partition(":")
        field, op = field.strip(), op.strip().lower()
        if not (sep1 and sep2 and field):
            self.fail("a condition must be (field:operator:value)")
        fld = {"api_name": "id", "data_type": "text"} if field == "id" else MODULES[self.module]["by_name"].get(field)
        if fld is None:
            self.fail("the api name given seems to be invalid", api_name=field)
        if op not in self.OPS:
            self.fail("invalid operator found", api_name=field, operator=op)
        values = [unescape(v) for v in split_unescaped(value, ",")] if op in self.MULTI else [unescape(value)]
        if op in ("between", "not_between") and len(values) != 2:
            self.fail("between needs two values", api_name=field)
        self.count += 1
        return ("cond", fld, op, values)

    def matches(self, rec, node=None):
        node = node or self.tree
        if node[0] == "and":
            return self.matches(rec, node[1]) and self.matches(rec, node[2])
        if node[0] == "or":
            return self.matches(rec, node[1]) or self.matches(rec, node[2])
        _, fld, op, values = node
        actual, dt = rec.get(fld["api_name"]), fld["data_type"]
        if values == ["${EMPTY}"] and op in ("equals", "not_equal"):
            return (actual in (None, "")) == (op == "equals")
        if dt in ("lookup", "ownerlookup"):
            texts = {norm(actual), norm(self.mock.lookup_name(fld, actual, rec))} - {None}
            hit = any(norm(v) in texts for v in values)
            if op == "starts_with":
                return any(t.startswith(norm(values[0])) for t in texts)
            return hit if op in ("equals", "in") else not hit if op in ("not_equal", "not_in") else False
        conv = (lambda v: float(v)) if dt in NUMERIC else (lambda v: parse_dt(v)) if dt == "datetime" else norm
        try:
            a, b = (conv(actual) if actual is not None else None), [conv(v) for v in values]
        except ValueError:
            a, b = norm(actual), [norm(v) for v in values]
        try:
            return self.compare(op, a, b)
        except TypeError:
            return False

    @staticmethod
    def compare(op, a, b):
        if op == "equals":
            return a == b[0]
        if op == "not_equal":
            return a != b[0]
        if op in ("in", "not_in"):
            return (a in b) == (op == "in")
        if a is None:
            return op in ("not_contains", "not_between")
        text, first = str(a), str(b[0])
        table = {"starts_with": lambda: text.startswith(first), "ends_with": lambda: text.endswith(first),
                 "contains": lambda: first in text, "not_contains": lambda: first not in text,
                 "greater_than": lambda: a > b[0], "greater_equal": lambda: a >= b[0],
                 "less_than": lambda: a < b[0], "less_equal": lambda: a <= b[0],
                 "between": lambda: b[0] <= a <= b[1], "not_between": lambda: not b[0] <= a <= b[1]}
        return table[op]()

# ---------------------------------------------------------------- notification delivery


class Notifier(threading.Thread):
    """Posts queued notifications in the background, retrying failures with backoff; never through a proxy."""

    def __init__(self, mock):
        super().__init__(name="notifier", daemon=True)
        self.mock, self.cond, self.heap, self.seq, self.stopping = mock, threading.Condition(), [], itertools.count(), False
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def submit(self, url, payload):
        with self.cond:
            heapq.heappush(self.heap, (time.monotonic(), next(self.seq), {"url": url, "payload": payload, "attempt": 0}))
            self.cond.notify()

    def stop(self):
        with self.cond:
            self.stopping = True
            self.cond.notify()

    def run(self):
        while True:
            with self.cond:
                while not self.stopping and (not self.heap or self.heap[0][0] > time.monotonic()):
                    self.cond.wait(self.heap[0][0] - time.monotonic() if self.heap else None)
                if self.stopping:
                    return
                job = heapq.heappop(self.heap)[2]
            self.deliver(job)

    def deliver(self, job):
        job["attempt"] += 1
        p, status, error = job["payload"], None, None
        req = urllib.request.Request(job["url"], data=json.dumps(p).encode(), method="POST", headers={
            "Content-Type": "application/json", "User-Agent": "MockZohoCRM-Notifications/1.0"})
        try:
            with self.opener.open(req, timeout=10) as resp:
                status = resp.status
                resp.read(65536)
        except urllib.error.HTTPError as e:
            status = e.code
            e.close()
        except (urllib.error.URLError, OSError, ValueError) as e:
            error = str(getattr(e, "reason", e))[:200]
        ok = status is not None and 200 <= status < 300
        retry = not ok and (status is None or status in (408, 429) or status >= 500) and job["attempt"] <= len(RETRY_DELAYS)
        self.mock.deliveries.append({"time": self.mock.fmt(time.time()), "channel_id": p["channel_id"],
                                     "notify_url": redact_url(job["url"]), "module": p["module"],
                                     "operation": p["operation"], "ids": p["ids"], "attempt": job["attempt"],
                                     "status": status, "error": error, "will_retry": retry})
        sys.stderr.write(f"notify channel={p['channel_id']} {p['module']}.{p['operation']} ids={len(p['ids'])} "
                         f"{redact_url(job['url'])} -> {status or error} (attempt {job['attempt']})\n")
        if retry:
            with self.cond:
                due = time.monotonic() + RETRY_DELAYS[job["attempt"] - 1]
                heapq.heappush(self.heap, (due, next(self.seq), job))
                self.cond.notify()

# ---------------------------------------------------------------- state: records, tokens, channels


class Mock:
    def __init__(self, args, port):
        self.args, self.base = args, f"http://127.0.0.1:{port}"
        self.redirect_uri = args.weft_public.rstrip("/") + "/oauth/callback"
        sign, hours, minutes = re.fullmatch(r"([+-])(\d\d):(\d\d)", args.utc_offset).groups()
        self.tz = timezone((1 if sign == "+" else -1) * timedelta(hours=int(hours), minutes=int(minutes)))
        self.lock = threading.RLock()
        self.records = {m: {} for m in MODULES}
        self.channels, self.refresh_tokens, self.access_tokens, self.codes, self.consents = {}, {}, {}, {}, {}
        self.next_id = 1_000_001
        self.calls, self.deliveries = deque(maxlen=2000), deque(maxlen=500)
        self.usage = {"minute": None, "minute_count": 0, "day": None, "day_count": 0, "total": 0}
        self.notifier = Notifier(self)
        if not self.load() and not args.no_seed:
            self.seed()

    def fmt(self, when):
        value = datetime.fromtimestamp(when, self.tz) if isinstance(when, (int, float)) else when.astimezone(self.tz)
        return value.replace(microsecond=0).isoformat()

    # persistence -------------------------------------------------------------------------------------------
    def load(self):
        path = self.args.state_file
        if not path or not os.path.exists(path):
            return False
        with open(path, encoding="utf-8") as fh:
            data = json.load(fh)
        for module in MODULES:
            self.records[module] = data.get("records", {}).get(module, {})
        self.channels = data.get("channels", {})
        self.refresh_tokens, self.access_tokens = data.get("refresh_tokens", {}), data.get("access_tokens", {})
        self.next_id = data.get("next_id", self.next_id)
        return True

    def save(self):
        """Writes the state file (tokens only as SHA-256 hashes). Caller holds the lock."""
        if not self.args.state_file:
            return
        data = {"version": 1, "next_id": self.next_id, "records": self.records, "channels": self.channels,
                "refresh_tokens": self.refresh_tokens, "access_tokens": self.access_tokens}
        tmp = self.args.state_file + ".tmp"
        with os.fdopen(os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600), "w", encoding="utf-8") as fh:
            json.dump(data, fh)
        os.replace(tmp, self.args.state_file)

    def seed(self):
        with self.lock:
            def add(module, values):
                result = self.write(module, [values], "insert", notify=False)[1]["data"][0]
                if result["status"] != "success":
                    raise RuntimeError(f"seed {module}: {result}")
                return result["details"]["id"]
            soon = (date.today() + timedelta(days=30)).isoformat()
            acme = add("Accounts", {"Account_Name": "Acme Builders", "Industry": "Construction", "Account_Type": "Customer",
                                    "Phone": "+91 20 5555 0100", "Website": "https://acme.example", "Billing_City": "Pune"})
            globex = add("Accounts", {"Account_Name": "Globex Health", "Industry": "Healthcare",
                                      "Account_Type": "Prospect", "Billing_City": "Austin"})
            ann = add("Contacts", {"First_Name": "Ann", "Last_Name": "Lee", "Email": "ann.lee@acme.example",
                                   "Mobile": "+91 98765 43210", "Account_Name": {"id": acme}, "Mailing_City": "Pune"})
            raj = add("Contacts", {"First_Name": "Raj", "Last_Name": "Patel", "Email": "raj.patel@globex.example",
                                   "Account_Name": {"id": globex}, "Mailing_City": "Austin"})
            add("Leads", {"First_Name": "Maria", "Last_Name": "Garcia", "Company": "Initech",
                          "Email": "maria@initech.example", "Lead_Source": "Web Download", "Lead_Status": "Not Contacted"})
            add("Leads", {"First_Name": "Ken", "Last_Name": "Tanaka", "Company": "Umbrella Clinics",
                          "Email": "ken@umbrella.example", "Lead_Source": "Trade Show", "Lead_Status": "Contacted"})
            deal = add("Deals", {"Deal_Name": "Acme tower fit-out", "Stage": "Qualification", "Amount": 120000,
                                 "Closing_Date": soon, "Account_Name": {"id": acme}, "Contact_Name": {"id": ann},
                                 "Probability": 10, "Type": "New Business"})
            add("Deals", {"Deal_Name": "Globex clinic HVAC", "Stage": "Negotiation/Review", "Amount": 45000,
                          "Closing_Date": soon, "Account_Name": {"id": globex}, "Contact_Name": {"id": raj},
                          "Probability": 70, "Type": "New Business"})
            add("Tasks", {"Subject": "Call Ann about the fit-out", "Due_Date": soon, "Status": "Not Started",
                          "Priority": "High", "What_Id": {"id": deal}, "$se_module": "Deals", "Who_Id": {"id": ann}})
            self.save()

    # OAuth -------------------------------------------------------------------------------------------------
    def authorize_problem(self, q):
        if q.get("client_id") != self.args.client_id:
            return "Invalid client: this client_id is not registered with the mock."
        if q.get("redirect_uri") != self.redirect_uri:
            return f"Invalid redirect URI: the registered one is {self.redirect_uri}."
        if q.get("response_type") != "code":
            return "Unsupported response_type: only code is supported."
        if not split_scopes(q.get("scope")):
            return "Invalid OAuth scope: a scope is required."
        return None

    def park_consent(self, q):
        rid = secrets.token_urlsafe(24)
        with self.lock:
            now = time.time()
            self.consents = {k: v for k, v in self.consents.items() if v["at"] > now - CONSENT_TTL}
            self.consents[rid] = {**q, "at": now}
        return rid

    def take_consent(self, rid):
        with self.lock:
            req = self.consents.pop(rid, None)
        return req if req and req["at"] > time.time() - CONSENT_TTL else None

    def consent_redirect(self, req, accept):
        params = {}
        if accept:
            code = new_token()
            with self.lock:
                self.codes = {k: v for k, v in self.codes.items() if v["expires"] > time.time()}
                self.codes[sha(code)] = {"client_id": req["client_id"], "redirect_uri": req["redirect_uri"],
                                         "scopes": split_scopes(req["scope"]), "expires": time.time() + CODE_TTL,
                                         "offline": req.get("access_type") == "offline",
                                         "consent": req.get("prompt") == "consent"}
            params["code"] = code
        else:
            params["error"] = "access_denied"
        if req.get("state") is not None:
            params["state"] = req["state"]
        if accept:
            params.update({"location": "us", "accounts-server": self.base})
        sep = "&" if "?" in req["redirect_uri"] else "?"
        return req["redirect_uri"] + sep + urllib.parse.urlencode(params)

    def issue_access(self, client_id, scopes, refresh_hash):
        now, token = time.time(), new_token()
        self.access_tokens = {k: v for k, v in self.access_tokens.items() if v["expires"] > now}
        self.access_tokens[sha(token)] = {"client_id": client_id, "scopes": scopes, "refresh": refresh_hash,
                                          "expires": now + self.args.token_ttl}
        return token

    def token(self, p):
        with self.lock:
            secret_ok = secrets.compare_digest(p.get("client_secret", "").encode(), self.args.client_secret.encode())
            if p.get("client_id") != self.args.client_id or not secret_ok:
                return {"error": "invalid_client"}
            refresh = None
            if p.get("grant_type") == "authorization_code":
                entry = self.codes.pop(sha(p.get("code", "")), None)
                if (not entry or entry["expires"] < time.time() or entry["client_id"] != p["client_id"]
                        or entry["redirect_uri"] != p.get("redirect_uri")):
                    return {"error": "invalid_code"}
                scopes = entry["scopes"]
                has_refresh = any(r["client_id"] == p["client_id"] for r in self.refresh_tokens.values())
                if entry["offline"] and (entry["consent"] or not has_refresh):
                    refresh = new_token()
                    self.refresh_tokens[sha(refresh)] = {"client_id": p["client_id"], "scopes": scopes,
                                                         "created": time.time()}
                    self.prune_refresh_tokens(p["client_id"])
                access = self.issue_access(p["client_id"], scopes, sha(refresh) if refresh else None)
            elif p.get("grant_type") == "refresh_token":
                entry = self.refresh_tokens.get(sha(p.get("refresh_token", "")))
                if entry is None or entry["client_id"] != p["client_id"]:
                    return {"error": "invalid_code"}
                scopes = entry["scopes"]
                access = self.issue_access(p["client_id"], scopes, sha(p["refresh_token"]))
            else:
                return {"error": "unsupported_grant_type"}
            self.save()
        out = {"access_token": access}
        if refresh:
            out["refresh_token"] = refresh
        out.update({"scope": " ".join(scopes), "api_domain": self.base, "token_type": "Bearer",
                    "expires_in": self.args.token_ttl})
        return out

    def prune_refresh_tokens(self, client_id):
        """Zoho keeps at most 20 refresh tokens per user and client; the oldest go first."""
        mine = sorted((v["created"], k) for k, v in self.refresh_tokens.items() if v["client_id"] == client_id)
        for _, key in mine[:-MAX_REFRESH_TOKENS]:
            self.drop_refresh(key)

    def drop_refresh(self, key):
        self.refresh_tokens.pop(key, None)
        self.access_tokens = {k: v for k, v in self.access_tokens.items() if v.get("refresh") != key}

    def revoke(self, p):
        key = sha(p.get("token", ""))
        with self.lock:
            if key in self.refresh_tokens:
                self.drop_refresh(key)
            else:
                self.access_tokens.pop(key, None)
            self.save()
        return {"status": "success"}

    def expire_tokens(self):
        with self.lock:
            for entry in self.access_tokens.values():
                entry["expires"] = 0
            self.save()
            return len(self.access_tokens)

    def authenticate(self, header):
        scheme, _, token = (header or "").partition(" ")
        if scheme != "Zoho-oauthtoken" or not token.strip():
            return None
        entry = self.access_tokens.get(sha(token.strip()))
        return entry if entry and entry["expires"] > time.time() else None

    def scope_ok(self, granted, parts, method):
        if self.args.lenient_scopes or not parts:
            return True
        have = {s.lower() for s in granted}
        op = {"GET": "read", "POST": "create", "PUT": "update", "PATCH": "update", "DELETE": "delete"}.get(method, "read")
        ops = {op, "write"} if op in ("create", "update") else {op}
        if parts[0] == "org":
            want = {"zohocrm.org.all"} | {f"zohocrm.org.{o}" for o in ops}
        elif parts[0] == "settings":
            sub = parts[1].lower() if len(parts) > 1 else ""
            want = {"zohocrm.settings.all", f"zohocrm.settings.{sub}.all"} | {
                f"zohocrm.settings{s}.{o}" for o in ops for s in ("", f".{sub}")}
        elif parts[0] == "actions":
            want = {"zohocrm.notifications.all"} | {f"zohocrm.notifications.{o}" for o in ops}
        else:
            module = ("notes" if len(parts) == 3 and parts[2] == "Notes" else parts[0]).lower()
            want = {"zohocrm.modules.all", f"zohocrm.modules.{module}.all"} | {
                f"zohocrm.modules{s}.{o}" for o in ops for s in ("", f".{module}")}
        return bool(have & want)

    def take_credit(self):
        """Counts one API call; returns (retry_after_secs, code) when a limit is reached. Caller holds the lock."""
        now, u = time.time(), self.usage
        minute, day = int(now // 60), datetime.fromtimestamp(now, timezone.utc).date().isoformat()
        if u["minute"] != minute:
            u["minute"], u["minute_count"] = minute, 0
        if u["day"] != day:
            u["day"], u["day_count"] = day, 0
        if self.args.limit_per_minute and u["minute_count"] >= self.args.limit_per_minute:
            return max(1, math.ceil(60 - now % 60)), "TOO_MANY_REQUESTS"
        if self.args.limit_per_day and u["day_count"] >= self.args.limit_per_day:
            return max(1, math.ceil(86400 - now % 86400)), "LIMIT_REACHED"
        u["minute_count"] += 1
        u["day_count"] += 1
        u["total"] += 1
        return None

    def usage_view(self):
        u, now = self.usage, time.time()
        this_minute = u["minute"] == int(now // 60)
        return {"minute": {"count": u["minute_count"] if this_minute else 0, "limit": self.args.limit_per_minute or None},
                "day": {"date": u["day"], "count": u["day_count"], "limit": self.args.limit_per_day or None},
                "total": u["total"]}

    def log_call(self, entry):
        self.calls.append({"time": self.fmt(time.time()), **entry})

    # records -----------------------------------------------------------------------------------------------
    def lookup_target(self, fld, rec):
        target = fld.get("lookup")
        return rec.get("$se_module") if target == "$se_module" else target

    def lookup_name(self, fld, value, rec):
        if fld["data_type"] == "ownerlookup":
            return USER["name"] if value == USER["id"] else None
        target = self.lookup_target(fld, rec)
        found = self.records.get(target, {}).get(value) if target else None
        return found.get(MODULES[target]["name"]) if found else None

    def render(self, module, rec, only=None):
        out = {"id": rec["id"]}
        for fld in MODULES[module]["fields"]:
            api, value = fld["api_name"], rec.get(fld["api_name"])
            if only is not None and api not in only:
                continue
            if value is not None and fld["data_type"] == "ownerlookup":
                value = dict(USER)
            elif value is not None and fld["data_type"] == "lookup":
                name = self.lookup_name(fld, value, rec)
                target = self.lookup_target(fld, rec)
                value = {"name": name, "id": value} if value in self.records.get(target, {}) else None
            out[api] = value
        if "$se_module" in rec:
            out["$se_module"] = rec["$se_module"]
        return out

    def convert(self, fld, value, values, existing, path):
        """The stored form of one field value, or ZohoError(INVALID_DATA)."""
        api, dt = fld["api_name"], fld["data_type"]
        if value is None or value == "" or value == NONE:
            return None
        if dt in ("text", "textarea", "phone", "website", "picklist") and isinstance(value, str):
            return value
        if dt == "email" and isinstance(value, str) and EMAIL_RE.match(value):
            return value
        if dt in NUMERIC and not isinstance(value, bool):
            try:
                number = float(value) if isinstance(value, str) else value
                if isinstance(number, (int, float)) and math.isfinite(number):
                    if dt == "integer" and float(number).is_integer():
                        return int(number)
                    if dt != "integer":  # "12000" from a template is stored as 12000, as Zoho returns it
                        whole = isinstance(value, str) and float(number).is_integer() and "." not in value
                        return int(number) if whole else number
            except ValueError:
                pass
        if dt == "boolean" and isinstance(value, bool):
            return value
        if dt == "date" and isinstance(value, str) and re.fullmatch(r"\d{4}-\d\d-\d\d", value):
            try:
                return date.fromisoformat(value).isoformat()
            except ValueError:
                pass
        if dt == "datetime" and isinstance(value, str):
            try:
                return self.fmt(parse_dt(value))
            except ValueError:
                pass
        if dt in ("lookup", "ownerlookup"):
            ref = value.get("id") if isinstance(value, dict) else value
            ref = str(ref) if isinstance(ref, int) and not isinstance(ref, bool) else ref
            if dt == "ownerlookup" and ref == USER["id"]:
                return ref
            target = self.lookup_target(fld, {**(existing or {}), **values})
            if dt == "lookup" and not target:
                raise ZohoError("MANDATORY_NOT_FOUND", "required field not found",
                                {"api_name": "$se_module", "json_path": path.rsplit(".", 1)[0] + ".$se_module"})
            if dt == "lookup" and isinstance(ref, str) and ref in self.records.get(target, {}):
                return ref
            raise invalid(api, path + (".id" if isinstance(value, dict) else ""))
        expected = {"email": "email", "boolean": "boolean", "date": "date", "datetime": "datetime"}.get(
            dt, "integer" if dt == "integer" else "double" if dt in NUMERIC else "string")
        raise invalid(api, path, expected_data_type=expected)

    def clean(self, module, item, idx, existing):
        spec, out = MODULES[module], {}
        if "$se_module" in item and any(f.get("lookup") == "$se_module" for f in spec["fields"]):
            if item["$se_module"] not in MODULES or item["$se_module"] == "Notes":
                raise invalid("$se_module", f"$.data[{idx}].$se_module")
            out["$se_module"] = item["$se_module"]
        for key, value in item.items():
            fld = spec["by_name"].get(key)
            if fld is None or fld.get("read_only"):
                continue  # unknown and read-only fields are ignored, as Zoho does
            out[key] = self.convert(fld, value, out, existing, f"$.data[{idx}].{key}")
        return out

    def finish(self, module, rec, idx, self_id):
        """Derived fields, then the mandatory and unique checks."""
        if module in ("Leads", "Contacts"):
            rec["Full_Name"] = " ".join(p for p in (rec.get("First_Name"), rec.get("Last_Name")) if p) or None
        for fld in MODULES[module]["fields"]:
            api = fld["api_name"]
            if fld.get("mandatory") and rec.get(api) is None:
                raise ZohoError("MANDATORY_NOT_FOUND", "required field not found",
                                {"api_name": api, "json_path": f"$.data[{idx}].{api}"})
            if fld.get("unique") and rec.get(api) is not None:
                for other_id, other in self.records[module].items():
                    if other_id != self_id and norm(other.get(api)) == norm(rec[api]):
                        raise ZohoError("DUPLICATE_DATA", "duplicate data", {
                            "api_name": api, "json_path": f"$.data[{idx}].{api}", "more_records": False,
                            "duplicate_record": {"id": other_id, "module": {"api_name": module, "id": MODULE_IDS[module]},
                                                 "Owner": {"name": USER["name"], "id": USER["id"]}}})

    def insert_one(self, module, item, idx):
        values = self.clean(module, item, idx, None)
        rec = {f["api_name"]: None for f in MODULES[module]["fields"]}
        rec.update(values)
        now = self.fmt(time.time())
        rec.update({"Owner": values.get("Owner") or USER["id"], "Created_By": USER["id"], "Modified_By": USER["id"],
                    "Created_Time": now, "Modified_Time": now})
        self.finish(module, rec, idx, None)
        rec["id"] = zid(self.next_id)
        self.next_id += 1
        self.records[module][rec["id"]] = rec
        return rec

    def update_one(self, module, rid, item, idx):
        old = self.records[module].get(rid)
        if old is None:
            raise ZohoError("INVALID_DATA", "the id given seems to be invalid",
                            {"api_name": "id", "id": rid, "json_path": f"$.data[{idx}].id"})
        values = self.clean(module, item, idx, old)
        new = {**old, **values}
        self.finish(module, new, idx, rid)
        changed = [k for k in values if not k.startswith("$") and old.get(k) != new.get(k)]
        new.update({"Modified_Time": self.fmt(time.time()), "Modified_By": USER["id"]})
        self.records[module][rid] = new
        return new, changed

    def find_duplicate(self, module, item, idx, fields):
        values = self.clean(module, item, idx, None)
        for api in fields:
            if values.get(api) is None:
                continue
            for rid, rec in self.records[module].items():
                if norm(rec.get(api)) == norm(values[api]):
                    return rid, api
        return None, None

    @staticmethod
    def success(rec, message, extra=None):
        who = {"name": USER["name"], "id": USER["id"]}
        return {"code": "SUCCESS", **(extra or {}), "details": {
            "Modified_Time": rec["Modified_Time"], "Modified_By": who, "Created_Time": rec["Created_Time"],
            "id": rec["id"], "Created_By": who}, "message": message, "status": "success"}

    def write(self, module, items, mode, record_id=None, dup_fields=None, notify=True):
        """insert / update / upsert each item; returns (http_status, body). Caller holds the lock."""
        results, events, inserted = [], [], 0
        for idx, item in enumerate(items):
            try:
                if not isinstance(item, dict):
                    raise invalid("data", f"$.data[{idx}]", expected_data_type="jsonobject")
                match, field = (None, None)
                if mode == "upsert":
                    match, field = self.find_duplicate(module, item, idx, dup_fields)
                if mode == "insert" or (mode == "upsert" and match is None):
                    rec = self.insert_one(module, item, idx)
                    extra = {"duplicate_field": None, "action": "insert"} if mode == "upsert" else None
                    results.append(self.success(rec, "record added", extra))
                    events.append(("insert", module, rec["id"], None))
                    inserted += 1
                    continue
                rid = match or record_id or str(item.get("id") or "")
                if not rid:
                    raise ZohoError("MANDATORY_NOT_FOUND", "required field not found",
                                    {"api_name": "id", "json_path": f"$.data[{idx}].id"})
                rec, changed = self.update_one(module, rid, item, idx)
                extra = {"duplicate_field": field, "action": "update"} if mode == "upsert" else None
                results.append(self.success(rec, "record updated", extra))
                events.append(("update", module, rid, changed))
            except ZohoError as e:
                results.append(e.body)
        ok = sum(1 for r in results if r["status"] == "success")
        if events:
            self.save()
            if notify:
                self.notify(events)
        if ok == len(results):
            return (201 if inserted == len(results) else 200), {"data": results}
        return (self.args.write_error_status if ok == 0 else 207), {"data": results}

    def delete(self, module, ids):
        results, events = [], []
        for rid in ids:
            if self.records[module].pop(rid, None) is None:
                results.append(ZohoError("INVALID_DATA", "the id given seems to be invalid", {"id": rid}).body)
            else:
                results.append({"code": "SUCCESS", "details": {"id": rid}, "message": "record deleted",
                                "status": "success"})
                events.append(("delete", module, rid, None))
        if events:
            self.save()
            self.notify(events)
        ok = len(events)
        return (200 if ok == len(ids) else self.args.write_error_status if ok == 0 else 207), {"data": results}

    # notifications -----------------------------------------------------------------------------------------
    def notify(self, events):
        """One notification per (module, operation) of this change, to every matching unexpired channel."""
        groups, now = {}, time.time()
        for op, module, rid, changed in events:
            groups.setdefault((module, op), []).append((rid, changed))
        for (module, op), items in groups.items():
            event = f"{module}.{EVENT_OF[op]}"
            for ch in self.channels.values():
                if ch["expires_ts"] <= now or not ({event, f"{module}.all"} & set(ch["events"])):
                    continue
                payload = {"server_time": int(now * 1000), "query_params": {}, "module": module,
                           "resource_uri": f"{self.base}{API_PREFIX}/{module}", "ids": [rid for rid, _ in items],
                           "operation": op, "channel_id": ch["channel_id"]}
                if op == "update":
                    payload["affected_fields"] = [{rid: changed} for rid, changed in items]
                    if ch.get("return_affected_field_values"):
                        payload["affected_values"] = [
                            {"record_id": rid, "values": {k: self.render(module, self.records[module][rid])[k]
                                                          for k in changed}}
                            for rid, changed in items if rid in self.records[module]]
                if ch.get("token"):
                    payload["token"] = ch["token"]
                self.notifier.submit(ch["notify_url"], payload)

    def channel_resources(self, ch):
        modules = list(dict.fromkeys(e.split(".", 1)[0] for e in ch["events"]))
        return [{"channel_expiry": ch["channel_expiry"], "resource_uri": f"{self.base}{API_PREFIX}/{m}",
                 "resource_id": MODULE_IDS[m], "resource_name": m, "channel_id": ch["channel_id"]} for m in modules]

    def channel_from(self, method, item, idx):
        path = f"$.watch[{idx}]"
        if not isinstance(item, dict):
            raise invalid("watch", path, expected_data_type="jsonobject")
        cid = item.get("channel_id")
        if isinstance(cid, bool) or not (isinstance(cid, int) or (isinstance(cid, str) and cid.isdigit())):
            code = "MANDATORY_NOT_FOUND" if cid is None else "INVALID_DATA"
            raise ZohoError(code, "invalid channel_id", {"api_name": "channel_id", "json_path": f"{path}.channel_id"})
        cid = str(cid)
        existing = self.channels.get(cid)
        if method in ("PATCH", "PUT") and existing is None:
            raise ZohoError("INVALID_DATA", "the channel id given seems to be invalid",
                            {"api_name": "channel_id", "json_path": f"{path}.channel_id"})
        ch = dict(existing) if method == "PATCH" else {"channel_id": cid, "token": None,
                                                         "return_affected_field_values": False,
                                                         "notify_on_related_action": False}
        if "events" in item or method != "PATCH":
            events = item.get("events")
            ok = isinstance(events, list) and events and all(
                isinstance(e, str) and e.count(".") == 1 and e.split(".")[0] in MODULES
                and e.split(".")[1] in ("create", "edit", "delete", "all") for e in events)
            if not ok:
                raise invalid("events", f"{path}.events")
            ch["events"] = list(events)
        if "notify_url" in item or method != "PATCH":
            url = item.get("notify_url")
            p = urllib.parse.urlsplit(url) if isinstance(url, str) else None
            if not p or p.scheme not in ("http", "https") or not p.netloc:
                raise invalid("notify_url", f"{path}.notify_url")
            ch["notify_url"] = url
        if "token" in item:
            if item["token"] is not None and (not isinstance(item["token"], str) or len(item["token"]) > 50):
                raise invalid("token", f"{path}.token", maximum_length=50)
            ch["token"] = item["token"]
        for flag in ("return_affected_field_values", "notify_on_related_action"):
            if flag in item:
                if not isinstance(item[flag], bool):
                    raise invalid(flag, f"{path}.{flag}", expected_data_type="boolean")
                ch[flag] = item[flag]
        now = datetime.now(timezone.utc)
        if item.get("channel_expiry") is not None:
            try:
                expiry = parse_dt(item["channel_expiry"])
            except (ValueError, TypeError):
                raise invalid("channel_expiry", f"{path}.channel_expiry", expected_data_type="datetime")
            if expiry <= now:
                raise invalid("channel_expiry", f"{path}.channel_expiry", reason="must be in the future")
            expiry = min(expiry, now + CHANNEL_MAX)  # Zoho allows at most one week
        elif "expires_ts" in ch:
            expiry = datetime.fromtimestamp(ch["expires_ts"], timezone.utc)
        else:
            expiry = now + CHANNEL_DEFAULT
        ch["expires_ts"], ch["channel_expiry"] = expiry.timestamp(), self.fmt(expiry)
        return ch

    def watch(self, method, q, body):
        if method == "GET":
            views = [{"notify_on_related_action": c["notify_on_related_action"], "channel_expiry": c["channel_expiry"],
                      "resource_uri": r["resource_uri"], "resource_id": r["resource_id"], "notify_url": c["notify_url"],
                      "resource_name": r["resource_name"], "channel_id": c["channel_id"], "events": c["events"],
                      "token": c["token"]} for c in self.channels.values() for r in self.channel_resources(c)[:1]]
            return (200, {"watch": views}) if views else (204, None)
        if method == "DELETE":
            ids = [i.strip() for i in q.get("channel_ids", "").split(",") if i.strip()]
            if not ids:
                raise ZohoError("REQUIRED_PARAM_MISSING", "One of the expected parameter is missing",
                                {"param": "channel_ids"})
            out = []
            for cid in ids:
                ch = self.channels.pop(cid, None)
                out.append({"code": "SUCCESS", "details": {"events": self.channel_resources(ch) if ch else []},
                            "message": "Successfully unsubscribed from actions-watch", "status": "success"})
            self.save()
            return 200, {"watch": out}
        items = body.get("watch") if isinstance(body, dict) else None
        if not isinstance(items, list) or not items:
            raise ZohoError("REQUIRED_PARAM_MISSING", "One of the expected parameter is missing", {"param": "watch"})
        out, ok = [], 0
        message = ("Successfully subscribed for actions-watch of the given module" if method == "POST"
                   else "Successfully updated the notification details")
        for idx, item in enumerate(items):
            try:
                ch = self.channel_from(method, item, idx)
                self.channels[ch["channel_id"]] = ch
                out.append({"code": "SUCCESS", "details": {"events": self.channel_resources(ch)}, "message": message,
                            "status": "success"})
                ok += 1
            except ZohoError as e:
                out.append(e.body)
        if ok:
            self.save()
        return (200 if ok == len(out) else 400 if ok == 0 else 207), {"watch": out}

    # CRM routing -------------------------------------------------------------------------------------------
    def crm_route(self, method, parts, q, body, headers):
        """Returns (status, body) for /crm/v8/<parts>. Caller holds the lock."""
        if parts == ["org"] and method == "GET":
            return 200, {"org": [{**ORG, "time_zone": "Asia/Kolkata" if self.args.utc_offset == "+05:30"
                                  else "UTC" if self.args.utc_offset == "+00:00" else self.args.utc_offset}]}
        if parts == ["settings", "modules"] and method == "GET":
            return 200, {"modules": modules_meta()}
        if parts == ["settings", "fields"] and method == "GET":
            if not q.get("module"):
                raise ZohoError("REQUIRED_PARAM_MISSING", "One of the expected parameter is missing", {"param": "module"})
            return 200, {"fields": fields_meta(self.module(q["module"], "module"))}
        if parts == ["actions", "watch"] and method in ("GET", "POST", "PUT", "PATCH", "DELETE"):
            return self.watch(method, q, body)
        if not parts or parts[0] in ("org", "settings", "actions"):
            raise self.bad_url(parts)
        module, rest = self.module(parts[0], None), parts[1:]
        if not rest and method == "GET":
            return self.list_records(module, q, headers)
        if not rest and method in ("POST", "PUT"):
            return self.write(module, data_items(body), "insert" if method == "POST" else "update")
        if not rest and method == "DELETE":
            ids = [i for i in q.get("ids", "").split(",") if i]
            if not ids:
                raise ZohoError("REQUIRED_PARAM_MISSING", "One of the expected parameter is missing", {"param": "ids"})
            if len(ids) > 100:
                raise ZohoError("LIMIT_EXCEEDED", "limit exceeded", {"param": "ids", "limit": 100})
            return self.delete(module, ids)
        if rest == ["upsert"] and method == "POST":
            return self.write(module, data_items(body), "upsert", dup_fields=self.dup_fields(module, body))
        if rest == ["search"] and method == "GET":
            return self.search(module, q)
        if len(rest) == 1 and rest[0].isdigit():
            if method == "GET":
                rec = self.records[module].get(rest[0])
                only = set(q["fields"].split(",")) if q.get("fields") else None
                return (200, {"data": [self.render(module, rec, only)]}) if rec else (204, None)
            if method == "PUT":
                return self.write(module, data_items(body)[:1], "update", record_id=rest[0])
            if method == "DELETE":
                return self.delete(module, [rest[0]])
        if len(rest) == 2 and rest[0].isdigit() and rest[1] == "Notes" and method == "POST":
            notes = [{**n, "Parent_Id": {"id": rest[0]}, "$se_module": module} if isinstance(n, dict) else n
                     for n in data_items(body)]
            return self.write("Notes", notes, "insert")
        raise self.bad_url(parts, method)

    @staticmethod
    def bad_url(parts, method=None):
        rest = parts[1:]
        known = not rest or rest in (["upsert"], ["search"]) or (rest[0].isdigit() and rest[1:] in ([], ["Notes"]))
        if method and parts and parts[0] in MODULES and known:
            return ZohoError("INVALID_REQUEST_METHOD", "The http request method type is not a valid one")
        return ZohoError("INVALID_URL_PATTERN", "Please check if the URL trying to access is a correct one", http=404)

    @staticmethod
    def module(name, param):
        if name in MODULES:
            return name
        details = {"param": param} if param else {"resource_path_index": 0}
        raise ZohoError("INVALID_MODULE", "the module name given seems to be invalid", details)

    def dup_fields(self, module, body):
        spec = MODULES[module]
        default = spec["dup"] + [f["api_name"] for f in spec["fields"] if f.get("unique") and f["api_name"] not in spec["dup"]]
        given = body.get("duplicate_check_fields") if isinstance(body, dict) else None
        if given is None:
            return default
        if not isinstance(given, list) or not all(isinstance(f, str) and f in spec["by_name"] for f in given):
            raise ZohoError("INVALID_DATA", "invalid data", {"api_name": "duplicate_check_fields"})
        return given

    def list_records(self, module, q, headers):
        if not q.get("fields"):
            raise ZohoError("REQUIRED_PARAM_MISSING", "One of the expected parameter is missing", {"param": "fields"})
        names = [f.strip() for f in q["fields"].split(",") if f.strip()]
        if len(names) > 50:
            raise ZohoError("LIMIT_EXCEEDED", "at most 50 fields may be requested", {"param": "fields", "limit": 50})
        rows = list(self.records[module].values())
        if q.get("ids"):
            ids = [i.strip() for i in q["ids"].split(",") if i.strip()]
            if len(ids) > 100 or not all(i.isdigit() for i in ids):
                raise ZohoError("INVALID_DATA", "invalid data", {"param": "ids", "limit": 100})
            rows = [self.records[module][i] for i in dict.fromkeys(ids) if i in self.records[module]]
        sort_by, order = q.get("sort_by", "id"), q.get("sort_order", "asc")
        if sort_by not in ("id", "Created_Time", "Modified_Time"):
            raise ZohoError("INVALID_DATA", "invalid data",
                            {"param": "sort_by", "supported_values": ["id", "Created_Time", "Modified_Time"]})
        if order not in ("asc", "desc"):
            raise ZohoError("INVALID_DATA", "invalid data", {"param": "sort_order", "supported_values": ["asc", "desc"]})
        since = headers.get("If-Modified-Since")
        if since:
            try:
                after = parse_dt(since)
            except ValueError:
                raise ZohoError("INVALID_DATA", "invalid data", {"header": "If-Modified-Since",
                                                                 "expected_data_type": "datetime"})
            inclusive = self.args.if_modified_since_inclusive
            rows = [r for r in rows if (parse_dt(r["Modified_Time"]) >= after if inclusive
                                        else parse_dt(r["Modified_Time"]) > after)]
        key = (lambda r: int(r["id"])) if sort_by == "id" else (lambda r: (parse_dt(r[sort_by]), int(r["id"])))
        rows.sort(key=key, reverse=order == "desc")
        chunk, info = page_of(rows, q, {"sort_by": sort_by, "sort_order": order, "next_page_token": None,
                                        "previous_page_token": None, "page_token_expiry": None})
        if not rows and since:
            return (204 if self.args.not_modified_204 else 304), None
        if not chunk:
            return 204, None
        return 200, {"data": [self.render(module, r, set(names)) for r in chunk], "info": info}

    def search(self, module, q):
        spec = MODULES[module]
        if q.get("criteria"):
            criteria = Criteria(self, module, q["criteria"])
            match = criteria.matches
        elif q.get("email") or q.get("phone"):
            kind, wanted = ("email", q["email"]) if q.get("email") else ("phone", q["phone"])
            names = [f["api_name"] for f in spec["fields"] if f["data_type"] == kind]
            match = lambda r: any(norm(r.get(n)) == norm(wanted) for n in names)  # noqa: E731
        elif q.get("word"):
            match = lambda r: any(isinstance(v, str) and norm(q["word"]) in norm(v) for v in r.values())  # noqa: E731
        else:
            raise ZohoError("REQUIRED_PARAM_MISSING", "One of the expected parameter is missing", {"param": "criteria"})
        rows = sorted((r for r in self.records[module].values() if match(r)), key=lambda r: int(r["id"]))
        chunk, info = page_of(rows, q, {})
        if not chunk:
            return 204, None
        return 200, {"data": [self.render(module, r) for r in chunk], "info": info}


def data_items(body):
    if not isinstance(body, dict) or "data" not in body:
        raise ZohoError("REQUIRED_PARAM_MISSING", "One of the expected parameter is missing", {"param": "data"})
    items = body["data"]
    if not isinstance(items, list) or not items:
        raise ZohoError("INVALID_DATA", "invalid data", {"api_name": "data", "expected_data_type": "jsonarray"})
    if len(items) > 100:
        raise ZohoError("LIMIT_EXCEEDED", "at most 100 records per call", {"api_name": "data", "limit": 100})
    trigger = body.get("trigger")
    if trigger is not None and not (isinstance(trigger, list)
                                    and all(t in ("workflow", "approval", "blueprint") for t in trigger)):
        raise ZohoError("INVALID_DATA", "invalid data", {"api_name": "trigger"})
    return items

# ---------------------------------------------------------------- HTTP layer


def consent_page(rid, q):
    e = lambda s: html.escape(str(s), quote=True)  # noqa: E731
    scopes = "".join(f"<li><code>{e(s)}</code></li>" for s in split_scopes(q.get("scope")))
    offline = "<p>Offline access: Weft gets a refresh token.</p>" if q.get("access_type") == "offline" else ""
    return f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Mock Zoho CRM sign-in</title>
<style>body{{font:16px/1.5 system-ui,sans-serif;max-width:36rem;margin:3rem auto;padding:0 16px;color:#1d1d1f}}
.tag{{color:#8a5a00;font-size:.85rem}}code{{word-break:break-all}}button{{font:inherit;padding:.5rem 1.4rem;
margin-right:.5rem;border-radius:6px;border:1px solid #888;cursor:pointer}}.ok{{background:#1b6ef3;color:#fff;border:0}}
</style></head><body>
<p class="tag">Mock Zoho CRM, for local testing only</p>
<h1>Weft wants access to Mock Zoho CRM</h1>
<p>Client <code>{e(q.get("client_id"))}</code> asks for:</p><ul>{scopes}</ul>{offline}
<p>You will be sent back to <code>{e(q.get("redirect_uri"))}</code>.</p>
<form method="post" action="/oauth/v2/auth"><input type="hidden" name="rid" value="{e(rid)}">
<button class="ok" type="submit" name="decision" value="accept">Accept</button>
<button type="submit" name="decision" value="deny">Deny</button></form></body></html>"""


def message_page(title, text):
    t, m = html.escape(title), html.escape(text)
    return (f"<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>{t}</title></head>"
            f"<body style=\"font:16px/1.5 system-ui,sans-serif;max-width:36rem;margin:3rem auto;padding:0 16px\">"
            f"<h1>{t}</h1><p>{m}</p></body></html>")


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server_version = "MockZohoCRM/1.0"
    sys_version = ""

    def log_request(self, code="-", size="-"):
        pass  # handle_any writes one sanitised line per request

    def log_message(self, fmt, *args):
        sys.stderr.write(re.sub(r"\?\S*", "?…", fmt % args) + "\n")

    def do_GET(self):
        self.handle_any("GET")

    def do_POST(self):
        self.handle_any("POST")

    def do_PUT(self):
        self.handle_any("PUT")

    def do_PATCH(self):
        self.handle_any("PATCH")

    def do_DELETE(self):
        self.handle_any("DELETE")

    def handle_any(self, method):
        mock, self.status, self.sent = self.server.mock, 500, False
        split = urllib.parse.urlsplit(self.path)
        path, query = split.path, dict(urllib.parse.parse_qsl(split.query, keep_blank_values=True))
        length = self.headers.get("Content-Length") or "0"
        if "chunked" in self.headers.get("Transfer-Encoding", "").lower() or not length.isdigit():
            self.close_connection = True
            self.send_json(411, {"error": "a Content-Length body is required"})
            return self.finish_any(method, path, query, None)
        raw = self.rfile.read(int(length))
        try:
            if path.startswith("/oauth/v2/"):
                self.oauth(method, path, query, raw)
            elif path.startswith("/crm/"):
                self.crm(method, path, query, raw)
            elif path.startswith("/_mock/"):
                self.admin(method, path, raw)
            else:
                self.send_json(404, ZohoError("INVALID_URL_PATTERN", "Please check if the URL trying to access is a "
                                                                     "correct one").body)
        except ZohoError as e:
            self.send_json(e.http, e.body, e.headers)
        except Exception as e:  # noqa: BLE001 - a mock answers 500 rather than dropping the connection
            sys.stderr.write(f"internal error on {method} {path}: {type(e).__name__}: {e}\n")
            if not self.sent:
                self.send_json(500, ZohoError("INTERNAL_ERROR", "Internal Server Error").body)
        self.finish_any(method, path, query, raw)

    def finish_any(self, method, path, query, raw):
        sys.stderr.write(f"{method} {path} {self.status}\n")
        if path.startswith("/_mock/") and method == "GET":
            return
        entry = {"method": method, "path": path, "status": self.status}
        if path.startswith("/crm/"):
            if query:
                entry["query"] = query
            if self.headers.get("If-Modified-Since"):
                entry["if_modified_since"] = self.headers["If-Modified-Since"]
            try:
                if raw:
                    entry["body"] = redact(json.loads(raw))
            except ValueError:
                pass
        self.server.mock.log_call(entry)

    def send_raw(self, status, data, ctype, headers=None):
        self.status, self.sent = status, True
        self.send_response(status)
        if status not in (204, 304):
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(data)))
        for name, value in (headers or {}).items():
            self.send_header(name, value)
        self.end_headers()
        if data and status not in (204, 304):
            self.wfile.write(data)

    def send_json(self, status, body, headers=None):
        data = b"" if body is None else json.dumps(body).encode()
        self.send_raw(status, data, "application/json;charset=UTF-8", headers)

    def send_html(self, status, text):
        self.send_raw(status, text.encode(), "text/html;charset=UTF-8", {
            "Content-Security-Policy": "default-src 'none'; style-src 'unsafe-inline'", "X-Frame-Options": "DENY",
            "Cache-Control": "no-store", "Referrer-Policy": "no-referrer"})

    def redirect(self, url):
        self.send_raw(302, b"", "text/plain", {"Location": url, "Cache-Control": "no-store",
                                               "Referrer-Policy": "no-referrer"})

    def oauth(self, method, path, query, raw):
        mock = self.server.mock
        form = dict(urllib.parse.parse_qsl(raw.decode("utf-8", "replace"), keep_blank_values=True)) if raw else {}
        if path == "/oauth/v2/auth" and method == "GET":
            problem = mock.authorize_problem(query)
            if problem:
                return self.send_html(400, message_page("Authorization failed", problem))
            if query.get("auto") in ("1", "deny"):
                return self.redirect(mock.consent_redirect(query, query["auto"] == "1"))
            return self.send_html(200, consent_page(mock.park_consent(query), query))
        if path == "/oauth/v2/auth" and method == "POST":
            req = mock.take_consent(form.get("rid", ""))
            if req is None:
                return self.send_html(400, message_page("Authorization failed",
                                                        "This request expired or was already answered. Start again."))
            return self.redirect(mock.consent_redirect(req, form.get("decision") == "accept"))
        if path == "/oauth/v2/token" and method == "POST":
            return self.send_json(200, mock.token({**query, **form}))
        if path == "/oauth/v2/token/revoke" and method == "POST":
            return self.send_json(200, mock.revoke({**query, **form}))
        self.send_json(404, {"error": "invalid_request"})

    def crm(self, method, path, query, raw):
        mock = self.server.mock
        with mock.lock:
            grant = mock.authenticate(self.headers.get("Authorization"))
            if grant is None:
                raise ZohoError("INVALID_TOKEN", "invalid oauth token", http=401)
            if not path.startswith(API_PREFIX + "/"):
                raise ZohoError("INVALID_URL_PATTERN", "Please check if the URL trying to access is a correct one",
                                http=404)
            parts = [p for p in path[len(API_PREFIX):].split("/") if p]
            if not mock.scope_ok(grant["scopes"], parts, method):
                raise ZohoError("OAUTH_SCOPE_MISMATCH", "invalid oauth scope to access this URL", http=401)
            limited = mock.take_credit()
            if limited:
                secs, code = limited
                raise ZohoError(code, "the API call limit was reached; try again later", {"retry_after": secs},
                                http=429, headers={"Retry-After": str(secs)})
            body = None
            if raw:
                try:
                    body = json.loads(raw)
                except ValueError:
                    raise ZohoError("INVALID_DATA", "the request body is not valid JSON")
            status, payload = mock.crm_route(method, parts, query, body, self.headers)
        self.send_json(status, payload)

    def admin(self, method, path, raw):
        mock = self.server.mock
        parts = [p for p in path.split("/") if p][1:]
        if parts == ["health"] and method == "GET":
            return self.send_json(200, {"status": "ok", "accounts_server": mock.base, "api_domain": mock.base,
                                        "redirect_uri": mock.redirect_uri})
        if parts == ["calls"] and method in ("GET", "DELETE"):
            with mock.lock:
                calls = list(mock.calls)
                if method == "DELETE":
                    mock.calls.clear()
                    return self.send_json(200, {"cleared": len(calls)})
                return self.send_json(200, {"usage": mock.usage_view(), "calls": calls})
        if parts == ["channels"] and method == "GET":
            with mock.lock:
                now = time.time()
                chans = [{"channel_id": c["channel_id"], "events": c["events"], "notify_url": redact_url(c["notify_url"]),
                          "channel_expiry": c["channel_expiry"], "expired": c["expires_ts"] <= now,
                          "has_token": bool(c.get("token")), "notify_on_related_action": c["notify_on_related_action"],
                          "return_affected_field_values": c["return_affected_field_values"]}
                         for c in mock.channels.values()]
            return self.send_json(200, {"channels": chans})
        if parts == ["deliveries"] and method == "GET":
            return self.send_json(200, {"deliveries": list(mock.deliveries)})
        if parts == ["expire-tokens"] and method == "POST":
            return self.send_json(200, {"expired": mock.expire_tokens()})
        if parts[:1] == ["records"] and len(parts) in (2, 3):
            return self.admin_records(method, parts[1], parts[2] if len(parts) == 3 else None, raw)
        self.send_json(404, {"error": "unknown admin endpoint"})

    def admin_records(self, method, module, rid, raw):
        mock = self.server.mock
        mock.module(module, None)
        try:
            body = json.loads(raw) if raw else None
        except ValueError:
            raise ZohoError("INVALID_DATA", "the request body is not valid JSON")
        items = body["data"] if isinstance(body, dict) and isinstance(body.get("data"), list) else (
            body if isinstance(body, list) else [body])
        with mock.lock:
            if method == "GET":
                recs = sorted(mock.records[module].values(), key=lambda r: int(r["id"]))
                recs = [r for r in recs if rid is None or r["id"] == rid]
                if rid and not recs:
                    return self.send_json(404, {"error": "record not found"})
                return self.send_json(200, {"data": [mock.render(module, r) for r in recs], "count": len(recs)})
            if method == "POST" and rid is None:
                return self.send_json(*mock.write(module, items, "insert"))
            if method == "PATCH" and rid:
                return self.send_json(*mock.write(module, items[:1], "update", record_id=rid))
            if method == "DELETE" and rid:
                return self.send_json(*mock.delete(module, [rid]))
        self.send_json(405, {"error": "method not allowed"})


def parse_args(argv):
    p = argparse.ArgumentParser(description="Mock Zoho CRM (accounts, CRM API v8, notifications) for local Weft tests.")
    p.add_argument("--port", type=int, default=9200, help="port on 127.0.0.1 (default 9200)")
    p.add_argument("--client-id", required=True, help="the OAuth client_id Weft uses (WEFT_OAUTH_ZOHO_CRM_CLIENT_ID)")
    p.add_argument("--client-secret", default=os.environ.get("ZOHO_MOCK_CLIENT_SECRET"),
                   help="the OAuth client secret (or set ZOHO_MOCK_CLIENT_SECRET)")
    p.add_argument("--weft-public", default="http://localhost:8080",
                   help="Weft's WEFT_PUBLIC_URL; the only accepted redirect URI is <this>/oauth/callback")
    p.add_argument("--state-file", help="JSON file that keeps records, channels and token hashes across restarts")
    p.add_argument("--token-ttl", type=int, default=3600, help="access token lifetime in seconds (default 3600)")
    p.add_argument("--limit-per-minute", type=int, default=0, help="answer 429 after N API calls in a minute")
    p.add_argument("--limit-per-day", type=int, default=0, help="answer 429 after N API calls in a UTC day")
    p.add_argument("--utc-offset", default="+05:30", help="org time zone of Created_Time / Modified_Time")
    p.add_argument("--no-seed", action="store_true", help="start with no records")
    p.add_argument("--if-modified-since-inclusive", action="store_true",
                   help="If-Modified-Since returns Modified_Time >= the header (default: strictly after)")
    p.add_argument("--not-modified-204", action="store_true",
                   help="an empty If-Modified-Since list answers 204 instead of 304")
    p.add_argument("--write-error-status", type=int, choices=(400, 202), default=400,
                   help="HTTP status when every record of a write fails (default 400)")
    p.add_argument("--lenient-scopes", action="store_true", help="do not check OAuth scopes")
    args = p.parse_args(argv)
    if not args.client_secret:
        p.error("--client-secret or ZOHO_MOCK_CLIENT_SECRET is required")
    if args.token_ttl < 1:
        p.error("--token-ttl must be at least 1")
    if not re.fullmatch(r"[+-]\d\d:\d\d", args.utc_offset):
        p.error("--utc-offset looks like +05:30")
    if not re.match(r"https?://[^/]+", args.weft_public):
        p.error("--weft-public must be an http(s) URL")
    return args


def main(argv=None):
    args = parse_args(argv)
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    server.mock = mock = Mock(args, server.server_address[1])
    mock.notifier.start()

    def stop(*_):
        raise KeyboardInterrupt

    signal.signal(signal.SIGTERM, stop)
    sys.stderr.write(f"Mock Zoho CRM (local testing only) on {mock.base}: accounts server and API domain; "
                     f"redirect URI {mock.redirect_uri}\n")
    try:
        server.serve_forever(poll_interval=0.5)
    except KeyboardInterrupt:
        pass
    finally:
        mock.notifier.stop()
        server.server_close()
        with mock.lock:
            mock.save()


if __name__ == "__main__":
    main()
