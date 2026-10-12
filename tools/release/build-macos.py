"""Build/package the native ARM64 application with a prepared CPU ORT; development only."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def run(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True, stderr=subprocess.STDOUT).strip()


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def inspect_macho(path, package):
    if run("lipo", "-archs", str(path)) != "arm64":
        raise ValueError("Package contains a non-ARM64 executable")
    build = run("vtool", "-show-build", str(path))
    minimum = re.search(r"minos\s+(\S+)", build)
    if not minimum or tuple(map(int, minimum[1].split("."))) > (14, 0):
        raise ValueError("Mach-O exceeds the minimum OS")
    deps = run("otool", "-L", str(path))
    for line in deps.splitlines()[1 + (path.suffix == ".dylib"):]:
        dependency = line.strip().split(" (", 1)[0]
        if dependency.startswith(("/System/Library/", "/usr/lib/")):
            continue
        if dependency.startswith(("@loader_path/", "@rpath/")) and (package / dependency.split("/", 1)[1]).is_file():
            continue
        raise ValueError(f"Unbundled or build-host dependency: {dependency}")
    run("codesign", "--force", "--sign", "-", str(path))
    run("codesign", "--verify", "--strict", str(path))
    return build + "\n" + deps + "\n" + run("codesign", "-dv", "--verbose=4", str(path))


def licenses(package, ort):
    destination = package / "licenses"
    destination.mkdir()
    for name in ("LICENSE", "NOTICE.md", "Cargo.lock"):
        shutil.copy2(ROOT / name, destination / name)
    shutil.copytree(ROOT / "licenses", destination / "project")
    for name in ("LICENSE", "ThirdPartyNotices.txt"):
        shutil.copy2(ort / name, destination / ("ORT-" + name))
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"], cwd=ROOT, text=True))
    for crate in metadata["packages"]:
        if not crate["source"]:
            continue
        source = Path(crate["manifest_path"]).parent
        for path in source.rglob("*"):
            if path.is_file() and re.search(r"license|copying|copyright|notice|^Cargo.toml$", path.name, re.I):
                target = destination / "crates" / source.name / path.relative_to(source)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, target)
    rust = Path(run("rustc", "--print", "sysroot")) / "share/doc/rust"
    shutil.copytree(rust / "licenses", destination / "rust")
    for path in rust.glob("COPYRIGHT*.html"):
        shutil.copy2(path, destination / "rust" / path.name)


def catalog(package):
    lines = []
    for path in sorted(package.rglob("*")):
        if not path.is_file():
            continue
        name = path.relative_to(package).as_posix()
        if path.is_symlink() or not re.fullmatch(r"[A-Za-z0-9_./+-]+", name):
            raise ValueError(f"Unsafe package file: {name}")
        lines.append(f"{digest(path)}\t{path.stat().st_size}\t{name}\n")
    (package / "program-files.tsv").write_text("".join(lines))


def assemble(args, package):
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    shutil.copy2(target / "aarch64-apple-darwin/release/laya-server", package)
    shutil.copy2(args.ort / "libonnxruntime.dylib", package)
    shutil.copy2(args.ort / "build-record.txt", package / "ort-build.txt")
    shutil.copy2(args.ort / "SHA256SUMS", package / "ort-sha256.txt")
    licenses(package, args.ort)
    for name in ("smoke.json", "response-macos.js"):
        shutil.copy2(ROOT / "tools/release" / name, package)
    manifest = ROOT / "docs/model-manifest.json"
    shutil.copy2(manifest, package)
    (package / "MODEL_ID").write_text(digest(manifest)[:16] + "\n")
    (package / "VERSION").write_text(args.version + "\n")
    entries = json.loads(manifest.read_text())["files"]
    (package / "model-files.tsv").write_text("".join(f"{e['sha256']}\t{e['bytes']}\t{e['path']}\n" for e in entries))
    inspections = [inspect_macho(package / name, package) for name in ("laya-server", "libonnxruntime.dylib")]
    (package / "macho-signatures.txt").write_text("\n".join(inspections))
    source = [ROOT / n for n in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml")]
    source += sorted((ROOT / "src").rglob("*.rs"))
    record = {"Source": run("git", "rev-parse", "HEAD"), "Dirty": bool(run("git", "status", "--porcelain")),
              "Rust": run("rustc", "-Vv"), "Xcode": run("xcodebuild", "-version"), "SDK": run("xcrun", "--show-sdk-version"),
              "OS": run("sw_vers"), "Target": "aarch64-apple-darwin", "DeploymentTarget": "14.0",
              "ORT": "1.28.0 CPU source build", "Notarized": False,
              "Files": {str(p.relative_to(ROOT)): digest(p) for p in source}}
    (package / "build.json").write_text(json.dumps(record, indent=2) + "\n")
    catalog(package)
    run(str(package / "laya-server"), "--help")
    with tarfile.open(args.out / "laya-server-macos-aarch64.tar.gz", "w:gz", format=tarfile.USTAR_FORMAT) as archive:
        for path in sorted(package.rglob("*")):
            if path.is_file():
                info = archive.gettarinfo(path, "./" + str(path.relative_to(package)))
                info.uid = info.gid = info.mtime = 0
                info.uname = info.gname = "root"
                info.mode = 0o755 if path.name == "laya-server" or path.suffix == ".dylib" else 0o644
                with path.open("rb") as stream:
                    archive.addfile(info, stream)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--ort", type=Path, required=True)
    parser.add_argument("--out", type=Path, default=ROOT / "dist")
    args = parser.parse_args()
    if platform.system() != "Darwin" or platform.machine() != "arm64" or not re.fullmatch(r"v\d+[A-Za-z0-9.+-]*", args.version):
        parser.error("Native ARM64 macOS and a valid version are required")
    os.environ["MACOSX_DEPLOYMENT_TARGET"] = "14.0"
    if any(os.environ.get(v) for v in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CFLAGS", "CXXFLAGS")):
        parser.error("Remove custom CPU/compiler flags for a portable release")
    args.ort = args.ort.resolve(); args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    subprocess.run(["cargo", "build", "--release", "--locked", "--target", "aarch64-apple-darwin"], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="macos-build-", dir=args.out) as temp:
        assemble(args, Path(temp))
        unpacked = Path(temp) / "unpacked"
        with tarfile.open(args.out / "laya-server-macos-aarch64.tar.gz") as archive:
            archive.extractall(unpacked, filter="data")
        for name in ("laya-server", "libonnxruntime.dylib"):
            run("codesign", "--verify", "--strict", str(unpacked / name))
    print("Built unsigned-identity, ad-hoc signed macOS ARM64 candidate:", args.out)


if __name__ == "__main__":
    main()
