# laya-rs

A pure Rust inference runtime and HTTP server for Laya System-1 models.

面向 Linux CPU 的 Laya 推理服务，计划提供中文、英文及多语言的 Choice、Score、
Noul 判断能力，通过 HTTP 为多个客户端共享模型。

**当前状态：Rust 工程与 CLI 配置校验可运行，HTTP 服务和模型推理尚未实现。**
正常启动校验配置后仍以非零码退出，不监听端口；当前没有服务 Docker 镜像。

## 文档

- [服务方案与验收条件](docs/laya-server-plan.md)
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
首次运行会安装该版本与 rustfmt/clippy；应用目前只用标准库，无第三方 Cargo 依赖。
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
# 退出码 1：Service not implemented；尚未加载模型或启动 HTTP。
```

`--help` / `-h` 单独使用时退出码 0。当前只校验目录存在，文件清单、哈希、tokenizer、
Session 与 bundle 支持范围由后续加载任务实现；任何合法 CLI 都不能启动可用服务。

## CPU 原生依赖基线

以下版本沿用 [#3 固定研究](docs/research/mvp-runtime.md)，本票复核了官方 manifest、
crates.io 索引和固定源码。仅 Rust 用于当前构建，其余在实际使用的任务中加入并锁定，
不把候选版本表称为已编译的推理组合：

| 组件 | 固定版本与启用方式 | 验证边界 |
| --- | --- | --- |
| Rust | 1.98.1，edition 2024；项目最低版本同为 1.98.1 | 本票编译与无模型测试 |
| ort / ort-sys | `=2.0.0-rc.13`；关闭 defaults，`std,load-dynamic,api-28` | 固定源码声明 MSRV 1.88，MIT OR Apache-2.0；尚未加入 Cargo |
| ONNX Runtime | 官方 CPU 1.28.0，C API **28** | Linux x64/aarch64 官方包；MIT；ARM64 原生加载已由 #5 验证 |
| tokenizers | `=0.23.2`；关闭 defaults，仅 `onig` | Apache-2.0；发布 manifest 未声明 MSRV，须在接入时用固定工具链编译验证 |

Axum 0.8、Tokio、Serde 等当前没有调用点，暂不引入；接入时检查实际版本、MSRV、许可证、
Linux CPU 支持、传递依赖和体积，再写入 Cargo.lock。tokenizers 的 onig 带来原生 C 构建需求，
同样留到实际依赖构建验证，不能由当前标准库工程通过推定其可用。

动态加载不启用 `download-binaries`、`copy-dylibs` 或 GPU provider，也不在 Cargo 构建中下载
ORT。后续加载器应先调用可失败的 `ort::init_from(path)` 并处理错误，再创建 Session；
`EnvironmentBuilder::commit()` 返回 bool。当前 CLI 没有 ORT 路径参数，也不会加载原生库。
[固定 ort 源码](https://github.com/pykeio/ort/tree/002f41a8e175eac7f6695ff361d2e51a50874c48)。

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
这只证明该容器原生加载/C API/provider 检查通过，未证明 Rust ort Session 或模型推理通过。

## 验证范围

Linux CI 运行 fmt、clippy 和无模型单元/CLI 测试，独立列出 **Real model validation — NOT RUN**。
缺权重且真实模型测试尚未实现；绿色 CI 不表示 multilingual 推理、Jev 兼容性或性能通过。
CLI 测试中的已有目录也不充当模型测试。具体工具链与本地 Linux 执行结果见
[#6 验收记录](https://github.com/redwolf2019/laya-rs/issues/6)。

需求与任务在 [GitHub Issues](https://github.com/redwolf2019/laya-rs/issues) 管理。
工程技能配置可直接编辑 `docs/agents/*.md`。

## License

项目代码使用 [MIT License](LICENSE)。模型权重及第三方运行库遵循各自许可证。
