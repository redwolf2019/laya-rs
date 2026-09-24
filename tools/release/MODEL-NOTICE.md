# laya-rs multilingual ONNX bundle

This is an independently exported ONNX bundle, not an upstream ONNX release.
Checkpoint: convaiinnovations/laya-multilingual at
052592a15d198d9ad47da779604259b10b47b7aa (model card: Apache-2.0).
Official reference: he-jev/laya at c5d78730f3493e4fe16d61507ef4b78eef7318cf.
Export adaptation: receptron/laya at 6478649e723122ca24bbf5fb69ed1010023c9750 (MIT).

Changes made by laya-rs: independent FP32 ONNX export with external tensor data
and dynamic batch/sequence/options dimensions; 50 LayerNorm operations expanded
to centered variance and reciprocal multiplication to match the fixed reference.
No further quantization or changes to the reference acceptance tolerances.

The original license declarations, attribution and license texts are preserved
in licenses/. Exact files and SHA-256 values are recorded in model-manifest.json.
Export source and validation: https://github.com/redwolf2019/laya-rs/tree/main/tools/model-prep
