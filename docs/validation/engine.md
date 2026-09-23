# multilingual engine 验收（#14）

日期：2026-09-23。来源：[任务 #14](https://github.com/redwolf2019/laya-rs/issues/14)、
[兼容契约](../compatibility.md)、[#10 固定 fixtures](../../tests/fixtures/README.md)。

`[已验证/HIGH，固定样例与 Linux ARM64 CPU 范围]` 21 个官方请求全部经过生产
[`engine::system_one`](../../src/engine.rs)，完整四位字段精确相等；主 logits、未舍入
概率与 action 满足原始容差。五个序列缓冲区、token/marker shape、usage 精确相同。
同一个加载结果中的两个 Session 交替使用，模型只在测试开始加载一次。
原始输出见 [parity.log](engine/parity.log)。本票没有改变模型、权重、校准或官方期望。

## 入口与错误

入口借用规范化 Request、Sequence Builder、一个独占的 `&mut Session` 和 Calibration，
构造五个输入张量、执行一次 forward、提取 float32 输出并返回完整 `Response`。
不克隆模型、不重新加载、不重试；不引入队列、路由、运行配置或新依赖。
加载器仍在监听前校验固定 bundle、tokenizer、原生库和所有执行槽。

```rust
let response = laya_server::engine::system_one(
    &request,
    &model.sequence,
    &mut model.sessions[slot],
    &model.config.calibration,
)?;
```

调用方先用 `Request::from_slice` 执行资源限制与规范化，并持有执行槽直到同步调用结束。
未来 HTTP 层须把 CPU 工作放到阻塞执行边界；结束 HTTP 等待不能提前归还仍在运行的槽。
当前 CLI 仍只初始化资源并非零退出，没有开启 HTTP 监听。

`engine::Error` 区分 Sequence、Runtime、Output 和缺失输出，`source()` 保留原始错误链。
状态与 envelope 复用请求错误；marker 丢失为 400，其余推理失败为 500 `inference_failed`。
Display/envelope 仅包含静态文本，原始 ORT/tokenizer 错误只能用于内部诊断。
请求和推理代码没有用 unwrap/expect/panic 处理可失败操作。

## 验证范围

- 中文/英文三类型、B=1/6/16、混合 K/L、K=2/3/5/6/10/11/32、长状态、L=1024、
  Unicode/任意精度 JSON、截断、token/marker padding，均使用原始请求字符串。
- 序列五个缓冲区逐值比较；另用同一 Session 做原始张量审计，检查 logits/act_probs 的
  shape、dtype 和原容差，再让完整请求经过生产 engine。两条路径使用相同固定模型；
  审计不替代生产入口的完整答案比较。
- 空问题的人工错误请求被拒绝；441 字节的人工 ONNX 接口图触发坏 shape 和 NaN，
  每次失败后同一接口图 Session 仍能返回正常结果。这部分不是模型兼容证据。
- 克隆测试 tokenizer 并添加一个超出固定模型词表的测试 token，在真实模型触发 ORT Gather
  错误；原始模型、tokenizer 文件不变。同一真实 Session 随后的正常请求与故障前完整结果相同。
  错误链包含原始 `ort::Error`，对外 envelope 为静态 `inference_failed`。
- [无模型普通测试](../../tests/engine.rs)检查错误分类、来源和脱敏；缺模型专项
  [实际退出 101](engine/missing-model.log)，没有降级为模拟成功。

## 实际环境与摘要

[environment.log](engine/environment.log)记录 rustc/cargo、架构、CPU/内存配额、实际文件
SHA-256 和 `cargo tree --locked`；[container.log](engine/container.log)记录镜像身份。
Docker Desktop Linux aarch64，8 vCPU、12 GiB、swap=0；Rust 1.98.1、ort 2.0.0-rc.13、
ORT CPU 1.28.0。依赖版本及包 checksum 由 [Cargo.lock](../../Cargo.lock) 固定。

| 文件 | 实际 SHA-256 |
| --- | --- |
| laya.onnx | `5a028e8dac51de3c430514c47e047fc89911edaaa72762c3e880a3591cc0184c` |
| laya.onnx.data | `6ef993ee707fe1d6a75529f4a1f8347ff89966ff955d332ea3ebe4e5171a9ac1` |
| libonnxruntime.so.1.28.0 | `f1ec1a08eb99bd6e5401340f0a2b101381bf4694415480291dc13bcaa30f9ec7` |
| Cargo.lock | `f62585a7434a0dce5c64fec5d885b94006d97a409102052a89b66d801b9769bc` |

## 已有环境复现

在仓库根目录执行，容器启动后运行。LAYA_TEST_* 仅用于开发专项，应用配置仍走 CLI。

```sh
rtk proxy docker start laya-loader-8
rtk proxy docker exec -w /work -e LAYA_TEST_MODEL=/work/models/multilingual \
  -e LAYA_TEST_ORT=/ort/lib/libonnxruntime.so laya-loader-8 \
  cargo test --locked --bin laya-server linux_system_one_model_parity -- --ignored --nocapture
rtk cargo fmt --check
rtk cargo clippy --locked --all-targets -- -D warnings
rtk cargo test --locked
```

## 从零准备并运行

使用独立准备 checkout，保留现有验收记录。按 [model-prep 的固定输入与容器命令](../../tools/model-prep/README.md)
创建 `laya-model-prep-7` 并安装 requirements.lock，然后依次执行以下命令；任一步非零即停止。
这些 Python 命令只在独立准备阶段运行，Rust 构建、启动、推理不调用 Python。

```sh
rtk proxy docker exec -w /work laya-model-prep-7 python tools/model-prep/prepare.py
rtk proxy docker exec -w /work laya-model-prep-7 python tools/model-prep/export.py
rtk proxy docker exec -w /work laya-model-prep-7 python tools/model-prep/verify.py
```

按 model-prep 的日志采集命令保存 prepare/export/verify/environment 日志后执行 `record.py`，
记录本次实际 bundle 摘要。导出器的源码位置等元数据可能使新图字节不同；不能仅改摘要绕过
数值验收。继续用冻结的官方数据运行完整后处理门槛，新证据路径必须不存在：

```sh
rtk proxy docker exec -w /work laya-model-prep-7 python tools/model-prep/record.py
rtk proxy docker exec -w /work laya-model-prep-7 python tools/model-prep/postprocess_oracle.py \
  models/multilingual/laya.onnx models/multilingual/prep/engine-outputs.json
```

若本次图摘要不同，仅在上述生成器退出 0 后保存新 **actual 输出**，与新 manifest 一起保存；
原始官方 `system-one/` 期望不得修改。再执行下面的真实 Rust 对照。
已有固定 bundle 则无需重新生成这些记录。

```sh
rtk proxy cp models/multilingual/prep/engine-outputs.json tests/fixtures/normalized-outputs.json
```

下载固定原生库并校验包摘要（见 [#5 来源记录](../validation-environment.md)）：

```sh
rtk proxy mkdir -p models/ort
rtk proxy curl -fL --retry 2 \
  https://github.com/microsoft/onnxruntime/releases/download/v1.28.0/onnxruntime-linux-aarch64-1.28.0.tgz \
  -o models/ort/ort.tgz
rtk proxy sh -c 'echo "e15ff8b5d85afe6c144d97c6fd432254bf76a219daaf17658087d6ecb3e8f0bb  models/ort/ort.tgz" | shasum -a 256 -c -'
rtk proxy tar -xzf models/ort/ort.tgz --strip-components=1 -C models/ort
rtk proxy docker run --rm --platform linux/arm64 --cpus 8 --memory 12g --memory-swap 12g \
  --mount "type=bind,source=$PWD,target=/work,readonly" \
  --mount "type=bind,source=$PWD/models/ort,target=/ort,readonly" \
  --mount type=volume,source=laya-target-linux,target=/target \
  --mount type=volume,source=laya-cargo-registry,target=/usr/local/cargo/registry \
  -w /work -e CARGO_TARGET_DIR=/target -e LAYA_TEST_MODEL=/work/models/multilingual \
  -e LAYA_TEST_ORT=/ort/lib/libonnxruntime.so \
  rust@sha256:ff521445a372125ed4f76e1453a1f8098f2d05332d1601d30db1c1f62757e730 \
  sh -ec 'apt-get update; apt-get install -y --no-install-recommends gcc g++ libc6-dev pkg-config; cargo test --locked --bin laya-server linux_system_one_model_parity -- --ignored --nocapture'
```

本次实际复用了已有的固定准备/运行环境，没有重新下载权重或从零重建模型。
441 字节人工接口图可由 `engine_probe.py /tmp/engine-probe.onnx` 在准备容器中重建；
它不包含训练权重，不能替代真实专项。

这些结果不代表 HTTP、Jev 完全兼容、并发吞吐、amd64 或全输入空间验收。

宿主 `cargo fmt --check`、`cargo clippy --locked --all-targets -- -D warnings` 通过；
完整 `cargo test --locked` 为 46 passed / 5 ignored。Linux 真实 engine 专项单独执行通过，
另外四个既有 tokenizer/model 专项未独立重跑；模型加载仍执行每槽启动探针。

code-review 基点为 `444fc1d93cdf56153c9ec1c4b4d2aa1ae30f7149`。
Standards 的一处 fixtures 格式说明已修正，复核后 0 项；Spec 0 项。
Spec 独立运行 engine/postprocess 短测试，11 passed；两轴未独立重跑大模型。
