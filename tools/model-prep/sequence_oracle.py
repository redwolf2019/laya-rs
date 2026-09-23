"""#12 synthetic-budget oracle using unchanged pinned official sequence functions.

Run in the #7 container; only loads the tokenizer, never the model. Output must be new.
"""

import hashlib
import json
import platform
import sys
from importlib.metadata import version
from pathlib import Path


def generate():
    root = Path("models/multilingual")
    manifest = json.loads(Path("docs/model-manifest.json").read_text())
    paths = ["tokenizer/tokenizer.json", "tokenizer/tokenizer_config.json"]
    sources = {str(root / f["path"]): f["sha256"] for f in manifest["files"] if f["path"] in paths}
    source = "models/multilingual/prep/source/rl_common.py"
    sources[source] = "8d83611d480c971d640a7b7d3aa2f2219c5e8455e9cc2329fd073681bd8be23e"
    for path, expected in sources.items():
        assert hashlib.sha256(Path(path).read_bytes()).hexdigest() == expected
    assert platform.python_version() == "3.11.16"
    assert version("transformers") == "5.0.0" and version("tokenizers") == "0.22.2"
    sys.path.insert(0, str(root / "prep/source"))
    from rl_common import build_sequence
    from transformers import AutoTokenizer

    tok = AutoTokenizer.from_pretrained(root / "tokenizer", local_files_only=True)
    q = dict(t="choice", ins="Choose.", crit={"a": None, "b": None})
    empty_ids, markers = build_sequence(tok, "", q, 1024, 256)
    scenarios = [("empty-state", "", 1024, 256), ("short-state", "中", 1024, 256),
                 ("room-zero", "中国很大", len(empty_ids), 256),
                 ("room-one", "中国很大", len(empty_ids) + 1, 256),
                 ("last-marker-kept", "state", markers[-1] + 1, 256),
                 ("last-marker-out-of-bounds", "state", markers[-1], 256),
                 ("all-markers-lost", "state", markers[0], 256)]
    cases = [capture(build_sequence, tok, q, *scenario) for scenario in scenarios]
    long = dict(t="choice", ins="question " * 40,
                crit={"a": "candidate " * 80, "b": "candidate " * 80})
    for head in [0, 15, 16, 23, 24, 113, 114]:
        cases.append(capture(build_sequence, tok, long, f"head-{head}", "state", 1024, head))
    return dict(kind="official_sequence_synthetic_budgets", python=platform.python_version(),
                sources=sources, versions={key: version(key) for key in ["transformers", "tokenizers"]},
                generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), cases=cases)


def capture(build, tok, q, name, state, max_len, head_max_len):
    ids, markers = build(tok, state, q, max_len, head_max_len)
    request = dict(state=state, questions={"q": dict(type=q["t"], instructions=q["ins"], criteria=q["crit"])})
    return dict(name=name, max_len=max_len, head_max_len=head_max_len,
                request_json=json.dumps(request, ensure_ascii=False), input_ids=ids, marker_pos=markers,
                expected_error="marker_lost" if len(markers) != len(q["crit"]) else None)


if __name__ == "__main__":
    with Path(sys.argv[1]).open("x", encoding="utf-8") as output:
        json.dump(generate(), output, ensure_ascii=False, indent=2, allow_nan=False)
        output.write("\n")
