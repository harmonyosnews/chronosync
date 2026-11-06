#!/usr/bin/env python3
"""Two-node loopback test for WebUI manual-Server takeover and failback.

A is a fixed server; B starts as a client with a seed to A. The test uses the
WebUI settings API to pin B as server, then releases it. A must step down, then
reclaim the server role after B resigns. No clock is written (auto_apply=false).
"""
import argparse, json, os, shutil, subprocess, sys, tempfile, time, urllib.request


def state(port):
    with urllib.request.urlopen(f"http://127.0.0.1:{port}/api/cluster/state", timeout=3) as r:
        return json.load(r)


def wait(port, predicate, timeout=12):
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        try:
            last = state(port)
            if predicate(last):
                return last
        except Exception:
            pass
        time.sleep(0.3)
    return last


def post(port, payload):
    request = urllib.request.Request(
        f"http://127.0.0.1:{port}/api/cluster/settings",
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=8) as r:
        return json.load(r)


def start(binary, root, secret, label, ip, web, discovery, time_port, mode, seed=None):
    args = [
        binary, "--bind", "127.0.0.1", "--port", str(web),
        "--log-dir", os.path.join(root, label),
        "--cluster-mode", mode, "--cluster-id", "takeover", "--node-id", label,
        "--cluster-secret-file", secret, "--cluster-bind", ip,
        "--discovery-port", str(discovery), "--time-port", str(time_port),
    ]
    if seed:
        args += ["--cluster-seed", seed]
    log = open(os.path.join(root, label + ".log"), "wb")
    return subprocess.Popen(args, stdout=log, stderr=subprocess.STDOUT), log


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default="target/debug/chronosync")
    parser.add_argument("--out", default="")
    args = parser.parse_args()
    binary = os.path.abspath(args.binary)
    if not os.path.isfile(binary):
        sys.exit(f"binary not found: {binary}")
    root = args.out or tempfile.mkdtemp(prefix="chronosync-takeover-")
    os.makedirs(root, exist_ok=True)
    secret = os.path.join(root, "cluster.key")
    with open(secret, "w") as f:
        f.write("0123456789abcdef0123456789abcdef\n")
    os.chmod(secret, 0o600)
    processes = []
    try:
        a, la = start(binary, root, secret, "manual-a", "127.0.0.20", 19720, 19640, 19650, "server")
        b, lb = start(binary, root, secret, "manual-b", "127.0.0.21", 19721, 19641, 19651, "client", "127.0.0.20:19640")
        processes += [(a, la), (b, lb)]
        initial = wait(19721, lambda x: x.get("server_id") == "manual-a")
        post(19721, {"manual_server": True})
        winner = wait(19721, lambda x: x.get("role") == "server" and x.get("manual_server"))
        stepped_down = wait(19720, lambda x: x.get("role") == "client", 8)
        post(19721, {"manual_server": False})
        reclaimed = wait(19720, lambda x: x.get("role") == "server", 12)
        follower = wait(19721, lambda x: x.get("server_id") == "manual-a", 12)
        ok = bool(
            initial
            and winner
            and stepped_down
            and reclaimed
            and follower
            and stepped_down.get("server_id") == "manual-b"
            and reclaimed.get("term", 0) > winner.get("term", 0)
            and reclaimed.get("last_error") is None
        )
        print(json.dumps({
            "ok": ok,
            "initial_b_server": initial.get("server_id") if initial else None,
            "winner_term": winner.get("term") if winner else None,
            "stepped_down": stepped_down.get("role") if stepped_down else None,
            "stepped_down_server": stepped_down.get("server_id") if stepped_down else None,
            "reclaimed": reclaimed.get("role") if reclaimed else None,
            "reclaimed_term": reclaimed.get("term") if reclaimed else None,
            "reclaimed_error": reclaimed.get("last_error") if reclaimed else None,
            "follower_server": follower.get("server_id") if follower else None,
        }, ensure_ascii=False))
        return 0 if ok else 1
    finally:
        for process, log in processes:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    process.kill()
            log.close()


if __name__ == "__main__":
    raise SystemExit(main())
