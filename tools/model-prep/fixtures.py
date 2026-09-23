"""Generate #10 evidence from the unchanged official API; never overwrite a run.

Run from the repository root in the pinned #7 development container. Python is
not part of the application build/runtime. See fixtures README for the schema.
"""

import argparse
import ast
import copy
import json
import platform
import sys
from importlib.metadata import version
from pathlib import Path

from prepare import CHECKPOINT, OFFICIAL, PREP, ROOT, SOURCE_HASHES, digest, download
from verify import NAMES, session
from fixture_cases import model_cases, question, rejected_cases, request

API_SHA = "be3b46819c9999c3ef88e0f2ecf6d3ab1cdfed1d9a8b466fc89811e34d44031b"


def write(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, allow_nan=False, indent=2)
        stream.write("\n")


def provenance():
    assert platform.python_version() == "3.11.16"
    manifest = json.loads(Path("docs/model-manifest.json").read_text())
    for item in manifest["files"]:
        assert digest(ROOT / item["path"]) == item["sha256"], item["path"]
    inputs = json.loads((PREP / "inputs.json").read_text())
    for item in inputs:
        assert digest(Path(item["path"])) == item["sha256"], item["path"]
    for line in Path("tools/model-prep/requirements.lock").read_text().splitlines():
        package, expected = line.split("==")
        assert version(package).split("+")[0] == expected, package
    source = PREP / "source/rl_agent_api.py"
    url = f"https://raw.githubusercontent.com/he-jev/laya/{OFFICIAL}/rl_agent_api.py"
    download(url, source)
    assert digest(source) == API_SHA
    assert digest(PREP / "source/rl_common.py") == SOURCE_HASHES["rl_common.py"]
    return dict(official_revision=OFFICIAL, checkpoint_revision=CHECKPOINT,
                api=dict(url=url, sha256=API_SHA), inputs=inputs,
                bundle_manifest_sha256=digest(Path("docs/model-manifest.json")),
                bundle_files=manifest["files"], python=sys.version, platform=platform.platform(),
                image=manifest["image"], cgroup={p: Path("/sys/fs/cgroup", p).read_text().strip()
                                               for p in ("cpu.max", "memory.max", "memory.swap.max")},
                versions={line.split("==")[0]: version(line.split("==")[0]) for line in
                          Path("tools/model-prep/requirements.lock").read_text().splitlines()},
                tools={str(p): digest(p) for p in [Path(__file__), Path(__file__).with_name("fixture_cases.py"),
                                                  Path("tools/model-prep/requirements.lock")]})


def postprocessor(api):
    """Extract the unchanged official postprocessing AST; no second implementation."""
    tree = ast.parse(Path(api.__file__).read_text())
    cls = next(n for n in tree.body if isinstance(n, ast.ClassDef) and n.name == "RLAgent")
    fn = next(n for n in cls.body if isinstance(n, ast.FunctionDef) and n.name == "system_one")
    start = next(i for i, n in enumerate(fn.body) if isinstance(n, ast.Assign)
                 and ast.unparse(n.targets[0]) == "(answers, n_tokens)")
    fn.body = fn.body[start:]
    fn.decorator_list = []
    fn.args = ast.parse("def replay(self, ids, items, b, logits, act, questions): pass").body[0].args
    fn.name = "replay"
    namespace = vars(api).copy()
    exec(compile(ast.fix_missing_locations(ast.Module(body=[fn], type_ignores=[])),
                 "<official-postprocess>", "exec"), namespace)
    return namespace["replay"]


class RecordingTokenizer:
    def __init__(self, tokenizer):
        self.tokenizer = tokenizer
        self.calls = []

    def __getattr__(self, name):
        return getattr(self.tokenizer, name)

    def __call__(self, text, **kwargs):
        output = self.tokenizer(text, **kwargs)
        self.calls.append(dict(text=text, input_ids=output["input_ids"]))
        return output


def stages(agent, questions, logits, inputs):
    import numpy as np
    from rl_common import QTYPES, render_options, temp_bucket

    rows, offset = [], 0
    for i, (name, definition) in enumerate(questions.items()):
        q = agent._to_internal(definition)
        options = render_options(q)
        k, qt = len(options), QTYPES[q["t"]]
        bucket = temp_bucket(qt, k)
        temperature = agent.temperature_by_options.get(bucket, agent.temperature[qt])
        z = logits[i, :k] / temperature
        p = np.exp(z - z.max())
        p /= p.sum()
        calls = agent.tok.calls[offset:offset + k + 2]
        offset += k + 2
        rows.append(dict(name=name, type=q["t"], instructions_text=q["ins"], option_texts=options,
                         option_keys=(list(q["crit"]) if q["t"] == "choice" else
                                      ["false", "true"] if q["t"] == "noul" else [str(j) for j in range(k)]),
                         tokenizer_calls=calls, length=int(inputs["attention_mask"][i].sum()),
                         temperature=dict(bucket=bucket, value=temperature,
                                          source="temperature_by_options" if bucket in agent.temperature_by_options else "temperature"),
                         scaled_logits=z.tolist(), probabilities=p.tolist(), probability_dtype=str(p.dtype)))
        assert np.isfinite(z).all() and np.isfinite(p).all()
        assert ((p >= 0) & (p <= 1)).all() and abs(float(p.sum()) - 1) <= 0.00011
    return rows


