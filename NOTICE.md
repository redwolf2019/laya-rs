# 第三方来源

`src/sequence.rs` 的序列预算、拼接与批处理算法移植自
[`he-jev/laya` 的 `rl_common.py`](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_common.py)
中的 `build_sequence` / `collate_items`。
固定 revision：`c5d78730f3493e4fe16d61507ef4b78eef7318cf`；原文件 SHA-256：
`8d83611d480c971d640a7b7d3aa2f2219c5e8455e9cc2329fd073681bd8be23e`。

改动：以 Rust 实现推理路径，复用规范化请求和文本渲染；增加类型化错误、marker 完整性
校验、维度/分配检查；仅构造五个推理输入，不移植训练、随机选项顺序或左侧截断。

`src/postprocess.rs` 的温度桶、概率和答案构造移植自同一 revision 的
`rl_common.py`（`temp_bucket` / `confidence_from_probs`）及
[`rl_agent_api.py`](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_agent_api.py)
的 `system_one` 后处理段。API 文件 SHA-256：
`be3b46819c9999c3ef88e0f2ecf6d3ab1cdfed1d9a8b466fc89811e34d44031b`。
改动：使用已有 Rust 答案类型，增加 shape/有限性/概率检查；action 接收固定导出器已
softmax 的输出。使用标准库十进制定点格式化保留 Python 四位舍入语义。

上游该 revision 的 README 声明 Apache-2.0，但没有独立 LICENSE 或 NOTICE 文件。
这部分移植代码遵循 [Apache-2.0](licenses/Apache-2.0.txt)；此许可文本来自已校验 bundle
中保存的 Apache 官方标准文本，不伪称上游自带文件，不添加未知版权人。
其余原创代码继续遵循项目 [MIT License](LICENSE)。模型准备工具的其他归属见
[model-prep NOTICE](tools/model-prep/NOTICE.md)。
