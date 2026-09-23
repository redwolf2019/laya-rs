"""FP32 dynamic export, adapted from receptron/laya's pinned MIT exporter.

See NOTICE.md for attribution. The official DecisionModel is imported unchanged.
"""

import json
import shutil
import sys

from prepare import PREP, ROOT, SOURCE_HASHES, digest
from normalize import rewrite


def load_reference():
    import torch
    from safetensors.torch import load_file

    assert digest(PREP / "source/rl_common.py") == SOURCE_HASHES["rl_common.py"]
    assert digest(PREP / "checkpoint/model.safetensors") == "9d628fd971b700382ac6f65920a86f149777b2e748e0c955fb3b19695aa8f204"
    sys.path.insert(0, str((PREP / "source").resolve()))
    from rl_common import build_model

    config = json.loads((PREP / "checkpoint/rl_agent_config.json").read_text())
    model = build_model(config, encoder_dir=str(PREP / "checkpoint/encoder"))
    model.load_state_dict(load_file(PREP / "checkpoint/model.safetensors"), strict=True)
    model.float().eval()
    model.encoder.config.reference_compile = False
    assert all(p.dtype == torch.float32 for p in model.parameters())
    return model, config


def export():
    import torch

    (PREP / "validation.json").unlink(missing_ok=True)
    torch.set_num_threads(8)
    torch.manual_seed(0)
    model, config = load_reference()

    class Wrapper(torch.nn.Module):
        def __init__(self, model):
            super().__init__()
            self.model = model

        def forward(self, input_ids, attention_mask, marker_pos, marker_mask, qtype):
            logits, act = self.model(input_ids, attention_mask, marker_pos, marker_mask, qtype)
            return logits, torch.softmax(act.float(), -1)

    example = (torch.randint(5, 1000, (2, 40)), torch.ones(2, 40, dtype=torch.int64),
               torch.tensor([[3, 9, 15, 21], [3, 9, 0, 0]]),
               torch.tensor([[True, True, True, True], [True, True, False, False]]),
               torch.tensor([0, 2]))
    example[1][1, 30:] = 0
    batch = torch.export.Dim("batch", min=1, max=16)
    seq = torch.export.Dim("seq", min=2, max=1024)
    options = torch.export.Dim("options", min=2, max=32)
    program = torch.onnx.export(
        Wrapper(model).eval(), example, opset_version=18, dynamo=True, optimize=True,
        input_names=["input_ids", "attention_mask", "marker_pos", "marker_mask", "qtype"],
        output_names=["logits", "act_probs"],
        dynamic_shapes=({0: batch, 1: seq}, {0: batch, 1: seq},
                        {0: batch, 1: options}, {0: batch, 1: options}, {0: batch}),
    )
    program.save(ROOT / "laya.onnx", external_data=True)
    normalized = ROOT / "laya.normalized.onnx"
    rewrite(ROOT / "laya.onnx", normalized)
    normalized.replace(ROOT / "laya.onnx")
    shutil.copytree(PREP / "checkpoint/tokenizer", ROOT / "tokenizer", dirs_exist_ok=True)
    keys = ("max_len", "head_max_len", "temperature", "temperature_by_options")
    (ROOT / "laya_config.json").write_text(json.dumps({k: config[k] for k in keys}, indent=2) + "\n")
    print("exported", ROOT, flush=True)


if __name__ == "__main__":
    export()
