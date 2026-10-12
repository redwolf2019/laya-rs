"""Public macOS deployment-command boundaries; development Python only, no model."""
from pathlib import Path
import hashlib
import io
import json
import os
import signal
import socket
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/laya-macos.sh"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def candidate(directory):
    program = directory / "program"
    model = directory / "model"
    program.mkdir(); model.mkdir()
    (model / "weights").write_bytes(b"fixture, not a model")
    manifest = json.dumps({"files": [{"path": "weights", "bytes": 20,
                                      "sha256": sha(model / "weights")}]})
    (program / "model-manifest.json").write_text(manifest)
    model_id = sha(program / "model-manifest.json")[:16]
    files = {"VERSION": "v0.1.2\n", "MODEL_ID": model_id + "\n",
             "laya-server": "#!/bin/sh\nexit 1\n", "libonnxruntime.dylib": "fixture",
             "model-files.tsv": f"{sha(model / 'weights')}\t20\tweights\n",
             "smoke.json": (ROOT / "tools/release/smoke.json").read_text()}
    files["response-macos.js"] = (ROOT / "tools/release/response-macos.js").read_text()
    for name, value in files.items():
        (program / name).write_text(value)
    (program / "laya-server").chmod(0o700)
    (program / "program-files.tsv").write_text("".join(
        f"{sha(p)}\t{p.stat().st_size}\t{p.name}\n" for p in sorted(program.iterdir())))
    for source, name in [(program, "laya-server-macos-aarch64.tar.gz"),
                         (model, f"model-{model_id}.tar.gz")]:
        with tarfile.open(directory / name, "w:gz", format=tarfile.USTAR_FORMAT) as archive:
            for path in sorted(source.iterdir()):
                archive.add(path, arcname=path.name)
    runtime = directory / "laya-server-macos-aarch64.tar.gz"
    weights = directory / f"model-{model_id}.tar.gz"
    (directory / "release.env").write_text(
        f"VERSION=v0.1.2\nMODEL_ID={model_id}\nMODEL_SHA256={sha(weights)}\n"
        f"MODEL_BYTES={weights.stat().st_size}\nMACOS_aarch64_SHA256={sha(runtime)}\n"
        f"MACOS_aarch64_BYTES={runtime.stat().st_size}\n")
    return model_id


