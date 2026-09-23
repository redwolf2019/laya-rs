# laya-rs

A pure Rust inference runtime and HTTP server for Laya System-1 models.

面向 Linux CPU 的 Laya 推理服务，计划提供中文、英文及多语言的 Choice、Score、
Noul 判断能力，通过 HTTP 为多个客户端共享模型。

**当前状态：Rust 已加载并校验固定 bundle、Tokenizer 和 CPU Session，启动时执行真实张量探针。**
HTTP 服务、Sequence Builder 和答案后处理尚未实现。资源初始化成功后仍以非零码退出，
不监听端口；当前没有服务 Docker 镜像。Linux 真实运行结果见 [#8 验收记录](docs/validation/model-loader.md)。

#9 已提供[模型无关的 System One 类型与校验](src/system_one.rs)：请求规范化、
完整响应 DTO、静态类型化错误，以及数字词法/嵌套顺序保留。
`Request::from_slice(body, &limits)` 接收完整 body；`state`/`instructions` 保留 `RawValue`，
`Request::state_text()` / `Question::instructions_text()` 将其渲染为官方模型输入文字：
字符串原样使用，其余按 Python 的 Unicode/ASCII 两种 JSON 模式渲染，保持各层对象顺序。
整数保留精度；浮点按 binary64 舍入并使用 Python 的记数法，包含负零和指数溢出/下溢。
可运行 `rtk cargo test --locked --test system_one` 验证该边界，不需要模型。
文本字节对照运行 `rtk cargo test --locked --test fixtures --test json_text`，
来源与复现见 [fixtures 说明](tests/fixtures/README.md)。这些检查不需要 Python 或模型。

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

Tokenizer 禁用隐式 truncation/padding；后续 Sequence Builder 使用 `encode(text, false)`。
按 `--max-concurrency` 创建独立 CPU Session，实际设置 intra/inter-op threads；
inter-op 大于 1 时启用 ORT 并行图执行。每个 Session 验证名称、dtype、shape、动态维度，
并运行 #7 的 `tensor-L2` 输入，检查输出 shape、有限值和 action 概率。
任何阶段失败均退出码 1；全部成功后持有可复用资源，再报告 HTTP 未实现并退出码 1，不能当成 ready。

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
按一次性启动使用，不能在其他代码提前初始化 ORT。HTTP 等没有调用点的依赖暂不引入。

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
尚不证明 Rust Sequence Builder、完整答案、Jev 兼容或性能。
绿色 CI 不表示真实模型测试已运行。

需求与任务在 [GitHub Issues](https://github.com/redwolf2019/laya-rs/issues) 管理。
工程技能配置可直接编辑 `docs/agents/*.md`。

## License

项目代码使用 [MIT License](LICENSE)。模型权重及第三方运行库遵循各自许可证。
