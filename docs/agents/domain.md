# Domain docs

本项目采用 **single-context**：根目录 `CONTEXT.md` 保存领域术语，
`docs/adr/` 保存必要的架构决策。没有 `CONTEXT-MAP.md` 或分包领域文档。

## 阅读顺序

探索代码或制定方案前：

1. 阅读根目录 `CONTEXT.md`。
2. 阅读 `docs/adr/` 中与当前任务相关的 ADR。
3. 实现功能时阅读 `docs/laya-server-plan.md` 中对应的需求与验收条件。

如果领域词汇或 ADR 文件不存在，静默继续，不因缺失而阻塞工作或要求补建。
后续出现 `CONTEXT-MAP.md` 时，按其指引读取相关领域的 `CONTEXT.md`。

## 文件职责

- `CONTEXT.md`：仅记录领域术语，不保存实现细节、任务清单或接口规格。
- `docs/laya-server-plan.md`：项目目标、技术方案、兼容性约束及验收条件。
- `docs/adr/NNNN-short-title.md`：记录难以逆转、需要背景且包含真实取舍的决策。
  有实际决策时再创建目录和文件，不生成空白 ADR。

## 使用规则

Issue、设计说明、代码和测试中的领域命名遵循 `CONTEXT.md`。
发现术语缺口时，先确认含义，再由 domain-modeling 补充。
如果提案与现有 ADR 冲突，应明确指出对应 ADR 和重新讨论的理由，不静默覆盖。
