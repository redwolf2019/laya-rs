"""Run the #7 bundle acceptance boundary against the pinned official model."""

import json
import platform
import sys
from collections import Counter
from importlib.metadata import version

from prepare import PREP, ROOT, digest

NAMES = ["input_ids", "attention_mask", "marker_pos", "marker_mask", "qtype"]
FILES = ["laya.onnx", "laya.onnx.data", "laya_config.json",
         "tokenizer/tokenizer.json", "tokenizer/tokenizer_config.json"]


def require_bundle(root):
    for name in FILES:
        if not (root / name).is_file() or (root / name).stat().st_size == 0:
            raise ValueError(f"Missing or empty bundle file: {name}")


def inspect_graph():
    import onnx

    graph = onnx.load(ROOT / "laya.onnx", load_external_data=False)
    onnx.checker.check_model(str(ROOT / "laya.onnx"))
    external = set()
    for tensor in graph.graph.initializer:
        assert tensor.data_type not in (onnx.TensorProto.FLOAT16, onnx.TensorProto.BFLOAT16,
                                        onnx.TensorProto.DOUBLE), tensor.name
        external.update(e.value for e in tensor.external_data if e.key == "location")
    assert external == {"laya.onnx.data"}, external
    return dict(ir_version=graph.ir_version,
                opsets={o.domain: o.version for o in graph.opset_import},
                initializer_types=dict(Counter(onnx.TensorProto.DataType.Name(t.data_type)
                                               for t in graph.graph.initializer)),
                external_data=sorted(external))


def session():
    import onnxruntime as ort

    options = ort.SessionOptions()
    options.intra_op_num_threads = 8
    options.inter_op_num_threads = 1
    result = ort.InferenceSession(str(ROOT / "laya.onnx"), options,
                                  providers=["CPUExecutionProvider"])
    assert result.get_providers() == ["CPUExecutionProvider"]
    inputs, outputs = result.get_inputs(), result.get_outputs()
    assert [x.name for x in inputs] == NAMES
    assert [x.type for x in inputs] == ["tensor(int64)", "tensor(int64)",
                                      "tensor(int64)", "tensor(bool)", "tensor(int64)"]
    assert [x.name for x in outputs] == ["logits", "act_probs"]
    assert [x.type for x in outputs] == ["tensor(float)"] * 2
    for item, rank in zip(inputs + outputs, [2, 2, 2, 2, 1, 2, 2]):
        assert len(item.shape) == rank
        dynamic = item.shape[:-1] if item.name == "act_probs" else item.shape
        assert all(isinstance(d, str) and d for d in dynamic), item.name
    assert outputs[1].shape[1] == 2
    return result


def cases(tok, config):
    from rl_common import QTYPES, build_sequence, collate_items

    examples = [
        ("zh-choice", "客户重复扣款，希望立即退款。", "choice", "由哪个部门处理？",
         {"账单": "退款、支付", "技术": "软件故障", "销售": "购买咨询"}),
        ("zh-score", "客户重复扣款，希望立即退款。", "score", "处理优先级？", ["低", "中", "高", "紧急"]),
        ("zh-noul", "客户重复扣款，希望立即退款。", "noul", "客户需要退款。", {}),
        ("en-choice", "The app crashes when I sign in.", "choice", "Choose a team.",
         {"billing": None, "technical": "Software faults", "sales": None}),
        ("en-score", "The app crashes when I sign in.", "score", "Rate urgency.", ["low", "medium", "high"]),
        ("en-noul", "The app crashes when I sign in.", "noul", "The customer reports a software fault.", {}),
        ("empty", "", "choice", "", {"": None, " ": None}),
        ("long", "客户需要帮助。 " * 1500, "choice", "请选择。", {"是": None, "否": None}),
    ]
    examples += [(f"options-{k}", "test", "choice", "Select.",
                  {str(i): "candidate " * 60 for i in range(k)}) for k in (5, 6, 10, 11, 32)]
    items = []
    for name, state, kind, instructions, criteria in examples:
        ids, markers = build_sequence(tok, state, dict(t=kind, ins=instructions, crit=criteria),
                                      config["max_len"], config["head_max_len"])
        assert 2 <= len(markers) <= 32 and len(ids) <= 1024
        items.append(dict(ids=ids, markers=markers, qtype=QTYPES[kind], target=[0] * len(markers),
                          label=0, episode=False, ep_step=0, name=name))
    groups = [(x[0], [item]) for x, item in zip(examples, items)]
    groups += [("mixed-2", items[:2]), ("mixed-6", items[:6]),
               ("mixed-16", (items[:6] * 3)[:16]), ("long-padding", [items[7], items[6]]),
               ("long-options-padding", [items[7], items[-1]])]
    for name, group in groups:
        batch = collate_items([group], tok.pad_token_id)
        yield name, {key: batch[key] for key in NAMES}


