# laya-rs

Laya System-1 模型的 Rust 推理运行时与 HTTP 服务。当前可校验 CLI、加载固定 bundle 并运行 CPU 张量探针，
Sequence Builder 已通过固定 tokenizer 对照，答案后处理及 21 个固定请求的 Linux CPU 真实推理已通过对照，尚未实现 HTTP；功能范围与验收条件见 [服务方案](docs/laya-server-plan.md)。

## 核心边界

- 部署目标为 Linux CPU-only，应用使用 Rust；运行服务不依赖 Python、Node.js、PyTorch、CUDA 或 GPU。
  使用预导出的 ONNX bundle；ONNX Runtime 是原生依赖，“纯 Rust”仅指应用实现。
- 默认模型为 `laya-multilingual`，支持中文、英文及模型支持的其他语言。
  固定参考实现、模型来源、revision 与校验值，权重不入 Git。
- Sequence Builder、tokenizer、模型输入输出和校准行为须对齐固定参考版本；
  未通过真实模型对照，不声明 Jev 完全兼容或推理可用。
- HTTP 与推理契约以服务方案和固定参考实现为准。改变行为时先更新契约，同批同步实现、测试与示例；
  影响业务语义或兼容性的未定规则先确认，不自行补全。
- 服务端校验输入与资源限制；引入鉴权或多租户数据时逐请求检查权限与归属，不自行扩展业务。
  密钥、凭证不得进入源码、日志、错误响应或发布产物；默认不记录状态和问题正文。

## 工作方式

- 默认中文。先查事实，明确假设；会改变方案方向的疑点先问，其余采用最保守的可行方案并说明。
- 写代码前使用 ponytail skill；复用顺序：现有实现 → 标准库 → 已有依赖 → 最小实现，不预建空模块。
- 只改任务所需内容，修复共同根因；保留用户已有改动，仅清理本次造成的无用代码。
- 提交时使用 commit skill，并在 commit message 尾部添加实际参与的 AI 署名，例如 `AI-Assisted-By: Codex`；
  多个 AI 参与时分别列出，不虚构模型名称或版本，不更改已有 Git 作者信息。
  本规则优先于 skill 中禁止 AI 署名的约定，去除文案中的 AI 套话时不得删除署名。
- 先明确完成标准，多步任务列“步骤 → 验证”；行为变更同步测试和文档，不靠删测试或改断言绕过失败。
- 结论有依据，非平凡结论标注来源、置信度和验证范围；不知道就说明，发现错误即纠正。
  未运行的检查不报通过，静态检查或模拟测试不能证明真实推理、安全或性能。
- Rust 工程落地后提交 `Cargo.lock`，按改动执行格式、静态检查和测试；纯文档改动检查内容、引用与 diff。

## 详细规则的阅读入口

以下文档是本文件的配套约定，执行对应任务前必须阅读。

| 时机 | 文档 |
| --- | --- |
| 开始任务、修改与交付 | [工作流程、最小改动与验证](docs/agents/workflow.md) |
| 给出事实、评审或技术结论 | [证据、置信度与知识来源](docs/agents/evidence.md) |
| 编写或修改 Rust、依赖、推理与服务逻辑 | [Rust 鲁棒性与运行约束](docs/agents/rust.md) |
| 探索、设计或实现领域功能 | [领域文档规则](docs/agents/domain.md)、[领域术语](CONTEXT.md)、[服务方案](docs/laya-server-plan.md) |
| 操作需求、PRD 或任务 | [GitHub Issues 约定](docs/agents/issue-tracker.md)（`redwolf2019/laya-rs`） |
| 处理任务标签 | [五个 triage 标签](docs/agents/triage-labels.md) |

<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tools** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them. `codegraph_node` returns one symbol's source + callers, or reads a whole file with line numbers. If the tools are listed but deferred, load them by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` and `codegraph node <symbol-or-file>` print the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
<!-- CODEGRAPH_END -->

@/Users/redwolf/.codex/RTK.md