class Commands(unittest.TestCase):
    def run_script(self, action, root, *args):
        return subprocess.run(["/bin/bash", str(SCRIPT), action, "--root", str(root), *map(str, args)],
                              capture_output=True, text=True, timeout=30)

    def test_invalid_configuration_does_not_create_deployment(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "中文 path's deployment"
            result = subprocess.run(["/bin/bash", str(SCRIPT), "install", "--root", str(root),
                                     "--threads", "0"], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Invalid threads", result.stderr)
            self.assertFalse(root.exists())

    def test_install_only_prepares_and_reuses_private_token(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            model_id = candidate(directory)
            root = directory / "中文 path's deployment"
            result = subprocess.run(["/bin/bash", str(SCRIPT), "install", "--root", str(root),
                                     "--release-directory", str(directory)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue((root / "current.env").is_file())
            self.assertFalse((root / "process.env").exists())
            self.assertTrue((root / "models" / model_id / "weights").is_file())
            token = (root / "token").read_text().strip()
            self.assertRegex(token, "^[0-9a-f]{64}$")
            self.assertEqual((root / "token").stat().st_mode & 0o777, 0o600)
            self.assertNotIn(token, result.stdout + result.stderr)
            self.assertFalse((root / "operation.lock").exists())
            (root / "process.env").write_text(f"PID=999999999\nUID={os.getuid()}\nROOT={root.resolve()}\n")
            self.assertEqual(self.run_script("stop", root).returncode, 0)
            self.assertFalse((root / "process.env").exists())
            # The prepared executable intentionally cannot start: failure must leave no owned child.
            failed = subprocess.run(["/bin/bash", str(SCRIPT), "start", "--root", str(root)],
                                    capture_output=True, text=True)
            self.assertNotEqual(failed.returncode, 0)
            self.assertIn("Candidate", failed.stderr)
            self.assertEqual((root / "token").read_text().strip(), token)
            self.assertFalse((root / "process.env").exists())
            failed_update = self.run_script("update", root, "--release-directory", directory)
            self.assertNotEqual(failed_update.returncode, 0)
            self.assertIn("Rollback failed", failed_update.stderr)
            self.assertTrue((root / "transaction/old.env").is_file())
            self.assertEqual((root / "token").read_text().strip(), token)

    def test_unsafe_archives_are_rejected_before_install(self):
        for name, kind in [("../escape", tarfile.REGTYPE), ("/tmp/escape", tarfile.REGTYPE),
                           ("alias", tarfile.SYMTYPE), ("hard", tarfile.LNKTYPE),
                           ("fifo", tarfile.FIFOTYPE), ("bad\nname", tarfile.REGTYPE)]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temp:
                directory = Path(temp)
                candidate(directory)
                archive_path = directory / "laya-server-macos-aarch64.tar.gz"
                with tarfile.open(archive_path, "w:gz", format=tarfile.USTAR_FORMAT) as archive:
                    info = tarfile.TarInfo(name); info.type = kind
                    if kind in (tarfile.SYMTYPE, tarfile.LNKTYPE):
                        info.linkname = "/tmp/escape"
                    archive.addfile(info, io.BytesIO())
                metadata = directory / "release.env"
                lines = [line for line in metadata.read_text().splitlines() if not line.startswith("MACOS_")]
                metadata.write_text("\n".join(lines) + f"\nMACOS_aarch64_SHA256={sha(archive_path)}\nMACOS_aarch64_BYTES={archive_path.stat().st_size}\n")
                root = directory / "deployment"
                result = self.run_script("install", root, "--release-directory", directory)
                self.assertNotEqual(result.returncode, 0, name)
                self.assertFalse((root / "current.env").exists())
                self.assertFalse((root / "token").exists())

    def test_browser_quarantine_is_preserved_on_unpacked_files(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp); candidate(directory)
            archive = directory / "laya-server-macos-aarch64.tar.gz"
            value = "0081;00000000;Laya-Test;"
            subprocess.run(["/usr/bin/xattr", "-w", "com.apple.quarantine", value, str(archive)], check=True)
            root = directory / "deployment"
            result = self.run_script("install", root, "--release-directory", directory)
            self.assertEqual(result.returncode, 0, result.stderr)
            executable = next((root / "programs").glob("*/laya-server"))
            actual = subprocess.run(["/usr/bin/xattr", "-p", "com.apple.quarantine", str(executable)],
                                    capture_output=True, text=True)
            self.assertEqual(actual.stdout.strip(), value)

    def test_lock_corrupt_download_and_config_are_not_executed(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp); candidate(directory)
            root = directory / "deployment"
            self.assertEqual(self.run_script("install", root, "--release-directory", directory).returncode, 0)
            original = (root / "current.env").read_bytes()
            token = (root / "token").read_bytes()
            lock = root / "operation.lock"; lock.mkdir()
            (lock / "owner").write_text("PID=999999\n")
            result = self.run_script("update", root, "--release-directory", directory)
            self.assertIn("Operation lock", result.stderr)
            (lock / "owner").unlink(); lock.rmdir()
            with (directory / "laya-server-macos-aarch64.tar.gz").open("ab") as stream:
                stream.write(b"broken")
            result = self.run_script("update", root, "--release-directory", directory)
            self.assertIn("verification failed", result.stderr)
            self.assertEqual((root / "current.env").read_bytes(), original)
            self.assertEqual((root / "token").read_bytes(), token)
            marker = directory / "must-not-exist"
            (root / "current.env").write_text(original.decode().replace("THREADS=2", f"THREADS=$(touch {marker})"))
            self.assertIn("Invalid configuration limits", self.run_script("start", root).stderr)
            self.assertFalse(marker.exists())
            self.assertFalse((root / "process.env").exists())

    def test_interrupt_download_reaps_transfer_and_keeps_old_deployment(self):
        with tempfile.TemporaryDirectory() as temp, socket.socket() as proxy:
            directory = Path(temp); candidate(directory)
            root = directory / "deployment"
            self.assertEqual(self.run_script("install", root, "--release-directory", directory).returncode, 0)
            original = (root / "current.env").read_bytes()
            token = (root / "token").read_bytes()
            proxy.bind(("127.0.0.1", 0)); proxy.listen(); proxy.settimeout(10)
            process = subprocess.Popen(["/bin/bash", str(SCRIPT), "update", "--root", str(root), "--version", "v0.1.2"],
                                       env={**os.environ, "https_proxy": f"http://127.0.0.1:{proxy.getsockname()[1]}"},
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
            try:
                connection, _ = proxy.accept()
                with connection:
                    self.assertIn(b"CONNECT github.com:443", connection.recv(4096))
                    process.send_signal(signal.SIGTERM)
                    self.assertEqual(process.wait(timeout=3), 143)
                    connection.settimeout(3)
                    self.assertEqual(connection.recv(4096), b"")
                self.assertEqual((root / "current.env").read_bytes(), original)
                self.assertEqual((root / "token").read_bytes(), token)
                self.assertFalse((root / "operation.lock").exists())
                self.assertEqual(list((root / "staging").iterdir()), [])
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGTERM)
                process.communicate(timeout=10)

    def test_permission_failure_does_not_create_deployment(self):
        with tempfile.TemporaryDirectory() as temp:
            parent = Path(temp) / "read-only"; parent.mkdir(); parent.chmod(0o500)
            try:
                root = parent / "deployment"
                result = self.run_script("install", root)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(root.exists())
            finally:
                parent.chmod(0o700)


if __name__ == "__main__":
    unittest.main()
