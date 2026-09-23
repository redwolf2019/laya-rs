# Sequence Builder 验收（#12）

日期：2026-09-23。来源：[任务 #12](https://github.com/redwolf2019/laya-rs/issues/12)、
[兼容契约 §3–4](../compatibility.md)、[#10 fixtures](../../tests/fixtures/README.md)。

`[已验证/HIGH，固定样例范围]` Rust tokenizers 0.23.2 使用 #7 manifest 固定的 tokenizer，
全部 21 个官方请求的 input_ids、attention_mask、marker_pos、marker_mask、qtype、shape
和 usage 精确一致。官方逐次 tokenizer 调用记录中的未裁切 IDs 也逐项精确一致。
参考 Python tokenizers 为 0.22.2，未替换期望结果或放宽任何容差。

实现复用 `Request::state_text`、`Question::instructions_text` 和 `Criteria::option_texts`。
加载器把已校验 tokenizer、配置中的实际特殊 token 和 1024/256 预算交给 Sequence Builder，
单份 tokenizer 常驻。输出为按行连续的 i64/bool 缓冲区；不引入 ndarray 或新依赖。
state 编码结果在批内复用，每行仍独立计入 usage。零 padding 不计数，output_tokens 固定 0。

## 对照与边界

- #10：三类型、中英文、混合 B=6/16、不同 k、k=32、长中文、长指令/选项、L=1024、
  实际 `<mask>` 的重复/相邻字面量清洗、其他特殊 token，以及 Python JSON 文本。
- [补充 oracle](../../tests/fixtures/sequence-boundaries.json)：14 个合成预算样例，实际 tokenizer；
  空状态、单字状态、room=0/1、最后 marker 恰好在截断内/外、全部 marker 丢失；
  head=0/15/16/23/24/113/114 覆盖负预算、floor 与压缩阈值。保留官方截断结果，
  不补末尾 SEP。marker 数减少时 Rust 整批返回 `MarkerLost`。
- 最大默认资源：同一请求精确 1 MiB、16 个问题、每题 32 项、64 层 JSON；
  每行与官方 options-32 样例一致。额外 body/depth 位于未知字段，仍经过请求校验。
- 无模型测试使用小词表的真实 WordLevel tokenizer，验证非固定特殊 ID、显式 marker、
  padding、重复 state 计数、批内后续题失败的原子性、非法预算及特殊 token 配置。

shape 乘法、长度求和和 usize→i64 转换均检查；最终缓冲区使用 `try_reserve_exact`。
索引由已校验的整体分配长度和批内最大行长约束。输入资源限制仍由
`Request::from_slice` 执行，不能把公开的 DTO 手工构造当作已校验请求。

## 复现

在仓库根目录执行。专项测试只需三个 manifest 固定文件：`laya_config.json`、
`tokenizer/tokenizer.json`、`tokenizer/tokenizer_config.json`；测试先验证 SHA-256。
不需要 ONNX 权重、Python 或原生 ORT。缺文件会失败，不会降级为 mock 后报通过。

```sh
rtk cargo test --locked --test sequence
rtk proxy env LAYA_TEST_MODEL=models/multilingual cargo test --locked --test sequence -- --ignored --nocapture
rtk cargo fmt --check
rtk cargo clippy --locked --all-targets -- -D warnings
rtk cargo test --locked
```

普通测试明确忽略三个真实 tokenizer 专项；显式执行后是 3 passed。
宿主 `cargo fmt --check`、`cargo clippy --locked --all-targets -- -D warnings` 通过；
全量 `cargo test --locked` 为 34 passed / 4 ignored（3 个 tokenizer 专项和既有模型 smoke）。
这四项另行显式运行：宿主 tokenizer 专项 3 passed，Linux ARM64 序列文件 6 passed，
Linux CPU smoke 1 passed。Linux 沿用 [#8 构建容器与 ORT](model-loader.md)，命令：

```sh
rtk proxy docker start laya-loader-8
rtk proxy docker exec -w /work -e LAYA_TEST_MODEL=/work/models/multilingual laya-loader-8 \
  cargo test --locked --test sequence -- --include-ignored --nocapture
rtk proxy docker exec -w /work -e LAYA_TEST_MODEL=/work/models/multilingual \
  -e LAYA_TEST_ORT=/ort/lib/libonnxruntime.so laya-loader-8 \
  cargo test --locked --bin laya-server linux_real_model_smoke -- --ignored --nocapture
```

两个 CPU Session 的复用探针均输出 logits `[1.0814669,-1.09079]`、act_probs `[1,0]`，
仍满足固定张量容差；ORT 保留 `Unknown CPU vendor` 警告，本次不推断性能影响。
这是加载器集成回归，未将 Sequence Builder 输出送入完整推理/后处理链路。

补充 oracle 来自 [sequence_oracle.py](../../tools/model-prep/sequence_oracle.py)，只调用未修改的
官方 `build_sequence`，不运行模型。它复核官方源码与 tokenizer 哈希、Python 3.11.16、
Transformers 5.0.0/tokenizers 0.22.2，并记录生成器摘要。复用 #7 容器：

```sh
rtk proxy docker start laya-model-prep-7
rtk proxy docker exec -w /work laya-model-prep-7 python tools/model-prep/sequence_oracle.py /tmp/sequence-oracle-12.json
rtk proxy docker exec laya-model-prep-7 cmp /work/tests/fixtures/sequence-boundaries.json /tmp/sequence-oracle-12.json
```

输出路径须不存在，拒绝覆盖既有证据。合成预算由测试构造器传入，生产加载器仍只接受
#7 固定预算，不放宽 bundle 校验。
`[已验证/HIGH]` 在新路径再次生成后，`cmp` 返回 0，补充 oracle 逐字节一致。

本次不实现 HTTP、后处理或完整推理入口。#10 `long-padding` 官方 Score `4.5307` 与
ONNX `4.5306` 的既有差异原样保留；本次序列完全一致不代表完整模型/答案兼容。

code-review 以任务起点 `5fea2864a202c5b12e5b0bbf6a3b7fdc3311c539` 审查本批变更：
Standards 0 项，Spec 0 项。Standards 为静态审查；Spec 独立复跑序列测试，6 passed / 0 ignored。
