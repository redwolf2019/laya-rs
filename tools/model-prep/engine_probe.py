"""Artificial native-interface probe, not a model or compatibility oracle.

The fixed output width exercises shape validation; Noul qtype emits NaN.
No learned weights. Rust uses this only in its explicit native regression.
"""
import sys

import onnx
from onnx import TensorProto as T, helper as h

inputs = [h.make_tensor_value_info(name, dtype, shape) for name, dtype, shape in [
    ("input_ids", T.INT64, ["B", "L"]), ("attention_mask", T.INT64, ["B", "L"]),
    ("marker_pos", T.INT64, ["B", "K"]), ("marker_mask", T.BOOL, ["B", "K"]),
    ("qtype", T.INT64, ["B"]),
]]
initializers = [
    h.make_tensor("noul_type", T.INT64, [1], [2]),
    h.make_tensor("finite", T.FLOAT, [1, 2], [0.0, 0.0]),
    h.make_tensor("bad", T.FLOAT, [1, 2], [float("nan"), 0.0]),
    h.make_tensor("act", T.FLOAT, [1, 2], [0.25, 0.75]),
]
nodes = [h.make_node("Equal", ["qtype", "noul_type"], ["is_noul"]),
         h.make_node("Where", ["is_noul", "bad", "finite"], ["logits"]),
         h.make_node("Identity", ["act"], ["act_probs"])]
outputs = [h.make_tensor_value_info(name, T.FLOAT, [1, 2]) for name in ("logits", "act_probs")]
graph = h.make_graph(nodes, "artificial-engine-errors", inputs, outputs, initializers)
model = h.make_model(graph, opset_imports=[h.make_opsetid("", 18)], ir_version=10)
onnx.checker.check_model(model)
with open(sys.argv[1], "xb") as output:
    output.write(model.SerializeToString())
