#!/usr/bin/env python3
"""Development-only acceptance of the built image; never copied into it.

Run from the repository root: python3 docs/validation/docker-smoke.py IMAGE BUNDLE
Requires Docker and Python stdlib on the validation host, not in the service.
"""
import concurrent.futures
import contextlib
import hashlib
import json
import math
import pathlib
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


def docker(*args, timeout=180):
    result = subprocess.run(["docker", *map(str, args)], capture_output=True,
                            text=True, timeout=timeout)
    if result.returncode:
        print(result.stderr, file=sys.stderr, flush=True)
        result.check_returncode()
    return result.stdout.strip()


def inspect(container):
    return json.loads(docker("inspect", container))[0]


def request(base, path, body=None):
    data = None if body is None else body.encode()
    req = urllib.request.Request(base + path, data=data,
                                 headers={"Content-Type": "application/json"})
    try:
        response = urllib.request.urlopen(req, timeout=150)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        return response.status, response.read().decode()


def wait_for(check, seconds=90):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if check():
            return
        time.sleep(0.05)
    raise AssertionError("deadline expired")


def ready(base):
    try:
        return request(base, "/readyz")[0] == 200
    except (urllib.error.URLError, ConnectionError):
        return False


def metric(base, line):
    status, body = request(base, "/metrics")
    assert status == 200
    return line in body.splitlines()


@contextlib.contextmanager
def container(image, bundle, *options, mounts=()):
    args = ["create", "--label", "laya.validation=18", "--platform", "linux/arm64", "--cpus", "8",
            "--memory", "12g", "--memory-swap", "12g", "--read-only",
            "--cap-drop", "ALL", "--security-opt", "no-new-privileges",
            "-p", "127.0.0.1::8080"]
    if bundle:
        args += ["--mount", f"type=bind,source={bundle},target=/models/multilingual,readonly"]
    for mount in mounts:
        args += ["--mount", mount]
    cid = docker(*args, image, "--model", "/models/multilingual",
                 "--threads", "1", "--max-concurrency", "1", *options)
    try:
        docker("start", cid)
        yield cid
    finally:
        try:
            result = subprocess.run(["docker", "logs", cid], capture_output=True,
                                    text=True, timeout=10, check=True)
            print(result.stdout + result.stderr, end="", flush=True)
            print("CONTAINER STATE", json.dumps(inspect(cid)["State"]), flush=True)
        finally:
            docker("rm", "-f", cid)


def base_url(cid):
    port = inspect(cid)["NetworkSettings"]["Ports"]["8080/tcp"][0]["HostPort"]
    base = f"http://127.0.0.1:{port}"
    wait_for(lambda: ready(base))
    print("READY 200", flush=True)
    return base


def exit_code(cid, expected):
    assert int(docker("wait", cid)) == expected
    state = inspect(cid)["State"]
    assert not state["OOMKilled"] and not state["Running"], state
    print(f"PASS exit={expected}, OOMKilled=false", flush=True)


def check_answer(actual, expected):
    assert actual["answers"].keys() == expected["answers"].keys()
    for key in expected["answers"]:
        a = actual["answers"][key].pop("rl_agent")
        e = expected["answers"][key].pop("rl_agent")
        assert a.keys() == e.keys() == {"act_probability"}
        x, y = a["act_probability"], e["act_probability"]
        assert math.isfinite(x) and abs(x - y) <= 1e-5 + 1e-4 * abs(y)
    assert actual == expected, "official rounded fields/usage/envelope differ"


def fixtures(base):
    root = pathlib.Path("tests/fixtures/system-one")
    manifest = json.loads((root / "manifest.json").read_text())
    count = 0
    for entry in manifest["files"]:
        data = json.loads((root / entry["path"]).read_text())
        if data["kind"] != "official_model":
            continue
        status, body = request(base, "/v1/system-one", data["request_json"])
        assert status == 200, (entry["path"], status, body)
        check_answer(json.loads(body), data["response"])
        print("PASS official fixture", entry["path"], flush=True)
        count += 1
    assert count == 21
    for line in ["laya_requests_total 21", "laya_inference_duration_seconds_count 21",
                 "laya_inference_inflight 0", "laya_queue_size 0"]:
        assert metric(base, line), line
    print(request(base, "/metrics")[1], flush=True)


def runtime_inventory(cid):
    script = """set -eu
id
test "$(id -u)" = 65532
test "$(cat /sys/fs/cgroup/cpu.max)" = '800000 100000'
test "$(cat /sys/fs/cgroup/memory.max)" = 12884901888
test "$(cat /sys/fs/cgroup/memory.swap.max)" = 0
for tool in python python3 node npm pip pip3 cargo rustc gcc g++ nvidia-smi; do
    if command -v "$tool"; then exit 1; fi
done
test ! -e /build && test ! -e /usr/local/cargo
test -s /etc/ssl/certs/ca-certificates.crt
test -s /usr/share/doc/onnxruntime/LICENSE
test -s /usr/share/doc/onnxruntime/ThirdPartyNotices.txt
test -s /usr/share/doc/laya-server/NOTICE.md
test -d /usr/share/doc/laya-server/dependencies/crates
dpkg-query -W
ldd /usr/local/bin/laya-server
ldd /opt/onnxruntime/lib/libonnxruntime.so
ldd /opt/onnxruntime/lib/libonnxruntime_providers_shared.so
cat /proc/1/maps
if touch /models/multilingual/write-probe; then exit 1; fi
"""
    output = docker("exec", cid, "sh", "-c", script)
    print(output, flush=True)
    assert "not found" not in output
    assert "/opt/onnxruntime/lib/libonnxruntime.so.1.28.0" in output
    assert not any(s in output.lower() for s in ["libcuda", "libtorch", "libnvidia", "libonnxruntime_providers_cuda"])
    state = inspect(cid)
    assert state["HostConfig"]["ReadonlyRootfs"]
    assert all(not mount["RW"] for mount in state["Mounts"])


