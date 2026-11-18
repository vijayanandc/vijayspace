"""Live check of Weft's Appblocks connector against a real Appblocks FSM server.

Covers:
- the connection test and the create / get / update / upsert / search actions;
- the instant trigger through a real FSM workflow webhook (X-Weft-Key);
- write-back without re-triggering (skip_workflows);
- the polling trigger;
- the run log.

Weft must run with sign-in off on this machine (e.g. `WEFT_AUTH=off WEFT_ALLOW_PRIVATE_NETWORKS=true weft serve`),
and must have an Appblocks connection to the FSM. The FSM must accept loopback webhooks (is_production=false).

Environment (secrets are read here and never printed):
- WEFT_URL: default http://127.0.0.1:8181
- FSM_BASE_URL, FSM_ORG_ID, FSM_API_TOKEN: an API token with CRUD on accounts, contacts and requests
- FSM_USER_TOKEN: an FSM admin's access token, needed to create the workflow webhook

    python3 -I scripts/live_fsm_check.py
"""
import json, os, sys, time, urllib.request

WEFT = os.environ.get("WEFT_URL", "http://127.0.0.1:8181")
OPEN = urllib.request.build_opener(urllib.request.ProxyHandler({}))

FSM = {k: os.environ[k] for k in ("FSM_BASE_URL", "FSM_ORG_ID", "FSM_API_TOKEN")}
results = []

def http(method, url, body=None, headers=None):
    req = urllib.request.Request(url, method=method, data=None if body is None else json.dumps(body).encode(),
                                 headers={"content-type": "application/json", **(headers or {})})
    try:
        with OPEN.open(req, timeout=30) as r:
            raw = r.read()
            return r.status, (json.loads(raw) if raw else None)
    except urllib.error.HTTPError as e:
        raw = e.read()
        try:
            return e.code, json.loads(raw)
        except Exception:
            return e.code, raw.decode(errors="replace")

def weft(method, path, body=None):
    return http(method, WEFT + path, body)

def fsm(method, path, body=None, user=False):
    token = os.environ["FSM_USER_TOKEN"] if user else FSM["FSM_API_TOKEN"]
    return http(method, FSM["FSM_BASE_URL"] + path, body, {"Authorization": f"Bearer {token}", "X-Org-ID": FSM["FSM_ORG_ID"]})

def check(name, ok, detail=""):
    results.append((name, bool(ok)))
    print(("PASS " if ok else "FAIL ") + name + (f"  ({detail})" if detail and not ok else ""), flush=True)

def wait_run(flow_id, n=1, timeout=30):
    for _ in range(timeout * 2):
        _, page = weft("GET", f"/api/v1/executions?flow_id={flow_id}&limit=10")
        done = [e for e in page["items"] if e["status"] in ("succeeded", "failed")]
        if len(done) >= n:
            return done
        time.sleep(0.5)
    return []

def flow(name, definition):
    _, f = weft("POST", "/api/v1/flows", {"name": name, "definition": definition})
    _, pub = weft("POST", f"/api/v1/flows/{f['id']}/publish")
    assert not pub.get("issues"), pub
    weft("POST", f"/api/v1/flows/{f['id']}/enable")
    return weft("GET", f"/api/v1/flows/{f['id']}")[1]

