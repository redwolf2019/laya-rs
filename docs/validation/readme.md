# 中英文 README 核验（#22）

日期：2026-10-12。文档基线为 `121a1031fbc53266364f615e8f43ae68f1aef699`。
本次只修改说明与验证记录，未修改程序、模型、脚本或依赖。
下列已执行项置信度 HIGH；静态核对、真实请求和历史验收分别列明。

## 前置成果与发布状态

#20/#21 均已按用户调整后的范围关闭。本次采用当前 Issue 收口说明和仓库验收记录，
不把 #22 创建时的 Windows 10 或 macOS 14 实测要求重新当作已通过条件。

| 平台 | 合入依据 | 采用的产物与证据 |
| --- | --- | --- |
| Linux | 公开 tag `v0.1.1` | [安装验收](installer.md)；本次重新下载同一 ARM64 包，SHA-256 `01b0bd1ab42341fe8262af3a8fe933e776439336fc11bb4cae7d95516d6247ba` |
| Windows | 实现 `5403ddd`、探针 `fdc7123`、收口 `90fd6e4` | [Windows 记录](windows.md)，本地 `v0.1.2-candidate`；ZIP SHA-256 `d28a5f51d68276fc6c603eed8418cf179809a57910c0f6cc18077ea72fe924fe` |
| macOS | 实现 `121a103` | [macOS 记录](macos.md)，本地 `v0.1.1-macos21`；包 SHA-256 `834b52aeb8bb82564940be32e83e21e406c888638997c130bffbc1ba110d6ecb` |

通过 GitHub Releases API 复核：最新公开稳定程序版仍为 `v0.1.1`，只有 Linux 资产。
Windows/macOS 候选不是公开 Release，README 使用本地候选目录入口并链接构建与组装步骤。
没有发布、推广候选或改变前置任务的验收范围。

固定模型 ID 为 `3bff67d421ae27d4`，公开模型归档 752018537 bytes，SHA-256
`699aae05cab2f36792859f59c533280daf8bcb96c57fae21d30850fc7ad8dfe5`。
GitHub asset digest 与本地候选清单一致；匿名 HTTPS HEAD 跟随重定向返回 200。
本次未重新下载完整模型，使用已有同哈希归档/逐文件校验通过的模型。

公开 latest 安装脚本完整下载成功，SHA-256
`8f980b000eac3eec1e525eac3dbf5605c985939eb896e4031a6b182cd4eb1ee3`，与 v0.1.1 资产 digest 相符。

## README 命令复验

直接从本次 README 的代码块提取命令执行；两种语言的 Windows/macOS 快速开始代码块逐字一致。
候选目录的脚本复制自当前仓库：

- `laya.ps1` SHA-256：`fd54cbfac8dc2e151af81a8519ff1f2beae3756327041e8963e544135486de89`，与 Windows 验收主机脚本一致。
- `laya-macos.sh` SHA-256：`c12791e7558e28877b03af31ff0145eca0d44ddc4bbf52bfb49609223c6dc18a`，与 [macOS 产物记录](macos/artifacts.json)一致。

| 入口 | 本次执行范围与结果 |
| --- | --- |
| Linux readiness 与 `sudo sh -c` 鉴权调用 | Docker Desktop Linux ARM64，临时 Debian/Rust 容器内解包公开 v0.1.1 原生包；按 README 的 `/opt/laya-server/current/smoke.json`、`/etc/laya-server/token.env` 路径运行原样两条命令。ready 返回 `ready`，中文 Choice/Score/Noul 请求返回 `model=rl-agent`、三类答案、input_tokens=133、output_tokens=0 |
| Windows 快速开始 | 原 #20 主机，Windows 11 build 26300，PowerShell 5.1.26100.9444。默认数据目录原先不存在，8080 无监听；从本地候选目录原样执行 Install、Start、readyz、显式 UTF-8 Noul 请求及 Stop，全部退出成功，进程记录移除 |
| macOS 快速开始 | 原 #21 本机 macOS 27.0.1 / M2 Max。默认数据目录原先不存在，8080 无监听；从本地候选目录原样执行 install、start、readyz、读取 token 后经标准输入传鉴权头、Noul 请求和 stop。start 退出码 0，进程记录移除 |
| 通用 API 与 metrics | 两份 README 的三类型请求分别在上述 macOS 候选上运行；均有 department/priority/urgent 答案。`--header @-` 的 metrics 请求成功，返回 `laya_` 指标 |
| 本地候选更新入口 | Windows `Update -ReleaseDirectory (Get-Location).Path`、macOS `update --release-directory "$PWD"` 对同一候选重复执行，真实三类型检查通过，原来停止的部署仍保持停止；不同版本切换与故障回退复用原平台验收 |

Windows/macOS Noul 示例均返回以下内容；Windows 另将响应对象转为 JSON 并断言类型、概率范围和 usage，
此格式化不改变请求命令：

```json
{"model":"rl-agent","answers":{"q":{"type":"noul","noul":0.9884,"rl_agent":{"act_probability":1.0}}},"usage":{"input_tokens":33,"output_tokens":0}}
```

Linux 容器检查仅证明这组程序与 API 命令可运行，不是系统服务安装或权限验收。
公开安装/卸载命令保持原样，复用 [v0.1.1 公开安装验收](installer.md#正式发布包)。
systemd 停止命令与该版本 `scripts/install.sh` 的 `service_stop` 一致；上述正式卸载已实际调用它。
其他服务管理器不新增未原样验收的停止命令，继续链接原有适配范围。

## 文档检查与评审

- 中英文逐节核对平台、默认值、依赖、路径、密钥、停机、回退、未公证状态和未执行项。
- 两份 README 与本记录的 149 个本地链接/锚点检查通过；24 个 Shell 代码块通过 `bash -n`；
  `git diff --check` 通过。
- 核对 Dockerfile 的 `linux/arm64` 限制、发布工作流步骤、平台脚本参数与通用 CLI 默认值。
- `cargo fmt --check`、`cargo clippy --locked --all-targets -- -D warnings` 通过；
  本机 `cargo test --locked` 为 85 passed、8 ignored，不把 ignored 项算作执行通过。
- code-review 的 Standards 轴未发现规范问题。Spec 轴补查时发现 runit 简写停止命令不能对应原样记录，
  已从两份 README 移除，保留有同版本证据的 systemd 入口。两轴复审均无遗留发现。
- Windows/macOS 验证创建的部署已确认停止并清理；Linux 临时容器已退出并移除，未接管既有实例。

## 未执行与适用边界

未重复完整模型数值对照、benchmark、故障回退矩阵或 Linux 系统服务安装；采用上方各平台原验收记录。
本次 README 请求复验不扩大模型语义、性能、权限或分发结论。
Windows 仍未验普通用户与跨用户 ACL、干净系统/VC-UAC、公开候选；macOS 仍未验 14、
无开发工具系统、浏览器 quarantine/Gatekeeper 手动许可。此次 Windows 会话不能替代普通用户验收。
JEV 外部资料未重新核对，README 保留 2026-09-24 核对日期。
