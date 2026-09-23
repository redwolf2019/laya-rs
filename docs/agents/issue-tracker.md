# Issue tracker: GitHub

本项目的需求、PRD 和任务使用 GitHub Issues：
https://github.com/redwolf2019/laya-rs/issues

使用 `gh` CLI；从本仓库执行时可自动识别远端，其他目录请显式传入
`--repo redwolf2019/laya-rs`。Shell 命令遵循根目录 Agent 文件引用的 RTK 约定。

## 操作约定

- 创建：`rtk gh issue create --title "..." --body-file /path/to/body.md`。
- 读取：`rtk gh issue view <number> --comments`；需要标签时另用
  `rtk gh issue view <number> --json number,title,body,labels,comments`。
- 列表：`rtk gh issue list --state open --json number,title,body,labels`；
  按需添加 `--label` 或修改 `--state`。
- 评论：`rtk gh issue comment <number> --body-file /path/to/comment.md`。
- 标签：`rtk gh issue edit <number> --add-label "..."` 或 `--remove-label "..."`。
- 关闭：`rtk gh issue close <number>`。

多行正文先写入文件，再通过 `--body-file` 提交；保留真实换行。
发布需求到 issue tracker 表示创建 GitHub Issue；获取 ticket 表示读取对应 Issue
及其评论。标签名称见 `docs/agents/triage-labels.md`。

## Pull requests as a triage surface

**PRs as a request surface: no.**

GitHub 的 Issue 与 PR 共用编号空间；对类型不明的编号先确认对象类型。

## Wayfinding

若调用 wayfinder，使用一个带 `wayfinder:map` 标签的 Issue 作为任务地图，
子任务使用 `wayfinder:research`、`wayfinder:prototype`、
`wayfinder:grilling` 或 `wayfinder:task` 标签。优先使用 GitHub 原生 sub-issues
和 issue dependencies；不可用时在地图中维护任务清单，并在子任务顶部记录
`Part of #<map>` 和 `Blocked by: #<number>`。

按地图顺序选取无未关闭依赖且未分配的任务；开始执行时分配给执行者，
完成后记录结论、关闭任务并更新地图。按需创建标签，本次初始化不操作远端。

以上记录操作方式；实际创建、评论、关闭或分配仍须在当前任务授权范围内。