def replay(reprocess, agent, questions, inputs, logits, act):
    items = [{"markers": range(int(mask.sum()))} for mask in inputs["marker_mask"]]
    return reprocess(agent, list(questions), items, inputs, logits, act, questions)


def capture(agent, raw):
    import torch
    from rl_common import serialize_state

    parsed = json.loads(raw)
    agent.tok.calls.clear()
    observed = {}

    def hook(_module, args, outputs):
        observed["inputs"] = {name: value.detach().numpy().copy() for name, value in zip(NAMES, args)}
        observed["logits"] = outputs[0].float().numpy().copy()
        observed["act_logits"] = outputs[1].float().numpy().copy()
        observed["act_probs"] = torch.softmax(outputs[1].float(), -1).numpy().copy()

    handle = agent.model.register_forward_hook(hook)
    try:
        response = agent.system_one(parsed["state"], parsed["questions"])
    finally:
        handle.remove()
    rows = stages(agent, parsed["questions"], observed["logits"], observed["inputs"])
    fixture = dict(schema_version=1, kind="official_model", request_json=raw,
                   state_text=serialize_state(parsed["state"]), rows=rows,
                   tensors={key: value.tolist() for key, value in observed["inputs"].items()},
                   tensor_dtypes={key: str(value.dtype) for key, value in observed["inputs"].items()},
                   logits=observed["logits"].tolist(), act_logits=observed["act_logits"].tolist(),
                   act_probs=observed["act_probs"].tolist(), response=response)
    return fixture, parsed, observed


def compare(fixture, observed, actual, response):
    import numpy as np

    failures = []
    checks = [("logits", observed["logits"][observed["inputs"]["marker_mask"]],
               actual[0][observed["inputs"]["marker_mask"]], 1e-4, 1e-3),
              ("act_probs", observed["act_probs"], actual[1], 1e-5, 1e-4)]
    errors = {}
    for name, expected, result, atol, rtol in checks:
        errors[name] = float(np.max(np.abs(result - expected)))
        if not np.all(np.abs(result - expected) <= atol + rtol * np.abs(expected)):
            failures.append(name)
    for i, row in enumerate(fixture["rows"]):
        expected = np.array(row["probabilities"], dtype=np.float32)
        z = actual[0][i, :len(expected)] / row["temperature"]["value"]
        p = np.exp(z - z.max())
        p /= p.sum()
        if not np.all(np.abs(p - expected) <= 1e-5 + 1e-4 * np.abs(expected)):
            failures.append(f"probabilities:{i}")
        a = copy.deepcopy(response["answers"][row["name"]])
        b = copy.deepcopy(fixture["response"]["answers"][row["name"]])
        a.pop("rl_agent")
        b.pop("rl_agent")
        if a != b:
            failures.append(f"rounded_answer:{i}")
    if response["usage"] != fixture["response"]["usage"] or response["model"] != "rl-agent":
        failures.append("envelope")
    return dict(passed=not failures, failures=failures, max_absolute_errors=errors)


def model_fixture(agent, runtime, reprocess, raw):
    import numpy as np

    fixture, parsed, observed = capture(agent, raw)
    expected = replay(reprocess, agent, parsed["questions"], observed["inputs"], observed["logits"], observed["act_probs"])
    assert expected == fixture["response"], "AST replay must equal the unchanged official API"
    actual = runtime.run(None, observed["inputs"])
    for result, reference in zip(actual, [observed["logits"], observed["act_probs"]]):
        assert result.shape == reference.shape and result.dtype == reference.dtype == np.float32
        assert np.isfinite(result).all() and np.isfinite(reference).all()
    assert ((actual[1] >= 0) & (actual[1] <= 1)).all()
    assert np.all(np.abs(actual[1].sum(-1) - 1) <= 0.00011)
    response = replay(reprocess, agent, parsed["questions"], observed["inputs"], *actual)
    fixture["onnx"] = dict(logits=actual[0].tolist(), act_probs=actual[1].tolist(), response=response)
    fixture["comparison"] = compare(fixture, observed, actual, response)
    return fixture


