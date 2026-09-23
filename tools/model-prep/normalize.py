"""Expand LayerNorm into centered FP32 operations during offline export (#13).

The CPU kernels' statistics/normalization order differ from pinned PyTorch.
Keep the centered variance and reciprocal multiplication explicit; no epsilon
adjustments, output corrections, weight changes or fixture-specific branches.
"""

import sys
from pathlib import Path

import numpy as np
import onnx
from onnx import helper, numpy_helper


def layer_norm_nodes(node, initializers):
    attrs = {a.name: helper.get_attribute_value(a) for a in node.attribute}
    assert attrs.get("axis", -1) == -1 and attrs.get("stash_type", 1) == 1
    assert len(node.output) == 1 and len(node.input) in (2, 3)
    prefix = "laya_norm_" + node.output[0] + "_"
    axes, epsilon = prefix + "axes", prefix + "epsilon"
    initializers.extend([
        numpy_helper.from_array(np.array([-1], dtype=np.int64), axes),
        numpy_helper.from_array(np.array(attrs.get("epsilon", 1e-5), dtype=np.float32), epsilon),
    ])
    nodes = []

    def add(op, inputs, name):
        output = prefix + name
        nodes.append(helper.make_node(op, inputs, [output], name=output))
        return output

    mean = add("ReduceMean", [node.input[0], axes], "mean")
    centered = add("Sub", [node.input[0], mean], "centered")
    squared = add("Mul", [centered, centered], "squared")
    variance = add("ReduceMean", [squared, axes], "variance")
    adjusted = add("Add", [variance, epsilon], "adjusted")
    root = add("Sqrt", [adjusted], "sqrt")
    reciprocal = add("Reciprocal", [root], "reciprocal")
    normalized = add("Mul", [centered, reciprocal], "normalized")
    scaled = add("Mul", [normalized, node.input[1]], "scaled")
    inputs = [scaled, node.input[2]] if len(node.input) == 3 else [scaled]
    nodes.append(helper.make_node("Add" if len(inputs) == 2 else "Identity", inputs,
                                  list(node.output), name=prefix + "output"))
    return nodes


def expand_layer_norm(model):
    nodes, count = [], 0
    for node in model.graph.node:
        if node.domain == "" and node.op_type == "LayerNormalization":
            nodes.extend(layer_norm_nodes(node, model.graph.initializer))
            count += 1
        else:
            nodes.append(node)
    assert count == 50, "pinned bundle must have exactly 50 LayerNormalization nodes"
    del model.graph.node[:]
    model.graph.node.extend(nodes)
    return count


def rewrite(source, destination):
    assert not destination.exists(), "refuse to overwrite an existing graph"
    assert source.parent.resolve() == destination.parent.resolve(), "preserve external-data location"
    model = onnx.load(source, load_external_data=False)
    count = expand_layer_norm(model)
    onnx.save(model, destination)
    onnx.checker.check_model(str(destination))
    return count


if __name__ == "__main__":
    print("expanded LayerNorm nodes:", rewrite(Path(sys.argv[1]), Path(sys.argv[2])))
