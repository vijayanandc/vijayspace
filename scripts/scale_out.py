"""Scale-out check: the same burst of runs drained by 1, 2 and 4 `weft worker` processes.

Each run calls a mock app that answers after LATENCY seconds, so a worker's throughput is bounded by its
concurrency; adding workers should cut the drain time and the queue lag roughly in proportion.

Local development only: it drops and recreates the `weft_stress` database, runs `weft api` on 127.0.0.1:8181
with sign-in off, and reads MySQL with the credentials in `my.cnf` next to this script (a `[client]` section
with user, password and host; keep it chmod 600). Build first: `cargo build -p weft-server`.

    LATENCY=0.25 CONCURRENCY=8 RUNS=1200 WORKERS=1,2,4 python3 -I scripts/scale_out.py
"""
import http.server, json, os, subprocess, sys, threading, time, urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = f"{ROOT}/target/debug/weft"
DB = "weft_stress"
API = "127.0.0.1:8181"
MOCK = ("127.0.0.1", 9911)
LATENCY = float(os.environ.get("LATENCY", "0.25"))
RUNS = int(os.environ.get("RUNS", "1200"))
PER_REQUEST = 100
CONCURRENCY = os.environ.get("CONCURRENCY", "8")
MY_CNF = os.path.join(os.path.dirname(os.path.abspath(__file__)), "my.cnf")
WORKERS = [int(n) for n in os.environ.get("WORKERS", "1,2,4").split(",")]

# Loopback only: never through the environment's HTTP proxy.
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))

class Slow(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        time.sleep(LATENCY)
        body = b'{"ok":true}'
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *a):
        pass

def env(extra=None):
    e = {k: v for k, v in os.environ.items() if k in ("PATH", "HOME")}
    e.update({
        "WEFT_DATABASE_URL": f"mysql://weft:weft@127.0.0.1:3306/{DB}",
        "WEFT_AUTH": "off",
        "WEFT_ALLOW_PRIVATE_NETWORKS": "true",
        "WEFT_BIND": API,
        "WEFT_PUBLIC_URL": f"http://{API}",
        "WEFT_WORKER_CONCURRENCY": CONCURRENCY,
        "WEFT_DB_POOL_SIZE": os.environ.get("POOL", str(int(CONCURRENCY) + 8)),
        "WEFT_LOG": "warn",
    })
    e.update(extra or {})
    return e

def call(method, path, body=None):
    req = urllib.request.Request(f"http://{API}{path}", method=method,
        data=None if body is None else json.dumps(body).encode(),
        headers={"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as r:
        raw = r.read()
        return json.loads(raw) if raw else None

def sql(q):
    out = subprocess.run(["mysql", f"--defaults-extra-file={MY_CNF}", "-h127.0.0.1", "-N", "-B", DB, "-e", q],
                         capture_output=True, text=True, check=True).stdout.strip()
    return out

def wait_api():
    for _ in range(120):
        try:
            urllib.request.urlopen(f"http://{API}/healthz", timeout=2).read(); return
        except Exception:
            time.sleep(0.5)
    sys.exit("api did not start")

def main():
    threading.Thread(target=http.server.ThreadingHTTPServer(MOCK, Slow).serve_forever, daemon=True).start()
    subprocess.run(["mysql", f"--defaults-extra-file={MY_CNF}", "-e",
                    f"DROP DATABASE IF EXISTS {DB}; CREATE DATABASE {DB}"], check=True)
    subprocess.run([BIN, "migrate"], env=env(), check=True)
    api = subprocess.Popen([BIN, "api"], env=env())
    try:
        wait_api()
        flow = call("POST", "/api/v1/flows", {"name": "Stress", "definition": {
            "trigger": {"type": "webhook", "split_array": True},
            "steps": [{"key": "call", "type": "action", "connector": "core.http", "action": "request",
                       "input": {"method": "GET", "url": f"http://{MOCK[0]}:{MOCK[1]}/slow?n={{{{ trigger.n }}}}"}}]}})
        call("POST", f"/api/v1/flows/{flow['id']}/publish")
        call("POST", f"/api/v1/flows/{flow['id']}/enable")
        hook = call("GET", f"/api/v1/flows/{flow['id']}")["trigger"]["webhook_url"]
        results = []
        for n in WORKERS:
            sql("DELETE FROM step_runs; DELETE FROM executions; DELETE FROM jobs; DELETE FROM instances")
            workers = [subprocess.Popen([BIN, "worker"], env=env()) for _ in range(n)]
            time.sleep(3)  # let the workers start and heartbeat
            start = time.time()
            for i in range(0, RUNS, PER_REQUEST):
                urllib.request.urlopen(urllib.request.Request(hook, method="POST",
                    data=json.dumps([{"n": i + j} for j in range(PER_REQUEST)]).encode(),
                    headers={"content-type": "application/json"}), timeout=30).read()
            peak_lag = 0
            while True:
                ops = call("GET", "/api/v1/ops/status")
                peak_lag = max([peak_lag] + [q["lag_seconds"] for q in ops["queues"]])
                done = int(sql("SELECT COUNT(*) FROM executions WHERE status IN ('succeeded','failed')"))
                if done >= RUNS:
                    break
                if time.time() - start > 600:
                    sys.exit("timed out")
                time.sleep(0.5)
            elapsed = time.time() - start
            stats = sql("SELECT SUM(status='failed'), ROUND(AVG(TIMESTAMPDIFF(MICROSECOND, queued_at, started_at))/1e6, 2), "
                        "ROUND(MAX(TIMESTAMPDIFF(MICROSECOND, queued_at, started_at))/1e6, 2) FROM executions").split("\t")
            live = len(ops["instances"])
            for w in workers:
                w.terminate()
            for w in workers:
                w.wait(timeout=60)
            results.append({"workers": n, "live_instances_seen": live, "runs": RUNS, "failed": int(stats[0]),
                            "drain_seconds": round(elapsed, 1), "runs_per_second": round(RUNS / elapsed, 1),
                            "avg_queue_wait_s": float(stats[1]), "max_queue_wait_s": float(stats[2]),
                            "peak_lag_seen_s": peak_lag})
            print(json.dumps(results[-1]), flush=True)
        print(json.dumps({"latency_s": LATENCY, "concurrency_per_worker": int(CONCURRENCY), "results": results}, indent=2))
    finally:
        api.terminate()
        api.wait(timeout=30)

if __name__ == "__main__":
    main()
