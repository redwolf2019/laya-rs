# 后处理验收（#13）

日期：2026-09-23。来源：[任务 #13](https://github.com/redwolf2019/laya-rs/issues/13)、
[兼容契约 §5/9](../compatibility.md)、[#10 固定 fixtures](../../tests/fixtures/README.md)。

`[已验证/HIGH，固定输出重放范围]` 全部 21 个官方模型请求的原始 logits 经 Rust 后处理，
完整响应与固定官方 API 精确相等；未舍入概率满足 `1e-5 + 1e-4*abs(reference)`。
对应 21 组 ONNX 输出也与同组 logits 的官方后处理 AST 重放结果精确相等。
这些测试不运行模型，不能作为实际 Rust 推理或完整模型兼容证据。

`[已验证/HIGH，固定样例与 Linux ARM64 CPU 范围]` 后续修复已通过跨后端门槛。
21 个固定请求经 Rust Sequence Builder、真实 ORT 和 Rust 后处理后，完整答案满足原契约：
四位字段精确相等，主 logits、未舍入概率和 action 满足原容差。
[旧 bundle 的 Rust 回归](postprocess/old-bundle-rust.log)重现了 `4.5306 / 4.5307`；
[新 bundle 的同一回归](postprocess/normalized-rust.log)全部通过，未改断言或官方期望。
旧[离线失败日志](postprocess/cross-backend.log)及 #10 原始 fixtures 保留为历史证据。

## LayerNorm 修复与诊断

[诊断数据](postprocess/layer-norm-diagnosis.json)显示官方未舍入 Score 为
`4.530653640627861`，旧 ONNX 为 `4.530648112297058`，分处四位舍入边界 `4.53065` 两侧。
f64 后处理、关闭 ORT 图优化、基础优化、单线程均未解决。
重新执行固定 PyTorch 模型，logits 与 #10 原始结果逐位相等。

embedding 查表结果逐位相同，首个 LayerNorm 的最大差异为 `5.7220458984375e-6`。
把官方首个 LayerNorm 输出注入原始 ONNX，其余计算不变，Score 回到
`4.530650153756142`，舍入为 `4.5307`。这是该归一化误差会导致本例跨界的干预证据，
不意味着其他算子逐位相同。导出中间结果会影响 ORT 融合；诊断图最终 logits 比原图
最多变化 `9.5367431640625e-7`，因此上述注入实验单独使用原图，不把插桩图冒充生产图。

固定 [PyTorch LayerNorm](https://github.com/pytorch/pytorch/blob/v2.10.0/aten/src/ATen/native/cpu/layer_norm_kernel.cpp)
使用行统计量和标准差倒数相乘；[ORT 实现](https://github.com/microsoft/onnxruntime/blob/v1.28.0/onnxruntime/core/providers/cpu/nn/layer_norm_impl.cc)
的统计/归一化运算路径不同。同一 embedding 输入下，显式 FP32 中心化方差与倒数乘法
将首层输出逐位相等比例从约 18.8% 提升到约 86.0%；使用官方 mean/rstd 则逐位相等。
这是运算路径对照，未声称新算法完整复制 PyTorch 的 Welford 归约或所有中间位模式。

[`normalize.py`](../../tools/model-prep/normalize.py) 在离线导出时统一展开全部 50 处
LayerNormalization：ReduceMean → Sub → 平方 → ReduceMean → 加原 epsilon → Sqrt →
Reciprocal → Mul → 原 scale/bias。只用标准 ONNX FP32 算子，保留原 epsilon、权重、
输入输出及温度。该图在固定 ORT 默认优化下仍保留展开形式，没有再次融合为 LayerNorm。
不按 fixture、问题或 Score 设分支，也没有添加数值偏移。

新图 `laya.onnx` 为 2,649,363 字节，SHA-256 为
`5a028e8dac51de3c430514c47e047fc89911edaaa72762c3e880a3591cc0184c`；
外部权重摘要仍为 `6ef993ee707fe1d6a75529f4a1f8347ff89966ff955d332ea3ebe4e5171a9ac1`。
原图信息保存在 [manifest](../model-manifest.json) 的 `normalization.previous_graph`；
旧导出日志和原始张量验收仍列为 `base_export_evidence` / `base_export_validation`。
从同一旧图两次执行展开后，新图逐字节一致。没有重新训练或重新量化权重。

[新输出](../../tests/fixtures/normalized-outputs.json)仅保存新图实际 logits/act_probs，
官方期望仍来自未修改的 #10；记录绑定图、权重、参考 manifest、生成器与固定 API 摘要。
[`postprocess_oracle.py`](../../tools/model-prep/postprocess_oracle.py)使用同一组固定张量实跑
新 ONNX，再调用官方后处理 AST 与原期望比较；21 项通过，见[日志](postprocess/normalized-oracle.log)。
随后 Rust 真实回归独立构造输入、运行模型并构造完整答案，交替使用两个 Session。

## 实现与边界

[`Calibration`](../../src/postprocess.rs) 由加载器从原有配置字段反序列化，启动时校验全部
温度，包括未使用的桶。缺失字段沿用 `[1,1,1]` / `{}`；null、错形、非正和非有限值失败。
LayerNorm 修复更新了图和 manifest；配置与权重保持一致。新增依赖为零。

`response` 接收规范化 Request、按行连续的 float32 logits/act_probs 及实际 shape，
input_tokens 由调用方传入 Sequence Builder 的 usage。B 必须等于问题数，K 必须等于
批内最大真实选项数；action 为 `[B,2]`。检查缓冲区长度、维度转换与乘法、全部槽有限性；
padding 槽不参与概率。dtype 由 `&[f32]` 限定，未来 ORT 调用方须用 float32 提取接口。

主运算使用 f32，Score 加权和使用 f64；confidence 保留 NumPy 2.3.3 的 f32 标量运算。
每行先除选定温度再减最大值，拒绝产生非有限中间值的运算。
温度虽为有限正 f64，若无法作为有限正 f32 使用，返回数值错误，不改为默认值。
未舍入概率及 action 的范围、和按固定容差检查；各字段舍入后不二次归一化。
Choice 精确并列取首项，近并列按原始概率；Score 是期望等级；Noul 是第 1 项概率。
action 原样返回第 0 项，不温度缩放、不再次 softmax、不四位舍入。

四位舍入使用标准库定点格式化再解析，避免乘以 10000 的额外浮点舍入。
测试使用 #10 的 `0.03125` 中点及相邻 binary64 值；补充正负零、十进制中点样例来自
已有 Linux 开发容器的 CPython 3.11.16 `round(x,4)` 探针。
真实与人工输入在测试中明确区分，人工样例不证明模型校准质量。

人工覆盖三类型 k=2/3/5/6/10/11/32 桶覆盖/缺桶回退、巨大/近似 logits、精确/近并列、
零概率、均匀 32 项、四位舍入和不为 1、非饱和 action、坏 shape/索引、部分/全坏 logits、
坏 action、缩放/减法溢出和后续行失败的原子性。生产错误仅为静态枚举与文本。
测试差异定位仅打印 fixture 名、字段序号和数值差异，不打印业务文本。

## 复现

仓库根目录执行。普通后处理测试无需模型，跨后端静态输出门槛已取消 ignored。
真实模型测试需要固定文件与 ORT，普通 CI 明确跳过，手动运行缺少文件即失败。

```sh
rtk cargo test --locked --test postprocess
rtk cargo fmt --check
rtk cargo clippy --locked --all-targets -- -D warnings
rtk cargo test --locked
rtk proxy docker exec -w /work -e LAYA_TEST_MODEL=/work/models/multilingual \
  -e LAYA_TEST_ORT=/ort/lib/libonnxruntime.so laya-loader-8 \
  cargo test --locked --bin laya-server linux_system_one_model_parity -- --ignored --nocapture
rtk proxy docker exec -w /work laya-model-prep-7 python tools/model-prep/postprocess_oracle.py \
  models/multilingual/laya.onnx /tmp/new-postprocess-evidence.json
```

输出证据路径须不存在。已有旧图时，先确认其摘要为
`a0a46af6144f0e7ff66612461be9064f528b1734bc87911463d3c7fc306ce7ef`，
在相同目录生成新文件；原图保留用于回归。脚本拒绝覆盖或不同目录的 external data 路径。

```sh
rtk proxy docker exec -w /work laya-model-prep-7 python tools/model-prep/normalize.py \
  models/multilingual/laya-before-13.onnx models/multilingual/laya-rebuilt.onnx
rtk proxy cmp models/multilingual/laya.onnx models/multilingual/laya-rebuilt.onnx
```

完整重建走 [model-prep](../../tools/model-prep/README.md)，`export.py` 已接入相同展开函数。
再次生成若图哈希不同，必须重新验收并记录，不能只修改期望摘要。
本次通过范围为固定 Linux ARM64 CPU 样例；HTTP、amd64、全输入空间逐位一致及性能未验收。

`[已验证/HIGH]` 宿主 fmt、clippy 通过；完整 `cargo test --locked` 为 45 passed / 5 ignored。
后处理 10 项均执行通过；新增 Linux 真实回归单独运行通过 21 个请求。
其他四个 ignored 为原有 tokenizer 专项和模型 smoke，未独立重跑；加载新模型时仍执行
每槽 startup smoke。新输出再次生成后 `cmp` 返回 0，21 个样例的输出记录逐字节一致。

code-review 基点 `141bba9483029ce8222f0e9c138c9702e96a55a8`：Standards 0 项，
Spec 0 项必须修复发现；Spec 独立执行普通后处理 10 passed。
Standards 复核了 manifest 工具和证据摘要；两轴均为固定实现/样例范围的审查，
不外推其他硬件或全输入空间。
