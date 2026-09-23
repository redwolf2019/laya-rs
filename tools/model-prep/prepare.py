"""Download and verify immutable inputs; only used during model preparation."""

import hashlib
import json
import shutil
from pathlib import Path
from urllib.request import urlopen

ROOT = Path("models/multilingual")
PREP = ROOT / "prep"
CHECKPOINT = "052592a15d198d9ad47da779604259b10b47b7aa"
OFFICIAL = "c5d78730f3493e4fe16d61507ef4b78eef7318cf"
EXPORTER = "6478649e723122ca24bbf5fb69ed1010023c9750"
HUB = "https://huggingface.co/convaiinnovations/laya-multilingual"
SOURCE_HASHES = {
    "rl_common.py": "8d83611d480c971d640a7b7d3aa2f2219c5e8455e9cc2329fd073681bd8be23e",
    "official-README.md": "575cb8fb4fd82465036a9d265c283fec9448b78067e6045afac8cfa113dde98f",
    "export_onnx.py": "a4da86abfe118779f8f0d5ac25d6213844f405d8feb1d222338fc98170d56b4b",
    "receptron-LICENSE": "c07ddef02172ce4162a2aae8ebddd5c2b84d8374da865c592afea5710e2acc0c",
}


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def download(url, path):
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists():
        temporary = path.with_name(path.name + ".part")
        with urlopen(url, timeout=120) as source, temporary.open("wb") as target:
            while block := source.read(1024 * 1024):
                target.write(block)
        temporary.replace(path)


def checkpoint_files():
    metadata = PREP / "hub-metadata.json"
    download(f"https://huggingface.co/api/models/convaiinnovations/laya-multilingual/revision/{CHECKPOINT}?blobs=true", metadata)
    data = json.loads(metadata.read_text())
    assert data["sha"] == CHECKPOINT
    records = []
    for entry in data["siblings"]:
        name = entry["rfilename"]
        assert not Path(name).is_absolute() and ".." not in Path(name).parts
        path = PREP / "checkpoint" / name
        url = f"{HUB}/resolve/{CHECKPOINT}/{name}"
        download(url, path)
        sha = digest(path)
        assert path.stat().st_size == entry["size"], name
        if "lfs" in entry:
            assert sha == entry["lfs"]["sha256"], name
        else:
            raw = path.read_bytes()
            blob = hashlib.sha1(f"blob {len(raw)}\0".encode() + raw).hexdigest()
            assert blob == entry["blobId"], name
        records.append(dict(path=str(path), url=url, bytes=path.stat().st_size,
                            sha256=sha, publisher=entry))
        print("verified", name, sha, flush=True)
    return records


def source_files():
    sources = {
        "rl_common.py": ("he-jev/laya", OFFICIAL, "rl_common.py"),
        "official-README.md": ("he-jev/laya", OFFICIAL, "README.md"),
        "export_onnx.py": ("receptron/laya", EXPORTER, "export/export_onnx.py"),
        "receptron-LICENSE": ("receptron/laya", EXPORTER, "LICENSE"),
    }
    records = []
    for name, (repo, revision, upstream) in sources.items():
        url = f"https://raw.githubusercontent.com/{repo}/{revision}/{upstream}"
        path = PREP / "source" / name
        download(url, path)
        assert digest(path) == SOURCE_HASHES[name], name
        records.append(dict(path=str(path), url=url, bytes=path.stat().st_size,
                            sha256=digest(path)))
    return records


if __name__ == "__main__":
    records = checkpoint_files() + source_files()
    license_file = PREP / "source/Apache-2.0.txt"
    download("https://www.apache.org/licenses/LICENSE-2.0.txt", license_file)
    assert digest(license_file) == "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30"
    licenses = ROOT / "licenses"
    licenses.mkdir(exist_ok=True)
    for source, name in [(license_file, "Apache-2.0.txt"),
                         (PREP / "source/official-README.md", "official-README.md"),
                         (PREP / "checkpoint/README.md", "model-README.md"),
                         (PREP / "source/receptron-LICENSE", "receptron-LICENSE"),
                         (Path("tools/model-prep/NOTICE.md"), "NOTICE.md")]:
        shutil.copyfile(source, licenses / name)
    (PREP / "inputs.json").write_text(json.dumps(records, indent=2) + "\n")
