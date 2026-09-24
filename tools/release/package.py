"""Verify/package the fixed model and assemble release metadata; development only."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re
import shutil
import tarfile

ROOT = Path(__file__).resolve().parents[2]


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def model_id():
    return digest(ROOT / "docs/model-manifest.json")[:16]


def pack_model(directory, out):
    manifest = json.loads((ROOT / "docs/model-manifest.json").read_text())
    for entry in manifest["files"]:
        path = directory / entry["path"]
        if path.is_symlink() or path.stat().st_size != entry["bytes"] or digest(path) != entry["sha256"]:
            raise ValueError(f"Model verification failed: {entry['path']}")
    target = out / f"model-{model_id()}.tar.gz"
    with target.open("wb") as raw, gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=0) as zipped:
        with tarfile.open(fileobj=zipped, mode="w") as archive:
            files = [(directory / e["path"], e["path"]) for e in manifest["files"]]
            files += [(ROOT / "docs/model-manifest.json", "model-manifest.json"),
                      (ROOT / "tools/release/MODEL-NOTICE.md", "MODEL-NOTICE.md")]
            for path, name in files:
                info = archive.gettarinfo(path, arcname=name)
                info.uid = info.gid = info.mtime = 0
                info.uname = info.gname = "root"
                info.mode = 0o644
                with path.open("rb") as stream:
                    archive.addfile(info, stream)
    print(f"Verified model: {target.name}; {target.stat().st_size} bytes; {digest(target)}")


def release(version, out):
    if not re.fullmatch(r"v\d+[A-Za-z0-9.+-]*", version):
        raise ValueError("Invalid release version")
    model = out / f"model-{model_id()}.tar.gz"
    lines = [f"VERSION={version}", f"MODEL_ID={model_id()}",
             f"MODEL_SHA256={digest(model)}", f"MODEL_BYTES={model.stat().st_size}"]
    for arch in ("x86_64", "aarch64"):
        path = out / f"laya-server-linux-{arch}.tar.gz"
        with tarfile.open(path) as archive:
            if archive.extractfile("./VERSION").read().decode().strip() != version:
                raise ValueError(f"Wrong program version: {arch}")
            if archive.extractfile("./MODEL_ID").read().decode().strip() != model_id():
                raise ValueError(f"Wrong model version: {arch}")
        lines += [f"RUNTIME_{arch}_SHA256={digest(path)}", f"RUNTIME_{arch}_BYTES={path.stat().st_size}"]
    (out / "release.env").write_text("\n".join(lines) + "\n")
    shutil.copyfile(ROOT / "scripts/install.sh", out / "install.sh")
    assets = [out / "release.env", out / "install.sh"]
    assets += sorted(out.glob("laya-server-linux-*.tar.gz"))
    assets += sorted(out.glob("runtime-sources-*.tar.gz"))
    (out / "SHA256SUMS").write_text("".join(f"{digest(p)}  {p.name}\n" for p in assets))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("model", "release", "id"))
    parser.add_argument("--model", type=Path, default=ROOT / "models/multilingual")
    parser.add_argument("--out", type=Path, default=ROOT / "dist")
    parser.add_argument("--version", default="v0.1.1")
    args = parser.parse_args()
    if args.action == "id":
        print(model_id())
        return
    args.out.mkdir(parents=True, exist_ok=True)
    if args.action == "model":
        pack_model(args.model, args.out)
    else:
        release(args.version, args.out)


if __name__ == "__main__":
    main()
