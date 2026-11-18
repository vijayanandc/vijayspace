"""Live check of a CRM → FSM integration through Weft: a Deal created in Zoho CRM (the local mock,
scripts/zoho_mock.py) upserts an account and creates a service request in a real Appblocks FSM. Then every access
token is expired and the next run must refresh and still succeed.

Weft runs with sign-in off on this machine, pointed at the mock (WEFT_ZOHO_ACCOUNTS_URL, the same OAuth client id
and secret as the mock, WEFT_ALLOW_PRIVATE_NETWORKS=true). It needs a Zoho CRM connection, signed in through the
mock's consent page in the web client, and an Appblocks connection to the FSM.

Environment: WEFT_URL (default http://127.0.0.1:8282), ZOHO_MOCK_URL (default http://127.0.0.1:9200), and
FSM_BASE_URL, FSM_ORG_ID, FSM_API_TOKEN (to read the created request back; never printed).

    python3 -I scripts/live_zoho_fsm_check.py
"""
import json, time, urllib.request, os, sys
O = urllib.request.build_opener(urllib.request.ProxyHandler({}))
W = os.environ.get("WEFT_URL", "http://127.0.0.1:8282")
Z = os.environ.get("ZOHO_MOCK_URL", "http://127.0.0.1:9200")
FSM = {k: os.environ[k] for k in ("FSM_BASE_URL", "FSM_ORG_ID", "FSM_API_TOKEN")}

def call(method, url, body=None, headers=None):
    req = urllib.request.Request(url, method=method, data=None if body is None else json.dumps(body).encode(),
                                 headers={"content-type": "application/json", **(headers or {})})
    try:
        with O.open(req, timeout=30) as r:
            raw = r.read(); return r.status, (json.loads(raw) if raw else None)
    except urllib.error.HTTPError as e:
        raw = e.read(); return e.code, raw.decode(errors="replace")

results = []
def check(name, ok, detail=""):
    results.append(ok); print(("PASS " if ok else "FAIL ") + name + ("" if ok else f"  ({detail})"), flush=True)

def runs(flow, n, timeout=40):
    for _ in range(timeout * 2):
        _, page = call("GET", f"{W}/api/v1/executions?flow_id={flow}&limit=10")
        done = [e for e in page["items"] if e["status"] in ("succeeded", "failed")]
        if len(done) >= n: return done
        time.sleep(0.5)
    return []

_, cxns = call("GET", f"{W}/api/v1/connections")
crm = next(c["id"] for c in cxns if c["connector"] == "zoho_crm")
fsm = next(c["id"] for c in cxns if c["connector"] == "appblocks")
stamp = str(int(time.time()))
_, flow = call("POST", f"{W}/api/v1/flows", {"name": f"CRM deal → FSM request {stamp}", "definition": {
    "trigger": {"type": "app", "connector": "zoho_crm", "trigger": "record_event", "connection_id": crm,
                "input": {"module": "Deals", "events": ["create"]}, "debounce_secs": 0},
    "steps": [{"key": "acct", "type": "action", "connector": "appblocks", "action": "upsert_record", "connection_id": fsm,
               "input": {"module": "accounts", "duplicate_check_fields": "external_id",
                         "values": {"external_id": "crm-deal-{{ trigger.id }}", "name": "{{ trigger.record.Deal_Name }}"}, "skip_workflows": True}},
              {"key": "request", "type": "action", "connector": "appblocks", "action": "create_record", "connection_id": fsm,
               "input": {"module": "requests", "values": {"summary": "From CRM: {{ trigger.record.Deal_Name }}", "status": "New",
                         "priority": "High", "account_id": "{{ steps.acct.id }}"}, "skip_workflows": True}}]}})
st, pub = call("POST", f"{W}/api/v1/flows/{flow['id']}/publish")
check("flow publishes", st == 200 and not pub.get("issues"), pub)
call("POST", f"{W}/api/v1/flows/{flow['id']}/enable")
time.sleep(2)
_, channels = call("GET", f"{Z}/_mock/channels")
chans = channels if isinstance(channels, list) else channels.get("channels", [])
check("Weft subscribed to Deals notifications in Zoho", any("Deals" in json.dumps(c) and "/hooks/app/" in json.dumps(c) for c in chans), channels)

def deal(name):
    st, d = call("POST", f"{Z}/_mock/records/Deals", {"Deal_Name": name, "Stage": "Qualification", "Amount": 1200, "Closing_Date": "2026-12-31"})
    return st, d

st, d = deal(f"Acme AC service {stamp}")
check("a Deal is created in Zoho", st in (200, 201), (st, d))
done = runs(flow["id"], 1)
check("the notification started a Weft run", len(done) == 1, done)
if done:
    _, r = call("GET", f"{W}/api/v1/executions/{done[0]['id']}")
    check("the run succeeded", r["status"] == "succeeded", r.get("error_message"))
    out = next((s["output"] for s in r["steps"] if s["step_key"] == "request"), {}) or {}
    req_id = out.get("id") or out.get("api_id") or (out.get("record") or {}).get("api_id")
    st, req = call("GET", f"{FSM['FSM_BASE_URL']}/api/requests/{req_id}", headers={"Authorization": f"Bearer {FSM['FSM_API_TOKEN']}", "X-Org-ID": FSM["FSM_ORG_ID"]})
    check("the FSM request exists with the deal's name", st == 200 and req.get("summary") == f"From CRM: Acme AC service {stamp}", (st, req))
    _, log = call("GET", f"{W}/api/v1/executions/{done[0]['id']}/log")
    hosts = sorted({e["details"]["url"].split("/")[2] for e in log["entries"] if e["kind"] == "http"})
    check("the run log shows the FSM calls (the Zoho fetch belongs to the trigger, before the run)", hosts == ["127.0.0.1:8090"], hosts)

# Expire every access token: the next run must refresh and still succeed.
call("POST", f"{Z}/_mock/expire-tokens")
call("DELETE", f"{Z}/_mock/calls")
st, d = deal(f"Bravo AC service {stamp}")
done = runs(flow["id"], 2)
check("after the token expired, the next deal still ran", len(done) == 2, done)
if len(done) == 2:
    _, r = call("GET", f"{W}/api/v1/executions/{done[0]['id']}")
    check("and succeeded", r["status"] == "succeeded", r.get("error_message"))
_, calls = call("GET", f"{Z}/_mock/calls")
items = calls if isinstance(calls, list) else calls.get("calls", [])
paths = [c.get("path") for c in items]
check("Weft got a 401, refreshed the token once and retried", "/oauth/v2/token" in paths and any(c.get("status") == 401 for c in items), paths)
# A Deal edit is not subscribed (events: create only).
call("POST", f"{W}/api/v1/flows/{flow['id']}/disable")
print(json.dumps({"passed": sum(results), "failed": len(results) - sum(results)}))