def main():
    stamp = str(int(time.time()))
    _, cxns = weft("GET", "/api/v1/connections")
    cxn = next(c["id"] for c in cxns if c["connector"] == "appblocks")
    st, t = weft("POST", f"/api/v1/connections/{cxn}/test")
    check("connection test reaches FSM", t.get("ok") is True, t)

    # 1. Actions through a manual flow: upsert an account by external_id, create a contact, find it.
    f1 = flow(f"Live actions {stamp}", {"trigger": {"type": "manual"}, "steps": [
        {"key": "acct", "type": "action", "connector": "appblocks", "action": "upsert_record", "connection_id": cxn,
         "input": {"module": "accounts", "duplicate_check_fields": "external_id", "values": {"external_id": "{{ trigger.ext }}", "name": "{{ trigger.name }}", "city": "Austin"}, "skip_workflows": True}},
        {"key": "person", "type": "action", "connector": "appblocks", "action": "create_record", "connection_id": cxn,
         "input": {"module": "contacts", "values": {"name": "Pat {{ trigger.ext }}", "email": "pat{{ trigger.ext }}@weft.example", "account_id": "{{ steps.acct.id }}"}, "skip_workflows": True}},
        {"key": "found", "type": "action", "connector": "appblocks", "action": "search_records", "connection_id": cxn,
         "input": {"module": "contacts", "field": "email", "value": "pat{{ trigger.ext }}@weft.example"}},
        {"key": "moved", "type": "action", "connector": "appblocks", "action": "update_record", "connection_id": cxn,
         "input": {"module": "accounts", "record_id": "{{ steps.acct.id }}", "values": {"city": "Dallas"}, "skip_workflows": True}},
        {"key": "again", "type": "action", "connector": "appblocks", "action": "get_record", "connection_id": cxn,
         "input": {"module": "accounts", "record_id": "{{ steps.acct.id }}"}},
    ]})
    ext = f"live-{stamp}"
    weft("POST", f"/api/v1/flows/{f1['id']}/run", {"payload": {"ext": ext, "name": f"Live Co {stamp}"}})
    runs = wait_run(f1["id"])
    st, run = weft("GET", f"/api/v1/executions/{runs[0]['id']}") if runs else (0, {})
    check("actions flow succeeds", run.get("status") == "succeeded", run.get("error_message"))
    out = {s["step_key"]: s.get("output") for s in run.get("steps", []) if s["kind"] == "action"}
    acct = out.get("acct") or {}
    acct_id = acct.get("id")
    check("upsert inserted the account", acct_id and acct.get("action") == "insert", acct)
    found = out.get("found") or {}
    check("search finds the contact just created", (found.get("count") or len(found.get("records", []))) >= 1, found)
    check("update + get show the new city", ((out.get("again") or {}).get("record") or {}).get("city") == "Dallas", out.get("again"))
    st, rec = fsm("GET", f"/api/accounts/{acct_id}")
    check("FSM itself has the update", st == 200 and rec.get("city") == "Dallas", (st, rec))
    weft("POST", f"/api/v1/flows/{f1['id']}/run", {"payload": {"ext": ext, "name": f"Live Co {stamp} renamed"}})
    runs = wait_run(f1["id"], 2)
    st, run2 = weft("GET", f"/api/v1/executions/{runs[0]['id']}")
    acct2 = next((s.get("output") for s in run2.get("steps", []) if s["step_key"] == "acct"), {}) or {}
    check("a second upsert updates the same account", acct2.get("id") == acct_id and acct2.get("action") == "update", acct2)
    _, log = weft("GET", f"/api/v1/executions/{runs[0]['id']}/log")
    calls = [e for e in log["entries"] if e["kind"] == "http"]
    check("the run log has every FSM call", len(calls) >= 5 and all("8090" in e["message"] for e in calls), len(calls))
    check("skip_workflows writes carry ?trigger=", any("trigger=" in e["message"] for e in calls), [e["message"] for e in calls][:3])

    # 2. Instant trigger: an FSM workflow webhook on request create → Weft sets the request to Open.
    f2 = flow(f"Live FSM request {stamp}", {"trigger": {"type": "app", "connector": "appblocks", "trigger": "record_event",
              "connection_id": cxn, "input": {"module": "requests", "operation": "create"}, "debounce_secs": 0}, "steps": [
        {"key": "open", "type": "action", "connector": "appblocks", "action": "update_record", "connection_id": cxn,
         "input": {"module": "requests", "record_id": "{{ trigger.id }}", "values": {"status": "Open"}, "skip_workflows": True}},
    ]})
    trig = f2["trigger"]
    check("instant trigger has an app event URL and key", trig.get("app_event_url") and trig.get("verify_token"), trig)
    hook = WEFT + trig["app_event_url"][trig["app_event_url"].index("/hooks/"):]
    st, wf = fsm("POST", "/api/workflows", {"module_api_name": "requests", "name": f"Weft live {stamp}", "is_active": True, "sort_order": 100,
                 "triggers": [{"event_type": "create"}],
                 "actions": [{"type": "webhook", "sort_order": 10, "webhook": {"url": hook, "method": "POST", "headers": {"X-Weft-Key": trig["verify_token"]},
                              "payload": {"module": "requests", "operation": "create", "api_id": "{api_id}"}}}]}, user=True)
    wf_id = (wf or {}).get("api_id") or ((wf or {}).get("data") or {}).get("api_id")
    check("FSM workflow webhook created", st in (200, 201) and wf_id, (st, wf))
    try:
        st, req = fsm("POST", "/api/requests", {"summary": f"Live AC check {stamp}", "status": "New", "priority": "High", "account_id": acct_id})
        req_id = (req or {}).get("api_id") or ((req or {}).get("data") or {}).get("api_id")
        check("FSM request created", st in (200, 201) and req_id, (st, req))
        runs = wait_run(f2["id"], 1, timeout=45)
        check("the FSM webhook started a Weft run", len(runs) == 1, runs)
        if runs:
            _, r = weft("GET", f"/api/v1/executions/{runs[0]['id']}")
            check("the run succeeded", r["status"] == "succeeded", r.get("error_message"))
            check("trigger data has the full FSM record", (r["trigger_data"].get("record") or {}).get("summary", "").startswith("Live AC"), r["trigger_data"])
        st, req2 = fsm("GET", f"/api/requests/{req_id}")
        check("Weft wrote the status back to FSM", req2.get("status") == "Open", req2.get("status"))
        time.sleep(5)
        runs = wait_run(f2["id"], 1)
        check("the write-back did not loop (still one run)", len(runs) == 1, len(runs))
        # A webhook without the key is refused.
        st, body = http("POST", hook, {"module": "requests", "operation": "create", "api_id": req_id})
        check("a delivery without X-Weft-Key is refused", st >= 400, (st, body))
    finally:
        if wf_id:
            fsm("DELETE", f"/api/workflows/{wf_id}", user=True)
    # 3. Polling trigger: no FSM setup; a contact edited after the baseline poll starts a run.
    f3 = flow(f"Live poll {stamp}", {"trigger": {"type": "app", "connector": "appblocks", "trigger": "record_changed",
              "connection_id": cxn, "input": {"module": "contacts"}, "poll_interval_secs": 60, "debounce_secs": 0},
              "steps": [{"key": "note", "type": "action", "connector": "core.formatter", "action": "text_transform",
                         "input": {"text": "{{ trigger.record.name }}", "operation": "upper"}}]})
    time.sleep(8)  # the first poll records the baseline time
    _, contacts = fsm("GET", "/api/contacts?limit=1&sort_by=modified_time&sort_type=desc")
    rows = contacts if isinstance(contacts, list) else (contacts or {}).get("data") or (contacts or {}).get("records") or []
    target = rows[0]["api_id"] if rows else None
    st, _ = fsm("PATCH", f"/api/contacts/{target}", {"name": f"Polled {stamp}"})
    check("contact edited in FSM", st == 200, st)
    runs = wait_run(f3["id"], 1, timeout=150)
    check("the poll started a run for the edited contact", len(runs) >= 1, runs)
    if runs:
        _, r = weft("GET", f"/api/v1/executions/{runs[0]['id']}")
        note = next((x.get("output") for x in r["steps"] if x["step_key"] == "note"), None)
        check("the polled record is the edited one", r["trigger_data"].get("id") == target, r["trigger_data"].get("id"))
        check("the step saw its new name", json.dumps(note).find(f"POLLED {stamp}") >= 0, note)
    weft("POST", f"/api/v1/flows/{f3['id']}/disable")
    print(json.dumps({"passed": sum(ok for _, ok in results), "failed": [n for n, ok in results if not ok]}))

if __name__ == "__main__":
    main()
