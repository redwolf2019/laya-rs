# laya-rs

A pure Rust inference runtime and HTTP server for Laya System-1 models.

面向 Linux CPU 的 Laya 推理服务，计划提供中文、英文及多语言的 Choice、Score、
Noul 判断能力，通过 HTTP 为多个客户端共享模型。

**当前状态：Rust 已加载并校验固定 bundle、Tokenizer 和 CPU Session，启动时执行真实张量探针。**
Sequence Builder、答案后处理和统一 engine 已通过固定样例对照；四个 HTTP 路由已接入共享调度器。
全部资源加载成功后才监听端口；已提供 Linux ARM64 CPU 多阶段 Docker 镜像构建。
HTTP 验收见 [#16 记录](docs/validation/http.md)，镜像验收见 [#18 记录](docs/validation/docker.md)。

#9 已提供[模型无关的 System One 类型与校验](src/system_one.rs)：请求规范化、
完整响应 DTO、静态类型化错误，以及数字词法/嵌套顺序保留。
`Request::from_slice(body, &limits)` 接收完整 body；`state`/`instructions` 保留 `RawValue`，
`Request::state_text()` / `Question::instructions_text()` 将其渲染为官方模型输入文字：
字符串原样使用，其余按 Python 的 Unicode/ASCII 两种 JSON 模式渲染，保持各层对象顺序。
整数保留精度；浮点按 binary64 舍入并使用 Python 的记数法，包含负零和指数溢出/下溢。
可运行 `rtk cargo test --locked --test system_one` 验证该边界，不需要模型。
文本字节对照运行 `rtk cargo test --locked --test fixtures --test json_text`，
来源与复现见 [fixtures 说明](tests/fixtures/README.md)。这些检查不需要 Python 或模型。

#12 的 [`SequenceBuilder::build`](src/sequence.rs) 将规范化请求转换为五个按行排列的
CPU 输入缓冲区及 shape/usage，复用加载器的 tokenizer 和真实任务预算。
全部 21 个官方样例及补充预算边界的 token、marker、mask、usage 已精确对照；
命令与范围见 [序列验收记录](docs/validation/sequence.md)。普通 CI 不下载 tokenizer。

#13 的 [`Calibration::response`](src/postprocess.rs) 校验 float32 输出并构造完整答案，
复用类型/桶温度、稳定 softmax、官方四位舍入和未再次处理的 action 概率。
`rtk cargo test --locked --test postprocess` 不依赖模型；对照与失败记录见
[后处理验收](docs/validation/postprocess.md)。离线导出中的 LayerNorm 展开已修复 long-padding
Score 差异；21 个固定请求经 Rust 真实推理通过完整答案门槛（Linux ARM64 CPU）。

#14 的 [`engine::system_one`](src/engine.rs) 是同步生产入口：借用已加载的 Sequence Builder、
一个独占 CPU Session 和校准配置，从规范化请求返回完整 `Response`。连续请求复用模型，
错误保留内部来源，对外使用静态类型化错误。21 个固定样例及失败恢复已通过 Linux CPU 对照，
从零准备、专项命令和实际哈希见 [engine 验收](docs/validation/engine.md)。

#15 的 [`scheduler`](src/scheduler.rs) 提供 FIFO 有界排队与独立 Session 执行槽，
在 Tokio 阻塞任务中运行 engine；调用方取消或超时后，真实工作继续占槽。
`Client` 提交请求并读取状态，`Scheduler::run` 持续回收任务，`close` 停止准入后排空。
闭锁时序测试及 Linux N=1/2 的真实执行重叠与 RSS 见[调度验收](docs/validation/scheduler.md)。

#16 的 [`api`](src/api.rs) 提供 `POST /v1/system-one`、`GET /healthz`、`GET /readyz`、
`GET /metrics`。请求按就绪、媒体类型、有界 body、JSON/字段限制、调度准入的顺序校验；
HTTP 错误复用静态 envelope，真实工作在调用方断开或超时后仍由调度器回收。
#17 已接入六个 OpenMetrics 指标及 Tracing JSON 日志：请求终态只计一次，推理耗时只覆盖
实际 Session run；超时/断连不提前降低 inflight。SIGTERM/SIGINT 停止准入并清空等待项，
在途工作最多等待 `--shutdown-grace`；到期记录剩余任务并以退出码 1 终止整个进程。
状态转换、脱敏和 Linux 进程信号验证见[观测与退出验收](docs/validation/observability.md)。

## 文档

- [服务方案与验收条件](docs/laya-server-plan.md)
- [multilingual bundle 准备与真实 CPU 对照](tools/model-prep/README.md)
- [MVP 开发路线图与可执行任务](https://github.com/redwolf2019/laya-rs/issues/1)
- [领域术语](CONTEXT.md)
- [Agent 工程约定](AGENTS.md)
- [Issue tracker](docs/agents/issue-tracker.md)
- [Triage 标签](docs/agents/triage-labels.md)
- [领域文档规则](docs/agents/domain.md)

`CLAUDE.md` 使用相对软链接指向 `AGENTS.md`，两类客户端共用同一份约定。

## 目标

- Rust 应用代码，Axum HTTP API，HuggingFace Tokenizers 和 ONNX Runtime CPU 推理。
- 默认模型 `laya-multilingual`；运行服务不需要 Python、Node.js、PyTorch 或 GPU。
- Choice / Score / Noul，以及经参考实现对照验证的 System One 接口兼容性。
- CPU 并发限制、Health / Readiness 和 Prometheus Metrics。

ONNX Runtime 属于原生运行库依赖；这里的“纯 Rust”指服务应用实现。
实施顺序见方案文档。

## 构建与 CLI

安装 Rustup 后，在仓库根目录执行。`rust-toolchain.toml` 固定 Rust **1.98.1**，
首次运行会安装该版本与 rustfmt/clippy；依赖版本由 `Cargo.lock` 固定。
tokenizers 的 onig 需要 C 编译器，Linux 构建环境准备 `gcc g++ libc6-dev pkg-config`。
`Cargo.lock` 纳入版本控制，构建不调用 Python 或 Node.js。

```sh
rtk cargo build --locked
rtk cargo run --locked -- --help
rtk cargo fmt --check
rtk cargo clippy --locked --all-targets -- -D warnings
rtk cargo test --locked
```

CLI 使用 `--参数 值` 的独立参数形式；完整默认值见 `--help`，权威边界见
[兼容契约 §7.1](docs/compatibility.md#71-cli-与限制)。`--model` 必填且须为已有目录，
路径相对于当前工作目录；重复参数采用最后一个值。整数只接受十进制数字，正负号、
小数、溢出均拒绝；只有 queue-capacity 可以为 0，max-options 最小为 2。
线程数最多 2147483647，以免后续 ORT C API 转换截断；超时还检查单调时钟可表示范围。
资源参数使用平台 `usize`；未来分配、张量与计时使用点仍须检查乘加溢出及模型支持范围。

以下是无需模型的退出行为检查，`.` 仅用于目录校验，不代表有效 bundle：

```sh
rtk cargo run --locked -- --model . --threads 0
# 退出码 2：配置非法；错误不回显输入值或路径。
rtk cargo run --locked -- --model .
# 退出码 1：Model initialization failed；缺少固定 bundle 文件。
```

`--help` / `-h` 单独使用时退出码 0，不加载模型或原生库。正常启动只消费本地文件：

```sh
rtk cargo run --release --locked -- --model ./models/multilingual \
  --ort-library /opt/onnxruntime/lib/libonnxruntime.so --threads 8 --max-concurrency 2
```

在 Linux 中提供实际 ORT 路径后可执行。`--ort-library` 默认 `libonnxruntime.so`，
遵循 ort 的本地动态库搜索行为；建议使用绝对路径。它是 CLI 配置，不读取 `ORT_DYLIB_PATH`。
先核验内嵌 [manifest](docs/model-manifest.json) 中所有文件的大小和 SHA-256（包括许可文件），
再校验 JSON、任务预算、温度和特殊 token ID。只接受这一版 bundle；改配置也需要更新并重新验收 manifest。
固定图的哈希锁定 #7 已检查的 external-data location，所有引用文件须存在且哈希吻合。
目录在验证后必须保持不变，部署时只读挂载。

Tokenizer 由 Sequence Builder 持有，禁用隐式 truncation/padding，使用 `encode(text, false)`。
按 `--max-concurrency` 创建独立 CPU Session，实际设置 intra/inter-op threads；
inter-op 大于 1 时启用 ORT 并行图执行。每个 Session 验证名称、dtype、shape、动态维度，
并运行 #7 的 `tensor-L2` 输入，检查输出 shape、有限值和 action 概率。
任何阶段失败均退出码 1；全部成功后启动监听，`/readyz` 返回 `{"status":"ready"}`。
队列满不影响 ready；调度器关闭准入时 ready 返回 503，health 仍只表示进程存活。

```sh
curl -fsS http://127.0.0.1:8080/readyz
curl -fsS http://127.0.0.1:8080/v1/system-one \
  -H 'Content-Type: application/json' \
  --data '{"state":"客户要求退款。","questions":{"urgent":{"type":"noul","instructions":"是否紧急？"}}}'
```

## Docker 部署（Linux ARM64 CPU）

需要 Docker 和已经准备好的固定 bundle。普通镜像构建、启动及以下 curl 检查均不需要
Python、Node.js、PyTorch 或 GPU。Dockerfile 固定 Rust / Debian 基础镜像 digest，
按 SHA-256 校验官方 ORT 1.28.0 CPU 包，并使用 `Cargo.lock` 编译 release 二进制；
构建时需要访问镜像仓库、Debian 软件源、crates.io 和 GitHub，不下载或导出模型。
当前只支持 `linux/arm64`，其他架构在构建时明确拒绝；未做 amd64 或裸机性能认证。

先从模型交付者取得与 [manifest 的 files 清单](docs/model-manifest.json) 完全匹配的文件，
放在 `models/multilingual/`：

```text
laya.onnx
laya.onnx.data
laya_config.json
tokenizer/tokenizer.json
tokenizer/tokenizer_config.json
licenses/（manifest 中列出的五个许可文件）
```

本项目未发布可直接下载的 bundle。若没有交付物，由开发者在独立环境按
[一次性离线导出说明](tools/model-prep/README.md)准备并完成对照，再交给部署者。
导出环境使用 Python/PyTorch；它不参与 Docker 构建或服务运行。不要挂载准备目录 `prep/`，
不要将权重写入镜像或 Git。启动器自动核验每个文件的大小与 SHA-256，不接受其他版本的图或配置。
UID/GID `65532:65532` 必须能遍历模型目录并读取文件；保持宿主文件不变，只读挂载不能阻止宿主改写。

```sh
rtk proxy docker build --platform linux/arm64 -t laya-rs:local .
rtk proxy docker run -d --name laya-server --platform linux/arm64 \
  --cpus 8 --memory 12g --memory-swap 12g \
  --read-only --cap-drop ALL --security-opt no-new-privileges \
  --mount "type=bind,source=$PWD/models/multilingual,target=/models/multilingual,readonly" \
  -p 127.0.0.1:8080:8080 laya-rs:local \
  --model /models/multilingual --threads 1 --inter-op-threads 1 \
  --max-concurrency 1 --queue-capacity 32 --queue-timeout 30 \
  --inference-timeout 120 --shutdown-grace 120
rtk proxy docker logs laya-server
rtk proxy curl -fsS http://127.0.0.1:8080/healthz
rtk proxy curl -fsS http://127.0.0.1:8080/readyz
rtk proxy curl -fsS http://127.0.0.1:8080/metrics
rtk proxy curl -fsS http://127.0.0.1:8080/v1/system-one \
  -H 'Content-Type: application/json' \
  --data '{"state":"客户要求退款。","questions":{"urgent":{"type":"noul","instructions":"是否紧急？"}}}'
```

模型校验与预热完成前端口尚未监听，等日志出现 `CPU model initialized` 后重试探活。
`/healthz` 只表示存活，`/readyz` 的 200 才表示可接收推理；镜像不内置额外探活客户端，
由宿主或编排器检查 HTTP。容器内原生库位于 `/opt/onnxruntime/lib`，通过
`LD_LIBRARY_PATH` 提供默认 `libonnxruntime.so`；也可显式传 `--ort-library`。
服务直接作为 PID 1 接收信号，没有 shell 启动包装器。

上面的 8 vCPU / 12 GiB、intra=1、inter=1、concurrency=1 是本票实测配置，不是最优性能建议。
未传命令参数时镜像 CMD 使用 model=/models/multilingual、threads=4、concurrency=1；
自行追加参数会替换整个 CMD，因此须同时传 `--model`，其余未指定值使用 CLI 默认值。
并发槽各自持有一个 Session，增加并发会增加内存。Docker Desktop VM 需留出对应资源；
资源不足时检查容器退出码、`OOMKilled`、日志和实际配额，不用缩小或替换模型冒充通过。

```sh
rtk proxy docker stats --no-stream laya-server
rtk proxy docker inspect laya-server --format '{{json .State}} {{json .HostConfig.Memory}} {{json .HostConfig.NanoCpus}}'
rtk proxy docker stop --timeout 130 laya-server
rtk proxy docker inspect laya-server --format '{{.State.ExitCode}} {{.State.OOMKilled}}'
rtk proxy docker logs laya-server
rtk proxy docker rm laya-server
```

Docker stop 发 SIGTERM：停止准入、ready=503、等待请求返回 unavailable，在途工作完成后退出 0；
超过 `--shutdown-grace` 时记录剩余任务并退出 1。Docker 的 stop timeout 必须大于服务 grace，
否则它可能先发 SIGKILL，无法验证服务自身的退出语义（[Docker stop 文档](https://docs.docker.com/reference/cli/docker/container/stop/)）。
模型目录缺失为退出码 2；文件缺失、哈希错误、不可读或 ORT 加载/ABI 不匹配为退出码 1，
日志提供静态失败类别，不回显模型路径或正文。

服务与 Rust/crate 许可保存在 `/usr/share/doc/laya-server/`，ORT 许可及 ThirdPartyNotices
在 `/usr/share/doc/onnxruntime/`，Debian 许可在 `/usr/share/doc/*/copyright`；模型许可随 bundle 挂载。
实际镜像摘要、动态链接、provider、21 个固定 HTTP 请求和 stop 结果见
[镜像验收及复现命令](docs/validation/docker.md)。

## CPU 原生依赖基线

以下组合已用 Rust 1.98.1 在宿主和 Linux ARM64 编译，Linux 真实 CPU smoke 已执行。
版本来源沿用 [#3 固定研究](docs/research/mvp-runtime.md)，详细依赖与体积见 [#8 记录](docs/validation/model-loader.md)。

| 组件 | 固定版本与启用方式 | 验证边界 |
| --- | --- | --- |
| Rust | 1.98.1，edition 2024 | fmt、clippy、无模型测试 |
| ort / ort-sys | `=2.0.0-rc.13`；关闭 defaults，`std,load-dynamic,api-28` | Linux ARM64 CPU Session 与真实张量 |
| ONNX Runtime | 官方 CPU 1.28.0，C API **28** | GNU/glibc Linux ARM64 |
| tokenizers | `=0.23.2`；关闭 defaults，仅 `onig` | 真实 tokenizer、特殊 token ID、关闭隐式策略 |
| serde / serde_json / sha2 | Cargo.lock | 配置解析及流式 SHA-256 |

不启用 ORT 的 `download-binaries`、`copy-dylibs` 或 GPU provider；不启用 tokenizer 的 HTTP 功能。
构建和启动不下载/导出模型，不调用 Python/Node。加载器先处理 `ort::init_from` 错误再创建环境，
按一次性启动使用，不能在其他代码提前初始化 ORT。
HTTP 使用 Axum 0.8.9，仅启用 HTTP/1、JSON 和 Tokio；OpenMetrics 使用 prometheus-client 0.25.0，
新增依赖、传递依赖与 Linux 编译记录见 [HTTP 验收](docs/validation/http.md)。

官方运行库获取和诊断命令见 [Linux CPU 验收环境的最小复现](docs/validation-environment.md#最小复现)，
可在没有模型时执行。按实际架构从 [微软 v1.28.0 release](https://github.com/microsoft/onnxruntime/releases/tag/v1.28.0)
下载、校验后解压，保留 `lib` 目录、许可证和 ThirdPartyNotices：

| 官方包名 | SHA-256 |
| --- | --- |
| `onnxruntime-linux-aarch64-1.28.0.tgz` | `e15ff8b5d85afe6c144d97c6fd432254bf76a219daaf17658087d6ecb3e8f0bb` |
| `onnxruntime-linux-x64-1.28.0.tgz` | `a3e1b79d7bb1bf09696ce675f49e4064e6c81f6202b8225624fff0e93f8d6407` |

目标为 GNU/glibc Linux CPU；不要混用架构或默认改用 musl/Alpine。
#5 在 Debian 12 ARM64 实测主库要求 GLIBC_2.27、GLIBCXX_3.4.21、CXXABI_1.3.11，
依赖 libc、libstdc++、libgcc、libdl、librt、libpthread、libm 与 AArch64 loader，`ldd` 无缺项。
以上是 #5 原生库基线；Rust Session 的验收范围见 #8 记录。

## 验证范围

Linux CI 运行 fmt、clippy、无模型测试，真实 smoke 以 `#[ignore]` 明确跳过。
它要求 Linux、真实 bundle 与官方 ORT 库，缺文件会失败，不能静默改用 mock。
执行方法、原生失败检查和输出见 [#8 验收记录](docs/validation/model-loader.md)。
#7 的完整离线对照见 [模型清单](docs/model-manifest.json)；#8 验证固定张量加载/运行，
#12 另行验证 Rust Sequence Builder；尚不证明完整答案、Jev 兼容或性能。
绿色 CI 不表示真实模型测试已运行。

需求与任务在 [GitHub Issues](https://github.com/redwolf2019/laya-rs/issues) 管理。
工程技能配置可直接编辑 `docs/agents/*.md`。

## License

项目原创代码使用 [MIT License](LICENSE)，序列算法移植归属见 [NOTICE](NOTICE.md)。
模型权重及第三方运行库遵循各自许可证。
