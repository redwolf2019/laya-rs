"""Publish small, locally verified provenance; never upload the bundle."""

import json
import shutil
from pathlib import Path

from prepare import CHECKPOINT, EXPORTER, OFFICIAL, PREP, ROOT, digest


def record():
    validation = json.loads((PREP / "validation.json").read_text())
    assert validation["status"] == "passed"
    for name, expected in validation["files"].items():
        assert digest(ROOT / name) == expected, name
    evidence = Path("docs/validation/model-prep")
    evidence.mkdir(exist_ok=True)
    for name in ("prepare.log", "export.log", "verify.log", "validation.json", "environment.log"):
        shutil.copyfile(PREP / name, evidence / name)
    files = list(validation["files"]) + [str(p.relative_to(ROOT)) for p in sorted((ROOT / "licenses").iterdir())]
    scripts = sorted(Path("tools/model-prep").glob("*.py"))
    scripts += [Path("tools/model-prep/requirements.lock"), Path("tools/model-prep/NOTICE.md")]
    manifest = dict(
        schema_version=1, status="verified", model="laya-multilingual", precision="fp32",
        checkpoint=dict(repository="convaiinnovations/laya-multilingual", revision=CHECKPOINT),
        official_source=dict(repository="he-jev/laya", revision=OFFICIAL),
        exporter_reference=dict(repository="receptron/laya", revision=EXPORTER),
        bundle_directory="models/multilingual",
        preparation_directory="models/multilingual/prep",
        image="python@sha256:a36c24f9cbdf4fd0f52d67f0823eeac19c2028c637cecc392d97f980d4fec56b",
        platform="linux/arm64", cpu_limit=8, memory_bytes=12884901888, swap_bytes=0,
        config=json.loads((ROOT / "laya_config.json").read_text()),
        files=[dict(path=n, bytes=(ROOT / n).stat().st_size, sha256=digest(ROOT / n)) for n in files],
        inputs=json.loads((PREP / "inputs.json").read_text()),
        tools=[dict(path=str(p), bytes=p.stat().st_size, sha256=digest(p)) for p in scripts],
        evidence=[dict(path=str(p), bytes=p.stat().st_size, sha256=digest(p)) for p in sorted(evidence.iterdir())],
        versions=validation["versions"], python=validation["python"],
        graph=validation["graph"], session=validation["session"], special_tokens=validation["special_tokens"],
        validation=dict(cases=len(validation["cases"]), providers=validation["providers"],
                        max_logit_error=max(c["max_logit_error"] for c in validation["cases"]),
                        max_act_error=max(c["max_act_error"] for c in validation["cases"])),
        reproduction="tools/model-prep/README.md",
    )
    Path("docs/model-manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    print("recorded", len(files), "bundle files")


if __name__ == "__main__":
    record()
