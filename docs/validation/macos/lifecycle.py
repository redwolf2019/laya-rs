"""Native CPU HTTP/signal acceptance; no simulation of the inference worker."""
import argparse
import concurrent.futures
import http.client
import json
import os
from pathlib import Path
import secrets
import signal
import socket
import subprocess
import time

PORT = 18088


def request(path, token, body=None):
    connection = http.client.HTTPConnection("127.0.0.1", PORT, timeout=135)
    try:
        connection.request("POST" if body is not None else "GET", path, body=body,
                           headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
        response = connection.getresponse()
        return response.status, response.read().decode()
    finally:
        connection.close()


def wait_for(predicate, process, seconds=120):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        assert process.poll() is None, "Native process exited before validation"
        try:
            if predicate():
                return
        except OSError:
            pass
        time.sleep(0.02)
    raise AssertionError("Native validation deadline expired")


def metric(token, expected):
    status, text = request("/metrics", token)
    return status == 200 and expected + "\n" in text


def start(args, case, token, timed):
    log = args.out / (case + ".log")
    with log.open("w") as stream:
        process = subprocess.Popen([str(args.program / "laya-server"), "--model", str(args.model),
                                    "--listen", f"127.0.0.1:{PORT}", "--threads", "1", "--max-concurrency", "1",
                                    "--queue-capacity", "1", "--queue-timeout", "1",
                                    "--inference-timeout", "1" if timed else "120",
                                    "--shutdown-grace", "1" if timed else "120"],
                                   env={**os.environ, "LAYA_API_TOKEN": token}, cwd=args.out, stderr=stream)
    try:
        wait_for(lambda: request("/readyz", token) == (200, '{"status":"ready"}'), process)
        listeners = subprocess.check_output(["/usr/sbin/lsof", "-nP", f"-iTCP:{PORT}", "-sTCP:LISTEN", "-Fp"], text=True)
        assert {line for line in listeners.splitlines() if line.startswith("p")} == {f"p{process.pid}"}
        return process, log
    except BaseException:
        process.terminate(); process.wait(timeout=135)
        raise


def check_signal(args, case, sig, timed, idle=False):
    token = secrets.token_hex(32)
    started = time.monotonic()
    process, log = start(args, case, token, timed)
    print(f"{case} cold_start_seconds={time.monotonic()-started:.3f}", flush=True)
    try:
        if not idle:
            run_requests(process, token, sig, timed)
        else:
            process.send_signal(sig)
        assert process.wait(timeout=135) == int(timed)
        text = log.read_text()
        assert ("shutdown_grace_expired" if timed else "shutdown_complete") in text
        assert token not in text
        with socket.socket() as listener:
            listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            listener.bind(("127.0.0.1", PORT))
        print(f"PASS: {case}, signal={sig.name}, exit={process.returncode}, port rebind", flush=True)
    finally:
        if process.poll() is None:
            process.terminate(); process.wait(timeout=135)


def run_requests(process, token, sig, timed):
    body = json.dumps({"state": "Customer requests an urgent refund. " * 2000,
                       "questions": {f"q{i}": {"type": "noul", "instructions": "Is a refund required?"}
                                     for i in range(16)}}).encode()
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        running = pool.submit(request, "/v1/system-one", token, body)
        wait_for(lambda: metric(token, "laya_inference_inflight 1"), process)
        print(subprocess.check_output(["/bin/ps", "-p", str(process.pid), "-o", "rss=,time=,pcpu="], text=True).strip(), flush=True)
        queued = pool.submit(request, "/v1/system-one", token, body)
        wait_for(lambda: metric(token, "laya_queue_size 1"), process)
        assert request("/v1/system-one", token, body)[0] == 429
        assert queued.result()[0] == 503  # one-second queue timeout
        if timed:
            assert running.result()[0] == 504
            wait_for(lambda: metric(token, "laya_inference_inflight 1"), process)
        queued = pool.submit(request, "/v1/system-one", token, body)
        wait_for(lambda: metric(token, "laya_queue_size 1"), process)
        process.send_signal(sig)
        status, text = queued.result()
        assert status == 503 and json.loads(text)["error"]["code"] == "unavailable"
        if not timed:
            assert running.result()[0] == 200


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--program", type=Path, required=True)
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.program = args.program.resolve(); args.model = args.model.resolve(); args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    for sig in (signal.SIGINT, signal.SIGTERM):
        for kind in ("idle", "drain", "timed"):
            check_signal(args, f"{kind}-{sig.name}", sig, kind == "timed", kind == "idle")


if __name__ == "__main__":
    main()