def failures(image, bundle):
    cases = [("missing-directory", None, (), (), 2, "existing local directory"),
             ("abi", bundle, ("--ort-library", "/lib/aarch64-linux-gnu/libc.so.6"), (),
              1, "native ONNX Runtime load or ABI check failed")]
    with tempfile.TemporaryDirectory(prefix="laya-docker-") as directory:
        root = pathlib.Path(directory)
        root.chmod(0o755)
        empty = root / "empty"
        empty.mkdir(mode=0o755)
        bad = root / "laya.onnx"
        bad.write_bytes((bundle / "laya.onnx").read_bytes())
        data = bytearray(bad.read_bytes())
        data[0] ^= 1
        bad.write_bytes(data)
        mount = (f"type=bind,source={bad},target=/models/multilingual/laya.onnx,readonly",)
        cases += [("missing-file", empty, (), (), 1, "bundle file is missing or unreadable"),
                  ("corrupt", bundle, (), mount, 1, "SHA-256 differs from manifest")]
        for name, model, args, mounts, expected, message in cases:
            with container(image, model, *args, mounts=mounts) as cid:
                exit_code(cid, expected)
                result = subprocess.run(["docker", "logs", cid], capture_output=True,
                                        text=True, timeout=10, check=True)
                assert message in result.stdout + result.stderr
                print("PASS startup failure", name, flush=True)
    unreadable(image)


def unreadable(image):
    # A Linux volume avoids Docker Desktop needing host permission to bind the file.
    volume = docker("volume", "create", "--label", "laya.validation=18")
    mount = f"type=volume,source={volume},target=/models/multilingual"
    try:
        docker("run", "--rm", "--user", "0:0", "--mount", mount,
               "--entrypoint", "sh", image, "-c",
               "touch /models/multilingual/laya.onnx; chmod 000 /models/multilingual/laya.onnx")
        with container(image, None, mounts=(mount + ",readonly",)) as cid:
            exit_code(cid, 1)
            result = subprocess.run(["docker", "logs", cid], capture_output=True,
                                    text=True, timeout=10, check=True)
            assert "bundle file is missing or unreadable" in result.stderr
            print("PASS startup failure unreadable (Linux mode 000, UID 65532)", flush=True)
    finally:
        docker("volume", "rm", volume)


def stop_scenarios(image, bundle):
    long = json.loads(pathlib.Path("tests/fixtures/system-one/long-padding.json").read_text())["request_json"]
    for mode in ["drain", "expire"]:
        grace, inference = (120, 120) if mode == "drain" else (1, 1)
        with container(image, bundle, "--shutdown-grace", str(grace),
                       "--inference-timeout", str(inference)) as cid:
            base = base_url(cid)
            with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
                active = pool.submit(request, base, "/v1/system-one", long)
                wait_for(lambda: metric(base, "laya_inference_inflight 1"))
                if mode == "expire":
                    assert active.result()[0] == 504
                    assert metric(base, "laya_inference_inflight 1")
                    print("HTTP=504, CPU inflight=1", flush=True)
                queued = pool.submit(request, base, "/v1/system-one", long)
                wait_for(lambda: metric(base, "laya_queue_size 1"))
                stopped = pool.submit(docker, "stop", "--timeout", str(grace + 10), cid)
                wait_for(lambda: request(base, "/readyz")[0] == 503, seconds=3)
                assert request(base, "/healthz")[0] == 200
                assert queued.result()[0] == 503
                assert metric(base, "laya_queue_size 0")
                print(mode, "ready=503, health=200, queued=503, queue=0", flush=True)
                stopped.result()
                if mode == "drain":
                    assert active.result()[0] == 200
            exit_code(cid, 0 if mode == "drain" else 1)
            assert not ready(base)


def main():
    image, model = sys.argv[1:]
    bundle = pathlib.Path(model).resolve(strict=True)
    print(docker("image", "inspect", image), flush=True)
    for path in [pathlib.Path("Cargo.lock"), pathlib.Path("docs/model-manifest.json"),
                 bundle / "laya.onnx", bundle / "laya.onnx.data"]:
        with path.open("rb") as source:
            print(hashlib.file_digest(source, "sha256").hexdigest(), path.name, flush=True)
    failures(image, bundle)
    with container(image, bundle) as cid:
        base = base_url(cid)
        runtime_inventory(cid)
        assert request(base, "/healthz") == (200, '{"status":"ok"}')
        fixtures(base)
        docker("stop", "--timeout", "130", cid)
        exit_code(cid, 0)
        assert not ready(base)
    stop_scenarios(image, bundle)
    print("PASS: image, 21 official HTTP fixtures, startup failures, idle/drain/expired Docker stop", flush=True)


if __name__ == "__main__":
    main()
