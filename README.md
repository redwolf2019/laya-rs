# laya-rs

A pure Rust inference runtime and HTTP server for Laya System-1 models.

面向 Linux CPU 的 Laya 推理服务，计划提供中文、英文及多语言的 Choice、Score、
Noul 判断能力，通过 HTTP 为多个客户端共享模型。

**当前状态：仅完成工程配置与方案文档初始化，尚未实现 HTTP 服务或模型推理。**
仓库目前没有 Cargo 工程、可运行二进制或 Docker 镜像。

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
构建与启动步骤将在可运行服务落地后补充，实施顺序见方案文档。

需求与任务在 [GitHub Issues](https://github.com/redwolf2019/laya-rs/issues) 管理。
工程技能配置可直接编辑 `docs/agents/*.md`。

## License

项目代码使用 [MIT License](LICENSE)。模型权重及第三方运行库遵循各自许可证。