def compare(name, batch, model, runtime):
    import numpy as np
    import torch

    with torch.no_grad():
        logits, act_logits = model(**batch)
        reference = [logits.numpy(), torch.softmax(act_logits.float(), -1).numpy()]
    inputs = {k: v.numpy() for k, v in batch.items()}
    actual = runtime.run(None, inputs)
    b, k = inputs["marker_pos"].shape
    assert actual[0].shape == (b, k) and actual[1].shape == (b, 2)
    assert all(v.dtype == np.float32 and np.isfinite(v).all() for v in actual + reference)
    valid = inputs["marker_mask"]
    np.testing.assert_allclose(actual[0][valid], reference[0][valid], atol=1e-4, rtol=1e-3)
    np.testing.assert_allclose(actual[1], reference[1], atol=1e-5, rtol=1e-4)
    assert ((actual[1] >= 0) & (actual[1] <= 1)).all()
    np.testing.assert_allclose(actual[1].sum(-1), 1, atol=1e-5, rtol=1e-4)
    for observed, expected, mask in zip(actual[0], reference[0], valid):
        p = np.exp(observed[mask] - observed[mask].max())
        p /= p.sum()
        q = np.exp(expected[mask] - expected[mask].max())
        q /= q.sum()
        np.testing.assert_allclose(p, q, atol=1e-5, rtol=1e-4)
    record = dict(name=name, shapes={k: list(v.shape) for k, v in inputs.items()},
                  lengths=inputs["attention_mask"].sum(-1).tolist(),
                  candidates=valid.sum(-1).tolist(), qtype=inputs["qtype"].tolist(),
                  max_logit_error=float(np.abs(actual[0][valid] - reference[0][valid]).max()),
                  max_act_error=float(np.abs(actual[1] - reference[1]).max()),
                  reference_logits=reference[0].tolist(), actual_logits=actual[0].tolist(),
                  reference_act_probs=reference[1].tolist(), actual_act_probs=actual[1].tolist())
    print(json.dumps(record, ensure_ascii=False), flush=True)
    return record


def verify():
    (PREP / "validation.json").unlink(missing_ok=True)
    require_bundle(ROOT)
    import torch
    from transformers import AutoTokenizer
    from export import load_reference

    torch.set_num_threads(8)
    graph = inspect_graph()
    runtime = session()
    model, config = load_reference()
    assert config["max_len"] == 1024 and config["head_max_len"] == 256
    bundle_config = json.loads((ROOT / "laya_config.json").read_text())
    assert bundle_config == {k: config[k] for k in
                             ("max_len", "head_max_len", "temperature", "temperature_by_options")}
    for name in ("tokenizer.json", "tokenizer_config.json"):
        assert digest(ROOT / "tokenizer" / name) == digest(PREP / "checkpoint/tokenizer" / name)
    tok = AutoTokenizer.from_pretrained(PREP / "checkpoint/tokenizer", local_files_only=True)
    records = [compare(name, batch, model, runtime) for name, batch in cases(tok, config)]
    minimum = dict(input_ids=torch.tensor([[tok.cls_token_id, tok.sep_token_id]]),
                   attention_mask=torch.ones(1, 2, dtype=torch.int64),
                   marker_pos=torch.tensor([[0, 1]]), marker_mask=torch.ones(1, 2, dtype=torch.bool),
                   qtype=torch.tensor([0]))
    records.append(compare("tensor-L2", minimum, model, runtime))
    metadata = [dict(name=x.name, dtype=x.type, shape=x.shape)
                for x in runtime.get_inputs() + runtime.get_outputs()]
    report = dict(status="passed", platform=platform.platform(), python=sys.version,
                  versions={p: version(p) for p in ["torch", "transformers", "onnx", "onnxscript",
                                                     "onnxruntime", "numpy", "safetensors", "tokenizers"]},
                  graph=graph, session=metadata, providers=runtime.get_providers(),
                  special_tokens={k: getattr(tok, k) for k in ["cls_token_id", "sep_token_id",
                                                              "mask_token_id", "pad_token_id"]},
                  files={n: digest(ROOT / n) for n in FILES}, cases=records)
    (PREP / "validation.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print("PASS: real CPU ONNX / official checkpoint parity", flush=True)


if __name__ == "__main__":
    verify()