def artificial(agent, reprocess):
    import numpy as np
    from rl_common import temp_bucket

    rows = []
    agent = copy.copy(agent)
    agent.temperature = [0.7, 1.3, 2.0]
    agent.temperature_by_options = {f"{kind}:{bucket}": value for kind in ("choice", "score", "noul")
                                    for bucket, value in [("2", 0.5), ("3-5", 1.5), ("6-10", 2.5), ("11+", 3.5)]}
    for qt, kind in enumerate(("choice", "score", "noul")):
        for k in ((2,) if kind == "noul" else (2, 3, 5, 6, 10, 11, 32)):
            for override in (True, False):
                table = agent.temperature_by_options.copy()
                if not override:
                    agent.temperature_by_options = {}
                questions = {"q": question(kind, "synthetic", [str(i) for i in range(k)] if kind != "noul" else None)}
                logits = np.linspace(-1, 1, k, dtype=np.float32).reshape(1, k)
                inputs = dict(marker_mask=np.ones((1, k), dtype=bool), attention_mask=np.ones((1, 1), dtype=np.int64))
                act = np.array([[0.12345678, 0.87654322]], dtype=np.float32)
                response = replay(reprocess, agent, questions, inputs, logits, act)
                bucket = temp_bucket(qt, k)
                rows.append(dict(type=kind, k=k, bucket=bucket, source="override" if override else "fallback",
                                 temperature=agent.temperature_by_options.get(bucket, agent.temperature[qt]),
                                 logits=logits.tolist(), act_probs=act.tolist(), questions=questions, response=response))
                agent.temperature_by_options = table
    return dict(schema_version=1, kind="artificial_logits", provenance="explicit artificial inputs; official postprocessing AST",
                cases=rows, boundaries=numeric_boundaries(agent, reprocess),
                rounding=[dict(input=x, expected=round(x, 4)) for x in
                                      [np.nextafter(0.03125, 0).item(), 0.03125, np.nextafter(0.03125, 1).item()]])


def numeric_boundaries(agent, reprocess):
    import numpy as np

    agent.temperature = [1.0] * 3
    agent.temperature_by_options = {}
    rows = []
    for name, values in [("tie-first", [0.0, 0.0]), ("near-tie-second", [0.0, 1e-6]),
                         ("zero-probability", [-1000.0, 1000.0]), ("uniform-32", [0.0] * 32)]:
        k = len(values)
        questions = {"q": question("choice", "artificial", [str(i) for i in range(k)])}
        logits = np.array([values], dtype=np.float32)
        act = np.array([[0.12345678, 0.87654322]], dtype=np.float32)
        inputs = dict(marker_mask=np.ones((1, k), dtype=bool), attention_mask=np.ones((1, 1), dtype=np.int64))
        rows.append(dict(name=name, logits=logits.tolist(), act_probs=act.tolist(), questions=questions,
                         temperature=1.0, response=replay(reprocess, agent, questions, inputs, logits, act)))
    assert rows[0]["response"]["answers"]["q"]["choice"] == "0"
    assert rows[1]["response"]["answers"]["q"]["choice"] == "1"
    return rows


def generate(output):
    output.mkdir(parents=True, exist_ok=False)
    source = provenance()
    write(output / "provenance.json", dict(schema_version=1, kind="provenance", **source))
    sys.path.insert(0, str((PREP / "source").resolve()))
    import torch
    import rl_agent_api as api

    torch.set_num_threads(8)
    torch.manual_seed(0)
    sys.set_int_max_str_digits(0)
    agent = api.RLAgent(str(PREP / "checkpoint"), device="cpu")
    assert all(p.dtype == torch.float32 for p in agent.model.parameters())
    agent.tok = RecordingTokenizer(agent.tok)
    runtime, reprocess = session(), postprocessor(api)
    reports = []
    for name, raw in model_cases(agent.tok):
        fixture = model_fixture(agent, runtime, reprocess, raw)
        write(output / f"{name}.json", fixture)
        reports.append(dict(name=name, **fixture["comparison"]))
        print(json.dumps(reports[-1]), flush=True)
    write(output / "artificial.json", artificial(agent, reprocess))
    write(output / "rejected.json", rejected_cases())
    passed = all(r["passed"] for r in reports)
    write(output / "manifest.json", dict(schema_version=1, status="passed" if passed else "failed",
          command=f"python tools/model-prep/fixtures.py --output {output}", providers=runtime.get_providers(),
          tolerances=dict(logits=dict(atol=1e-4, rtol=1e-3), probabilities=dict(atol=1e-5, rtol=1e-4), rounded="exact"),
          files=[dict(path=p.name, sha256=digest(p)) for p in sorted(output.glob("*.json"))], comparisons=reports))
    if not passed:
        raise SystemExit("FAIL: preserved all outputs; do not replace golden or relax tolerances")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="new directory; existing directories are refused")
    generate(parser.parse_args().output)
