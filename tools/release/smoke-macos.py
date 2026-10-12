"""Real macOS install/start/stop/update/rollback acceptance; Python is development-only."""
import argparse
import hashlib
import os
from pathlib import Path
import signal
import socket
import subprocess
import tarfile
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/laya-macos.sh"


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def fields(path):
    return dict(line.split("=", 1) for line in path.read_text().splitlines())


def wait_for(check, process, seconds=240):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if check():
            return
        if process.poll() is not None:
            raise AssertionError(f"Command exited early: {process.returncode}")
        time.sleep(0.1)
    raise AssertionError("Timed out waiting for deployment command")


def make_candidate(source, out, fault):
    program = out / "program"
    program.mkdir()
    with tarfile.open(source / "laya-server-macos-aarch64.tar.gz") as archive:
        archive.extractall(program, filter="data")
    version = "v0.1.2-test-" + fault
    (program / "VERSION").write_text(version + "\n")
    if fault == "startup":
        (program / "libonnxruntime.dylib").write_bytes(b"intentional invalid native library")
    if fault == "api":
        (program / "smoke.json").write_text("{}")
    if fault == "architecture":
        subprocess.run(["/usr/bin/clang", "-arch", "x86_64", "-mmacosx-version-min=14.0", "-dynamiclib",
                        "-x", "c", "-", "-o", str(program / "libonnxruntime.dylib")],
                       input="int wrong_architecture_fixture(void) { return 0; }\n", text=True, check=True)
    (program / "program-files.tsv").unlink()
    paths = sorted(p for p in program.rglob("*") if p.is_file())
    (program / "program-files.tsv").write_text("".join(
        f"{sha(p)}\t{p.stat().st_size}\t{p.relative_to(program)}\n" for p in paths))
    archive_path = out / "laya-server-macos-aarch64.tar.gz"
    with tarfile.open(archive_path, "w:gz", format=tarfile.USTAR_FORMAT) as archive:
        for path in sorted(program.rglob("*")):
            if path.is_file():
                archive.add(path, arcname="./" + str(path.relative_to(program)), recursive=False)
    meta = fields(source / "release.env")
    meta.update(VERSION=version, MACOS_aarch64_SHA256=sha(archive_path), MACOS_aarch64_BYTES=str(archive_path.stat().st_size))
    (out / "release.env").write_text("".join(f"{k}={v}\n" for k, v in meta.items()))
    print(f"candidate={version} sha256={sha(archive_path)} fault={fault}", flush=True)


class Deployment:
    def __init__(self, root):
        self.root = root
        self.processes = []
        self.logs = root.parent / (root.name + "-logs")
        self.logs.mkdir()
        self.script = self.logs / "laya-macos.sh"
        self.script.write_bytes(SCRIPT.read_bytes())
        print(f"script_sha256={sha(self.script)}", flush=True)

    def command(self, action, *args, fails=False):
        result = subprocess.run(["/bin/bash", str(self.script), action, "--root", str(self.root), *map(str, args)],
                                capture_output=True, text=True, timeout=300)
        assert (result.returncode != 0) == fails, result.stdout + result.stderr
        print(f"PASS: {action} expected_failure={fails}\n{result.stdout}{result.stderr}", flush=True)
        return result

    def foreground(self, action, *args, marker="Validated."):
        log = self.logs / f"command-{len(self.processes)}.log"
        with log.open("w") as stream:
            process = subprocess.Popen(["/bin/bash", str(self.script), action, "--root", str(self.root), *map(str, args)],
                                       stdout=stream, stderr=subprocess.STDOUT)
        self.processes.append(process)
        if marker:
            wait_for(lambda: marker in log.read_text(), process)
            assert process.poll() is None
            print(f"PASS: {action} {marker} log={log}", flush=True)
        return process, log

    def close(self):
        if (self.root / "process.env").exists() and not (self.root / "transaction").exists():
            self.command("stop")
        for process in self.processes:
            if process.poll() is None:
                process.send_signal(signal.SIGTERM)
            process.wait(timeout=150)


