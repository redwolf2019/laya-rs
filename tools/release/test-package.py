"""Exercise release metadata through the packaging CLI, without learned weights."""
import io
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
with tempfile.TemporaryDirectory() as temp:
    out = Path(temp)
    model_id = subprocess.check_output([sys.executable, str(ROOT / "tools/release/package.py"), "id"], text=True).strip()
    (out / f"model-{model_id}.tar.gz").write_bytes(b"test-only model archive")
    for platform in ("linux-x86_64", "linux-aarch64", "macos-aarch64"):
        with tarfile.open(out / f"laya-server-{platform}.tar.gz", "w:gz") as archive:
            for name, value in (("VERSION", "v0.1.2"), ("MODEL_ID", model_id)):
                data = (value + "\n").encode()
                info = tarfile.TarInfo("./" + name)
                info.size = len(data)
                archive.addfile(info, io.BytesIO(data))
    with zipfile.ZipFile(out / "laya-server-windows-x86_64.zip", "w") as archive:
        archive.writestr("VERSION", "v0.1.2\n")
        archive.writestr("MODEL_ID", model_id + "\n")
    subprocess.run([sys.executable, str(ROOT / "tools/release/package.py"), "release", "--version", "v0.1.2", "--out", str(out)], check=True)
    metadata = dict(line.split("=", 1) for line in (out / "release.env").read_text().splitlines())
    assert len(metadata["WINDOWS_x86_64_SHA256"]) == 64
    assert int(metadata["WINDOWS_x86_64_BYTES"]) > 0
    assert (out / "laya.ps1").is_file()
    checksums = (out / "SHA256SUMS").read_text()
    assert "laya-server-windows-x86_64.zip" in checksums
    assert "laya.ps1" in checksums
    assert len(metadata["MACOS_aarch64_SHA256"]) == 64
    assert int(metadata["MACOS_aarch64_BYTES"]) > 0
    assert "laya-server-macos-aarch64.tar.gz" in checksums
    assert "laya-macos.sh" in checksums
    assert (out / "laya-macos.sh").is_file()
    # Windows metadata stays separate; Linux installers retain their existing fields.
    assert "RUNTIME_x86_64_SHA256" in metadata and "RUNTIME_aarch64_SHA256" in metadata
print("PASS: macOS/Windows assets and Linux metadata compatibility")

if os.name == "posix":
    build_macos = runpy.run_path(ROOT / "tools/release/build-macos.py")
    with tempfile.TemporaryDirectory() as temp:
        directory = Path(temp)
        package, ort, commands = (directory / name for name in ("package", "ort", "bin"))
        for path in (package, ort, commands):
            path.mkdir()
        for name in ("LICENSE", "ThirdPartyNotices.txt"):
            (ort / name).write_text("ORT test license\n")
        rust = directory / "share/doc/rust"
        (rust / "licenses").mkdir(parents=True)
        (rust / "licenses/test.txt").write_text("Rust test license\n")
        (rust / "COPYRIGHT.html").write_text("Rust test copyright\n")
        cargo = commands / "cargo"
        cargo.write_text("#!/bin/sh\nprintf 'Fetching metadata fixture\\n' >&2\nprintf '{\"packages\":[]}\\n'\n")
        rustc = commands / "rustc"
        rustc.write_text(f"#!{sys.executable}\nprint({str(directory)!r})\n")
        for path in (cargo, rustc):
            path.chmod(0o755)
        with patch.dict(os.environ, PATH=str(commands) + os.pathsep + os.environ["PATH"]):
            build_macos["licenses"](package, ort)
        assert (package / "licenses/ORT-LICENSE").read_text() == "ORT test license\n"
        assert (package / "licenses/rust/test.txt").read_text() == "Rust test license\n"
    print("PASS: macOS license collection parses stdout despite metadata diagnostics")
