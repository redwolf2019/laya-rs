"""Replay fixed official tensors against a candidate graph; preserve every failure.

Expected answers always come from #10, never from the candidate. This is a
development tool; the Rust service does not invoke Python.
"""

import json
import sys
from pathlib import Path

import numpy as np
import onnxruntime as ort

from fixtures import compare, postprocessor, provenance, replay, write
from prepare import PREP, ROOT, digest


def check_case(data, agent, process, session):
    inputs = {k: np.array(v, dtype=data["tensor_dtypes"][k]) for k, v in data["tensors"].items()}
    actual = session.run(None, inputs)
    reference = [np.array(data[k], dtype=np.float32) for k in ("logits", "act_probs")]
    for a, e in zip(actual, reference):
        assert a.shape == e.shape and a.dtype == e.dtype
        assert np.isfinite(a).all() and np.isfinite(e).all()
    assert ((actual[1] >= 0) & (actual[1] <= 1)).all()
    assert np.all(np.abs(actual[1].sum(-1) - 1) <= 0.00011)
    questions = json.loads(data["request_json"])["questions"]
    response = replay(process, agent, questions, inputs, *actual)
    observed = dict(inputs=inputs, logits=reference[0], act_probs=reference[1])
    result = compare(data, observed, actual, response)
    return dict(logits=actual[0].tolist(), act_probs=actual[1].tolist(), comparison=result)


def generate(graph, output):
    assert not output.exists(), "refuse to overwrite evidence"
    source = provenance()
    assert graph.parent.resolve() == ROOT.resolve(), "candidate must share verified external data"
    sys.path.insert(0, str((PREP / "source").resolve()))
    import rl_agent_api as api

    sys.set_int_max_str_digits(0)
    agent = api.RLAgent.__new__(api.RLAgent)
    config = json.loads((ROOT / "laya_config.json").read_text())
    agent.temperature = config.get("temperature", [1.0] * 3)
    agent.temperature_by_options = config.get("temperature_by_options", {})
    options = ort.SessionOptions()
    options.intra_op_num_threads, options.inter_op_num_threads = 8, 1
    session = ort.InferenceSession(str(graph), options, providers=["CPUExecutionProvider"])
    process, records = postprocessor(api), {}
    root = Path("tests/fixtures/system-one")
    manifest = json.loads((root / "manifest.json").read_text())
    for entry in manifest["files"]:
        assert digest(root / entry["path"]) == entry["sha256"], entry["path"]
        data = json.loads((root / entry["path"]).read_text())
        if data.get("kind") != "official_model":
            continue
        records[entry["path"]] = check_case(data, agent, process, session)
        print(entry["path"], json.dumps(records[entry["path"]]["comparison"]), flush=True)
    assert len(records) == 21
    passed = all(c["comparison"]["passed"] for c in records.values())
    write(output, dict(schema_version=1, passed=passed, graph_sha256=digest(graph),
                       weights_sha256=digest(ROOT / "laya.onnx.data"),
                       reference_manifest_sha256=digest(root / "manifest.json"),
                       api=source["api"], versions=source["versions"], platform=source["platform"],
                       generator_sha256=digest(Path(__file__)), cases=records))
    if not passed:
        raise SystemExit("FAIL: candidate does not match the fixed official answers")


if __name__ == "__main__":
    generate(Path(sys.argv[1]), Path(sys.argv[2]))