def check_running(deploy, release, scratch):
    original = (deploy.root / "current.env").read_bytes()
    with socket.socket() as conflict:
        conflict.bind(("127.0.0.1", 18087)); conflict.listen()
        deploy.command("start", fails=True)
        assert not (deploy.root / "process.env").exists()
    process, log = deploy.foreground("start", marker=None)
    wait_for(lambda: (deploy.root / "process.env").exists(), process)
    process.send_signal(signal.SIGTERM)
    assert process.wait(timeout=150) == 143, log.read_text()
    assert not (deploy.root / "process.env").exists()
    assert (deploy.root / "current.env").read_bytes() == original
    print("PASS: port conflict and interrupted first start do not adopt a listener or leave a child", flush=True)
    deploy.foreground("start")
    before = fields(deploy.root / "process.env")["PID"]
    deploy.command("start", fails=True)
    assert fields(deploy.root / "process.env")["PID"] == before
    saved = (deploy.root / "process.env").read_text()
    (deploy.root / "process.env").write_text(saved.replace(f"PID={before}\n", f"PID={os.getpid()}\n"))
    assert "ownership cannot be confirmed" in deploy.command("stop", fails=True).stderr
    (deploy.root / "process.env").write_text(saved)
    bad = scratch / "bad"; bad.mkdir()
    make_candidate(release, bad, "api")
    with (bad / "laya-server-macos-aarch64.tar.gz").open("ab") as stream:
        stream.write(b"corrupt download")
    deploy.command("update", "--release-directory", bad, fails=True)
    assert fields(deploy.root / "process.env")["PID"] == before
    assert (deploy.root / "current.env").read_bytes() == original
    print("PASS: duplicate start, wrong PID and download failure preserve old live instance", flush=True)


def check_updates(deploy, release, scratch):
    for fault in ("version", "startup", "api", "architecture"):
        candidate = scratch / fault; candidate.mkdir()
        make_candidate(release, candidate, fault)
        old = fields(deploy.root / "current.env")
        marker = "Validated." if fault == "version" else "Previous deployment restored and validated."
        process, log = deploy.foreground("update", "--release-directory", candidate, marker=marker)
        current = fields(deploy.root / "current.env")
        if fault == "version":
            assert current["VERSION"] == "v0.1.2-test-version" and current["PROGRAM"] != old["PROGRAM"]
        else:
            assert current == old, log.read_text()
        for key in ("MODEL", "MODEL_ID", "LISTEN", "THREADS", "INTER", "SLOTS", "GRACE"):
            assert current[key] == old[key]
        deploy.command("stop")
        assert process.wait(timeout=150) == (0 if fault == "version" else 1), log.read_text()
        deploy.foreground("start")
    print("PASS: real A-to-B upgrade and startup/API-failure rollback", flush=True)


def check_interruption(deploy, release):
    before = fields(deploy.root / "process.env")["PID"]
    old = (deploy.root / "current.env").read_bytes()
    process, log = deploy.foreground("update", "--release-directory", release, marker=None)
    wait_for(lambda: (deploy.root / "transaction").exists() and (deploy.root / "process.env").exists()
             and fields(deploy.root / "process.env")["PID"] != before, process)
    assert "Operation lock" in deploy.command("update", "--release-directory", release, fails=True).stderr
    assert (deploy.root / "current.env").read_bytes() == old
    process.send_signal(signal.SIGTERM)
    assert process.wait(timeout=150) == 143, log.read_text()
    assert (deploy.root / "transaction/old.env").read_bytes() == old
    assert not (deploy.root / "process.env").exists()
    deploy.command("start", fails=True)
    deploy.foreground("update", "--recover", marker="Previous deployment restored and validated.")
    assert (deploy.root / "current.env").read_bytes() == old
    print("PASS: concurrent update rejected; interrupted real switch reclaims candidate; recovery validates old CPU inference", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release-directory", required=True, type=Path)
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--model-directory", type=Path)
    args = parser.parse_args()
    release = args.release_directory.resolve()
    deploy = Deployment(args.root.resolve())
    try:
        extra = ["--model-directory", args.model_directory.resolve()] if args.model_directory else []
        start = time.monotonic()
        deploy.command("install", "--release-directory", release, "--listen", "127.0.0.1:18087", *extra)
        print(f"install_seconds={time.monotonic()-start:.3f}", flush=True)
        assert not (deploy.root / "process.env").exists()
        token_hash = sha(deploy.root / "token")
        config = fields(deploy.root / "current.env")
        model = Path(config["MODEL"]) / "laya.onnx.data"
        stamp = model.stat().st_mtime_ns
        with tempfile.TemporaryDirectory(prefix="laya-candidates-") as temp:
            scratch = Path(temp)
            check_running(deploy, release, scratch)
            check_updates(deploy, release, scratch)
            check_interruption(deploy, release)
        deploy.command("stop")
        deploy.command("update", "--release-directory", release)
        assert not (deploy.root / "process.env").exists()
        assert sha(deploy.root / "token") == token_hash and model.stat().st_mtime_ns == stamp
        print("PASS: stopped state, unchanged model cache and token; real deployment gate", flush=True)
    finally:
        deploy.close()


if __name__ == "__main__":
    main()
