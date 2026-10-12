# laya-rs

**简体中文** | [English](README.en.md)

**在 Linux、Windows 或 macOS CPU 上自托管 Laya，用 Rust HTTP 服务完成分类、评分和命题判断。**

[![Rust 应用实现](https://img.shields.io/badge/Rust-runtime-F46623?logo=rust&logoColor=F46623&labelColor=24292F)](#构建与-cli)
[![Linux x86_64 与 ARM64 原生部署](https://img.shields.io/badge/Linux-x86__64%20%7C%20ARM64-FCC624?logo=linux&logoColor=FCC624&labelColor=24292F)](docs/installation.md)
[![Windows 11 x64 便携版](https://img.shields.io/badge/Windows_11-x64-0078D4)](#windows-11-x64-便携版)
[![macOS ARM64 便携版](https://img.shields.io/badge/macOS-ARM64-555555?logo=apple)](#macos-apple-silicon-便携版)
[![Docker 仅支持 Linux ARM64](https://img.shields.io/badge/Docker-ARM64-2496ED?logo=docker&logoColor=2496ED&labelColor=24292F)](#docker-部署linux-arm64-cpu)
[![原创代码采用 MIT 许可证](https://img.shields.io/badge/License-MIT-238636?labelColor=24292F)](LICENSE)

`laya-rs` 是 Laya System One（System-1）模型的 Rust 推理运行时，服务程序名为 `laya-server`。
输入文本或结构化 JSON，以及一组类型化问题，即可得到选项、评分或概率。
默认使用 `laya-multilingual`，支持中文、英文及模型支持的其他语言；
模型常驻内存，多个客户端通过 HTTP 共享推理资源。

[选择平台](#选择部署方式) · [Linux](#一键安装--卸载linux-x86_64arm64) ·
[Windows](#windows-11-x64-便携版) · [macOS](#macos-apple-silicon-便携版) · [调用 API](#调用-api) ·
[与 JEV 的异同](#与-jev-的异同) · [验证范围](#验证范围) · [常见问题](#常见问题)

## 能做什么

提交一份 `state` 和一组 `questions`，服务按问题名返回答案：

| 问题类型 | 用途示例 | 返回结果 |
| --- | --- | --- |
| `choice` 选择 | 工单应交给哪个部门？ | 选中的候选项及各项概率 |
| `score` 评分 | 处理优先级有多高？ | 从 0 开始的期望等级，可为小数，并附等级分布 |
| `noul` 命题概率 | 是否需要人工介入？ | 命题成立的概率，范围 0–1 |

适用于工单分流、优先级评分、工具路由，以及是否交给人工或更大模型的判断。
答案来自预先定义的选项或等级，服务不生成聊天回复；输出概率也不保证判断一定正确。

## 与 JEV 的异同

**JEV（官方写作 Jev）是 TypeSafe AI 的 System One 模型；Laya 是另一个公开权重的
System One 模型项目；laya-rs 提供 Laya 的 Rust 本地推理服务。**
JEV 与 Laya 都面向类型化决策，laya-rs 负责 Laya 的本地服务化，不运行 JEV 权重。
模型定位分别见 [TypeSafe 官方介绍](https://typesafe.ai/blog/introducing-system-one-models-and-jev)
和 [Laya 固定版本说明](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/README.md)。

| 维度 | JEV / TypeSafe API | laya-rs |
| --- | --- | --- |
| 共同能力 | 根据状态回答 Choice、Score、Noul 问题 | 相同的三类问题，返回类型化答案与概率 |
| 模型与部署 | 调用 TypeSafe 托管 API，使用其提供的模型名 | 自托管固定 `laya-multilingual` ONNX bundle，在 Linux、Windows x64 或 macOS ARM64 CPU 上推理；发布与实测边界见下表 |
| 推理入口 | `POST /v1/systemone` | `POST /v1/system-one`，路径含连字符 |
| 模型选择 | 请求必填 `model`，可通过 `GET /v1/models` 查询 | 启动时加载固定模型；不提供模型列表或逐请求切换 |
| 响应标识 | `model` 表示实际回答的模型 | 沿用 Laya 参考响应，`model` 固定为 `"rl-agent"` |
| 鉴权与运维 | 使用 TypeSafe API key | 自行设置共享 Bearer 密钥，提供健康检查、指标、有界排队和优雅退出 |

外部 JEV 资料核对于 **2026-09-24**，本项目平台信息核对于 **2026-10-12**：
`[已验证/HIGH，公开文档与仓库契约范围]`。
JEV 字段与路由依据 [TypeSafe OpenAPI](https://api.typesafe.ai/openapi.json)，
laya-rs 行为依据 [API 兼容契约](docs/compatibility.md)；本项目未执行 JEV API 对照。

**laya-rs 不能直接宣称为 JEV 的即插即用替代。** 当前兼容基线是固定版本
`he-jev/laya@c5d7873` 的进程内 `system_one` 语义。迁移客户端时，除 URL 和密钥外，
还须核对请求校验、响应字段和错误处理：例如本项目要求每题提供 `instructions`，
并保留 Laya 的 `rl_agent.act_probability` 扩展字段。
固定 Laya 样例通过对照不代表与 JEV 模型答案一致；本项目也没有与 JEV 同环境、同负载的性能对比。

需要在下列平台提供本地决策 API，可使用 laya-rs；需要 JEV 模型本身，
应使用 TypeSafe 的服务。已有 JEV 集成可先复用业务问题定义，再按本项目契约适配并验证。

## 选择部署方式

核验日期 **2026-10-12**。公开程序版 [v0.1.4](https://github.com/redwolf2019/laya-rs/releases/tag/v0.1.4)
提供下列四种原生程序包。程序与固定模型独立下载，按所用平台选择脚本。
Windows Server 的数值对照、macOS 15 的回退 CI 未通过；Windows 按已验 Windows 11、macOS 按本机 27.0.1 范围发布，详见[失败记录与验收范围](docs/validation/release-v0.1.4.md)。

| 平台与最低版本 | CPU 架构 | 运行方式与入口 | 实际验证范围 |
| --- | --- | --- | --- |
| Linux；未定义统一的发行版/内核下限 | x86_64 / ARM64 | [安装器与系统服务](#一键安装--卸载linux-x86_64arm64)，需交互终端、curl、root/sudo | 原生发布 CI；Debian 12 VM 的 systemd 生命周期；Alpine/musl 与其他管理器的局部检查见[安装验收](docs/validation/installer.md) |
| Windows 11 | x64 | [PowerShell 5.1 控制台便携版](#windows-11-x64-便携版)，需 VC++ x64 运行库 | 11 专业工作站版 build 26300；普通用户与干净环境/VC-UAC 完整矩阵未验收；不扩展到 Windows 10、Server、x86 或 ARM64 |
| macOS 14+（最低构建目标） | Apple Silicon ARM64 | [Shell 前台便携版](#macos-apple-silicon-便携版)，使用系统工具，未公证 | 原生 27.0.1 / M2 Max；14、干净环境和 Gatekeeper 手动许可未验收；不支持 Intel、Universal 或 Rosetta |
| Docker：Linux 容器 | ARM64 | [本地构建 Docker 镜像](#docker-部署linux-arm64-cpu) | Docker Desktop Linux ARM64；不支持该 Dockerfile 构建 linux/amd64 或 Windows/macOS 容器 |

各原生平台也可[从源码构建](#构建与-cli)。详细教程：[Linux](docs/installation.md)、
[Windows](docs/windows.md)、[macOS](docs/macos.md)。Windows/macOS 不注册系统服务或开机自启。

运行预编译包不需要 Rust、Xcode、Python、Node.js、PyTorch 或 GPU；原生运行库随程序包分发，
Windows 另检查系统 VC++ 运行库。ONNX Runtime 是原生依赖，
“Rust 实现”指应用代码；自行导出模型时，才需要独立的 Python/PyTorch 准备环境。

程序与[固定模型包 `model-3bff67d421ae27d4.tar.gz`](https://github.com/redwolf2019/laya-rs/releases/tag/model-3bff67d421ae27d4)
独立分发。安装时核对大小、SHA-256 和模型逐文件 manifest；相同模型校验通过后复用。
同源校验和用于检查完整性，不是独立数字签名。

### Linux 起步配置（仅运行服务）

以下配置用于运行预编译服务和固定的 `laya-multilingual` 模型，不包含源码编译或模型导出：

| 资源 | 推荐配置 |
| --- | --- |
| CPU | 4 核 / 4 vCPU |
| 内存 | 6 GiB |
| 磁盘 | 至少 5 GiB 可用空间，不含操作系统；用于程序、模型、下载解压和日志 |
| GPU / 显存 | 不需要 |

这是依据[安装验收](docs/validation/installer.md)、
[模型体积](docs/model-manifest.json)和[性能记录](docs/mvp-validation.md)给出的起步建议，
严格的硬件下限尚未测定，不将 Linux 配置当作 Windows/macOS 的最低验收结果。
macOS 安装另检查暂存卷可用空间至少为下载量加 8 GiB、程序卷至少 4 GiB；
具体依赖与空间规则见平台教程。长文本、多问题、更高并发和日志保留需按实际负载预留资源。

## 一键安装 / 卸载（Linux x86_64、ARM64）

在有交互终端的 Linux 宿主机或虚拟机执行，需要 root 或 sudo。安装器自动下载并校验
程序、运行库和固定模型，配置后台服务及开机自启。回车使用默认配置，输入 `e` 修改配置。
**默认监听 `0.0.0.0:8080`，API 必须使用密钥；安装器不修改防火墙。**

安装或升级（先完整下载再执行，交互从终端读取）：

```sh
( script=$(mktemp) || exit; trap 'rm -f "$script"' EXIT; curl -fsSL --proto '=https' --tlsv1.2 https://github.com/redwolf2019/laya-rs/releases/latest/download/install.sh -o "$script" && sh "$script" install )
```

卸载：

```sh
( script=$(mktemp) || exit; trap 'rm -f "$script"' EXIT; curl -fsSL --proto '=https' --tlsv1.2 https://github.com/redwolf2019/laya-rs/releases/latest/download/install.sh -o "$script" && sh "$script" uninstall )
```

默认卸载保留模型和配置；交互选择彻底删除后还需输入 `DELETE`。升级保留配置与密钥，
复用有效模型，验收失败回退旧版本。成功安装须通过带鉴权的中文三类型真实推理检查。

程序位于 `/opt/laya-server`，模型位于 `/var/lib/laya-server/models`，配置位于 `/etc/laya-server`。
密钥保存在 root 所有、0600 的 `/etc/laya-server/token.env`，服务以专用非 root 用户运行。
安装后检查就绪并使用本地密钥调用随包提供的三类型真实请求（POSIX Shell）：

```sh
curl -fsS http://127.0.0.1:8080/readyz
sudo sh -c 'token=$(sed -n "s/^LAYA_API_TOKEN=//p" /etc/laya-server/token.env); printf "Authorization: Bearer %s\n" "$token" | curl -fsS --header @- -H "Content-Type: application/json" --data-binary @/opt/laya-server/current/smoke.json http://127.0.0.1:8080/v1/system-one'
```

systemd 正常停止：`sudo systemctl stop laya-server`；其他管理器的配置与生命周期入口见下方教程。
完整的配置、鉴权调用、服务管理、升级和卸载说明见 **[原生安装教程](docs/installation.md)**。
目前适配 systemd、OpenRC、runit 和 dinit，未知环境明确拒绝；
各发行版、架构及服务管理器的实际验证范围见 [安装验收](docs/validation/installer.md)。

## Windows 11 x64 便携版

从 [v0.1.4 Release](https://github.com/redwolf2019/laya-rs/releases/tag/v0.1.4) 下载
[`laya.ps1`](https://github.com/redwolf2019/laya-rs/releases/download/v0.1.4/laya.ps1)、
[`laya-server-windows-x86_64.zip`](https://github.com/redwolf2019/laya-rs/releases/download/v0.1.4/laya-server-windows-x86_64.zip)、
[`release.env`](https://github.com/redwolf2019/laya-rs/releases/download/v0.1.4/release.env) 与 [`SHA256SUMS`](https://github.com/redwolf2019/laya-rs/releases/download/v0.1.4/SHA256SUMS)，
再从上面的固定模型 Release 获取 `model-3bff67d421ae27d4.tar.gz`，放到同一目录。
用 `Get-FileHash` 将脚本、程序包和元数据与 `SHA256SUMS` 对照；模型会按 `release.env` 与逐文件清单校验。
最终产物、同哈希实机复验和未执行项见[发布验收](docs/validation/release-v0.1.4.md)。

在该下载目录打开 **Windows PowerShell 5.1**，核对脚本来源后执行；下载标记和组织执行策略
按[详细教程](docs/windows.md#操作与恢复)处理。普通运行不需 Rust/MSVC 开发工具。
VC++ 缺失或过旧时脚本先征求同意，再验证微软安装程序签名，仅运行库安装请求 UAC；
取消、失败或需要重启均不会报告成功。

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\laya.ps1 Install -ReleaseDirectory (Get-Location).Path
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\laya.ps1 Start
Invoke-RestMethod 'http://127.0.0.1:8080/readyz'
$root = Join-Path $env:LOCALAPPDATA 'laya-server'
$headers = @{ Authorization = 'Bearer '+[IO.File]::ReadAllText((Join-Path $root 'token')) }
$json = '{"state":"urgent refund request","questions":{"q":{"type":"noul","instructions":"Is this urgent?"}}}'
Invoke-RestMethod 'http://127.0.0.1:8080/v1/system-one' -Method Post -Headers $headers -ContentType 'application/json; charset=utf-8' -Body ([Text.Encoding]::UTF8.GetBytes($json))
Remove-Variable headers
```

`RemoteSigned` 仅作用于子进程，组织策略仍优先。示例使用 `Invoke-RestMethod`，不依赖 `curl` 别名。
Install 执行真实推理验收后保持首次部署停止；Start 启动独立控制台进程。停止可从另一窗口执行：

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\laya.ps1 Stop
```

数据根目录为 `%LOCALAPPDATA%\laya-server`：`programs` 存程序，`models` 存模型，
`current.json`/`previous.json` 存配置，`token` 存密钥。目录及密钥 ACL 限当前用户与 SYSTEM；
另一普通用户的实际访问拒绝仍待验收。升级用 `Update -ReleaseDirectory (Get-Location).Path`
读取该目录的新候选；也可使用无本地目录参数的 Update 从最新稳定版下载。
失败回退、事务恢复 `Update -Recover`、外部模型及配置参数见[操作与恢复](docs/windows.md#操作与恢复)。

## macOS Apple Silicon 便携版

从 [v0.1.4 Release](https://github.com/redwolf2019/laya-rs/releases/tag/v0.1.4) 下载
[`laya-macos.sh`](https://github.com/redwolf2019/laya-rs/releases/download/v0.1.4/laya-macos.sh)、
[`laya-server-macos-aarch64.tar.gz`](https://github.com/redwolf2019/laya-rs/releases/download/v0.1.4/laya-server-macos-aarch64.tar.gz)、
[`release.env`](https://github.com/redwolf2019/laya-rs/releases/download/v0.1.4/release.env) 与 [`SHA256SUMS`](https://github.com/redwolf2019/laya-rs/releases/download/v0.1.4/SHA256SUMS)，
再获取固定模型 `model-3bff67d421ae27d4.tar.gz`，放到同一目录。
用 `shasum -a 256` 将脚本、程序包和元数据与 `SHA256SUMS` 对照；模型由脚本校验。
最终产物与同哈希实机复验见[发布验收](docs/validation/release-v0.1.4.md)。

程序与 dylib 为 **ad-hoc 签名，未公证**。浏览器下载可能附带 quarantine；
核验来源后按[具体文件手动允许运行说明](docs/macos.md#未公证分发)处理。
该 Gatekeeper 流程尚未实测，脚本不关闭防护或自动清除隔离属性。
普通用户使用系统 Bash 和工具即可，无需 sudo、Homebrew 或 Xcode。

在下载目录的终端执行（Bash/zsh 均可调用）：

```sh
bash ./laya-macos.sh install --release-directory "$PWD"
bash ./laya-macos.sh start
```

install 只准备文件，start 校验并运行三类型真实推理后留在当前终端。另开终端检查并调用：

```sh
curl -fsS http://127.0.0.1:8080/readyz
LAYA_API_TOKEN=$(cat "$HOME/Library/Application Support/laya-server/token")
printf 'Authorization: Bearer %s\n' "$LAYA_API_TOKEN" | \
  curl -fsS --header @- -H 'Content-Type: application/json' \
  --data '{"state":"urgent refund request","questions":{"q":{"type":"noul","instructions":"Is this urgent?"}}}' \
  http://127.0.0.1:8080/v1/system-one
unset LAYA_API_TOKEN
```

回到下载目录后停止：

```sh
bash ./laya-macos.sh stop
```

数据根目录为 `$HOME/Library/Application Support/laya-server`，目录 0700、密钥 `token` 为 0600，
移除继承 ACL。`programs`、`models`、`current.env`/`previous.env` 和 `run` 分别保存程序、
模型、配置和日志。升级用 `update --release-directory "$PWD"` 读取该目录的新候选；
也可直接使用 update 从最新稳定版下载。原来运行的实例更新或回退后由当前终端承载。
事务恢复 `update --recover` 和外部目录规则见[升级与恢复](docs/macos.md#升级回退和中断恢复)。

### 监听、密钥与便携版维护

三个原生安装脚本默认 `0.0.0.0:8080`、threads=2、inter-op=1、max-concurrency=1、
shutdown-grace=120 秒；通用 CLI 默认为 threads=8、max-concurrency=2，其余上述默认相同。
仅本机使用可在 Windows Install 指定 `-Listen '127.0.0.1:8080'`，或 macOS install 指定
`--listen 127.0.0.1:8080`；Linux 在交互配置中修改。改端口后同步调整请求地址。
脚本不改防火墙，HTTP 仅供受控内网；生产入口由网关提供 HTTPS。

首次准备自动生成共享 Bearer 密钥，升级、回退复用，日常启动不回显。
不要开启 Shell 跟踪、打印鉴权头或把密钥放入源码、日志、URL、浏览器前端。
轮换时先停止服务，用新的密码学随机密钥替换上述 token 文件内容（Linux 保留
`LAYA_API_TOKEN=` 前缀），保持原所有者与权限，再启动并更新客户端；没有在线轮换或自动过期。

Windows Ctrl+C/Ctrl+Break、macOS Ctrl+C 和脚本 Stop/stop 进入正常 drain；
grace 内排空退出 0，到期非零退出。关闭窗口、注销、关机或强杀可能中断推理与升级。
升级切换有停机：下载/校验失败保留旧进程，新版真实推理失败恢复旧版并再次验证；
配置、密钥、有效模型和旧程序保留。两种便携版无卸载命令：先停止并确认进程退出，
再手动移除本部署目录；需保留数据时先备份密钥、配置与模型，外部模型目录不随部署清理。

## 调用 API

以下为 POSIX Shell 示例，假设客户端已配置 `LAYA_API_TOKEN`；Windows PowerShell 调用见上方快速开始。
使用 Linux 一键安装时，可先按
[检查与调用](docs/installation.md#检查与调用)读取安装配置并发出鉴权请求。
服务启动后，发送一条包含三种问题的请求：

```sh
printf 'Authorization: Bearer %s\n' "$LAYA_API_TOKEN" | \
  curl -fsS http://127.0.0.1:8080/v1/system-one --header @- \
  -H 'Content-Type: application/json' \
  --data '{
    "state": "客户说重复扣款，希望立即退款。",
    "questions": {
      "department": {
        "type": "choice",
        "instructions": "应该由哪个部门处理？",
        "criteria": {"billing": "支付、退款、账单", "technical": "技术问题", "sales": "销售问题"}
      },
      "priority": {
        "type": "score",
        "instructions": "这个请求的处理优先级如何？",
        "criteria": ["低", "中", "高", "紧急"]
      },
      "urgent": {
        "type": "noul",
        "instructions": "这个请求是否紧急？"
      }
    }
  }'
```

成功响应包含 `model`、`answers` 和 `usage`。读取本例结果时：

- `answers.department.choice`：选中的部门键；`probabilities` 为各部门概率。
- `answers.priority.score`：0–3 之间的评分，对应从“低”到“紧急”的有序等级。
- `answers.urgent.noul`：紧急这一命题成立的概率。

`usage.input_tokens` 为输入 token 数，`usage.output_tokens` 为 0。
完整响应字段、输入限制与错误码见 [API 契约](docs/compatibility.md)。

| 路由 | 用途 |
| --- | --- |
| `POST /v1/system-one` | 需要 Bearer token；提交状态与问题，返回推理结果 |
| `GET /healthz` | 免鉴权；检查进程存活 |
| `GET /readyz` | 免鉴权；检查服务是否可接收推理请求 |
| `GET /metrics` | 需要 Bearer token；获取 Prometheus/OpenMetrics 指标 |

缺少、错误或重复的 Authorization 返回 `401`，响应为
`{"error":{"code":"unauthorized","message":"Unauthorized"}}`。
Prometheus 抓取也须配置 Bearer 凭证；手动检查：

```sh
printf 'Authorization: Bearer %s\n' "$LAYA_API_TOKEN" | curl -fsS http://127.0.0.1:8080/metrics --header @-
```

## Docker 部署（Linux ARM64 CPU）

需要 Docker 和固定版本的模型包。当前 Dockerfile 只支持 `linux/arm64`；
以下配置沿用已验收的 8 vCPU / 12 GiB 资源配额，Docker Desktop 需预留相应资源。
验收环境与性能数据见 [MVP 验收报告](docs/mvp-validation.md)。

### 1. 准备模型

可从 [GitHub Releases](https://github.com/redwolf2019/laya-rs/releases) 下载固定的
[`model-3bff67d421ae27d4.tar.gz`](https://github.com/redwolf2019/laya-rs/releases/download/model-3bff67d421ae27d4/model-3bff67d421ae27d4.tar.gz)，
解压到 `models/multilingual/`。原生安装器会自动完成这一步。
如需自行导出，按[模型准备教程](tools/model-prep/README.md)完成一次性导出与对照；
仅导出需要独立的 Python/PyTorch 环境，Docker 镜像构建和服务运行不需要它们。

在仓库根目录准备以下文件，内容须与[固定模型清单](docs/model-manifest.json)一致：

```text
models/multilingual/
├── laya.onnx
├── laya.onnx.data
├── laya_config.json
├── tokenizer/
│   ├── tokenizer.json
│   └── tokenizer_config.json
└── licenses/                 # 清单列出的全部许可文件
```

启动时会校验文件大小和 SHA-256，不接受其他版本的模型或配置。
容器用户 `65532:65532` 须能读取这些文件；运行期间保持模型目录不变。

### 2. 构建并启动

在仓库根目录执行。构建需要联网下载依赖，不会下载模型：

```sh
export LAYA_API_TOKEN="$(openssl rand -hex 32)"
docker build --platform linux/arm64 -t laya-rs:local .
docker run -d --name laya-server --platform linux/arm64 \
  --env LAYA_API_TOKEN \
  --cpus 8 --memory 12g --memory-swap 12g \
  --read-only --cap-drop ALL --security-opt no-new-privileges \
  --mount "type=bind,source=$PWD/models/multilingual,target=/models/multilingual,readonly" \
  -p 127.0.0.1:8080:8080 laya-rs:local \
  --model /models/multilingual --threads 1 --inter-op-threads 1 \
  --max-concurrency 1 --shutdown-grace 120
```

查看启动日志并检查就绪状态：

```sh
docker logs laya-server
curl -fsS http://127.0.0.1:8080/readyz
```

模型校验和预热完成前端口不会监听；就绪后返回 `{"status":"ready"}`。
若容器退出，先查看日志并核对模型文件、读取权限与内存配额。

停止服务时，给在途请求留出退出时间：

```sh
docker stop --timeout 130 laya-server
docker rm laya-server
```

`--timeout` 应大于服务的 `--shutdown-grace`（默认 120 秒）。

`LAYA_API_TOKEN` 是所有受控客户端共享的密钥；将其通过部署密钥管理分发给后端服务或内部脚本。
缺失、为空或格式非法时服务拒绝启动；不设置自动过期，更换后须用新环境变量重建容器
（直接运行二进制时重启进程）。不要在源码、日志或浏览器前端保存密钥。
生产入口由网关提供 HTTPS，服务 HTTP 端口只允许网关或受控内网访问。

## 构建与 CLI

以下仅面向开发者；预编译用户不需安装构建工具。仓库固定 Rust 1.98.1、
`ort 2.0.0-rc.13`、ORT CPU 1.28.0、tokenizers 0.23.2，使用 Cargo.lock。

| 构建平台 / Rust 目标 | 构建工具与 ORT |
| --- | --- |
| Linux / `x86_64-unknown-linux-gnu`、`aarch64-unknown-linux-gnu` | Rustup、C/C++（`gcc g++ libc6-dev pkg-config`）；`libonnxruntime.so`，见[原生库获取与校验](docs/validation-environment.md#官方-cpu-原生库) |
| Windows / `x86_64-pc-windows-msvc` | Rust、MSVC C/C++ 与 Windows SDK，在 x64 Developer PowerShell 构建；默认加载 exe 旁的 `onnxruntime.dll`，见[Windows 构建入口](docs/windows.md#操作与恢复) |
| macOS / `aarch64-apple-darwin` | Rust、Xcode/CLT 与 SDK，`MACOSX_DEPLOYMENT_TARGET=14.0`；默认加载程序旁的 `libonnxruntime.dylib`。本项目单独构建 CPU-only ORT，官方 macOS 包的非 CPU device 会被拒绝；CMake/Python/Git 等依赖准备工具见[macOS 构建入口](docs/macos.md#开发与发布) |

Linux 源码运行示例：

```sh
cargo build --release --locked
./target/release/laya-server --help
# 已配置 LAYA_API_TOKEN；首次本地试用可用 openssl rand -hex 32 生成后 export。
./target/release/laya-server --model ./models/multilingual \
  --ort-library /opt/onnxruntime/lib/libonnxruntime.so --threads 2 --max-concurrency 1
```

将 `--ort-library` 替换为本机动态库的实际路径。
全部参数与默认值见 `--help` 或 [CLI 契约](docs/compatibility.md#71-cli-与限制)。

## 验证范围

平台与发布状态复核于 **2026-10-12**；以下为既有验收记录，
`[已验证/HIGH，限所列模型、输入与环境]`，不把无模型 CI 当作真实推理或性能验收。

- **Laya 语义对照**：21 个固定请求在 Linux ARM64 CPU 上通过真实推理对照，
  四位舍入字段与离散答案精确一致，logits 和未舍入概率满足契约容差。
  见 [后处理验收](docs/validation/postprocess.md) 与 [HTTP 验收](docs/validation/http.md)。
- **负载与生命周期**：Docker Desktop Linux ARM64 的 12 组负载共 2,180 个请求通过
  固定 Laya 答案比较，并完成资源、信号和退出回归。环境、原始数据和复现命令见
  [MVP 验收报告](docs/mvp-validation.md)，不将其延迟或吞吐量外推到其他硬件与输入。
- **原生发布包**：x86_64 与 ARM64 发布 CI 均通过带鉴权的中文三类型真实推理。
  systemd 虚拟机生命周期、其他服务管理器及公开安装命令的不同验证范围见
  [安装验收](docs/validation/installer.md)。
- **Windows 本地候选**：#20 实现 `5403ddd`、独立探针 `fdc7123`、收口 `90fd6e4`；
  `v0.1.2-candidate` 在 Windows 11 build 26300 / i5-13600KF 通过 21 请求、HTTP、退出及升级回退。
  普通用户 ACL 与干净环境/VC-UAC 完整矩阵仍未验收，见[Windows 记录](docs/validation/windows.md)。
- **macOS 本地候选**：#21 实现 `121a103`；`v0.1.1-macos21` 在 macOS 27.0.1 / M2 Max
  通过 21 请求、HTTP、退出及升级回退。macOS 14、无开发工具环境、浏览器 quarantine/Gatekeeper
  手动许可未验收，见[macOS 记录](docs/validation/macos.md)。

#20/#21 按用户调整后的范围关闭，关闭状态不代表暂缓项通过。
Linux 历史验收日期仍为 2026-09-24；平台候选日期为 2026-10-12。
v0.1.4 的发布门禁、资产哈希及原机复验见[发布验收](docs/validation/release-v0.1.4.md)。
发布前的中英文命令、下载入口和引用复核见 [README 核验记录](docs/validation/readme.md)。

这些记录验证实现与固定参考的一致性，不等同于业务分类准确率评测、全部语言评测或 JEV 客户端兼容认证。

## 常见问题

### Windows/macOS 能直接从 latest 安装吗？

可以，`v0.1.4` 已提供两平台资产。上面使用完整下载目录安装；脚本自动下载的方式见平台教程。
普通运行不需要开发工具。Windows 10、macOS Intel/Rosetta 不在本次支持范围，
macOS 14 仅是构建目标。原生平台增加不改变 Docker 的 Linux ARM64 限制。

### 能离线运行吗？业务数据会发送给 JEV 吗？

准备好程序、运行库和固定模型后，laya-rs 在本机执行推理，不调用 JEV 或其他云端推理 API。
安装下载与源码构建需要获取依赖；离线运行与首次离线安装是不同条件。
服务默认不记录状态和问题正文，日志与指标边界见 [观测与退出验收](docs/validation/observability.md)。

### 能处理多长的文本？

当前固定模型每个问题的序列预算为 **1024 token**，包含问题、选项和状态，
并非可单独输入 1024 token 的正文。超长状态按剩余预算截断；同一请求中的每个问题
分别构造序列。精确规则见 [序列与 batch](docs/compatibility.md#3-序列与-batch)。

### 推理速度如何？

延迟取决于 CPU、线程、问题数量、序列长度与排队情况；部署容量应根据自己的负载测量。
本项目的 Linux CPU 数据见 [benchmark](docs/mvp-validation.md#实测结果)，
Windows/macOS 的资源观测见各平台验收记录，不将 Linux 数据、上游 GPU 或 JEV 托管数据
作为其他平台或生产负载的性能承诺。

## 文档导航

| 要做什么 | 文档 |
| --- | --- |
| Linux 安装、升级、管理服务或卸载 | [原生安装教程](docs/installation.md) |
| Windows 控制台运行、依赖、升级与恢复 | [Windows 教程](docs/windows.md) · [验收记录](docs/validation/windows.md) |
| macOS 前台运行、未公证边界与恢复 | [macOS 教程](docs/macos.md) · [验收记录](docs/validation/macos.md) |
| 接入客户端、处理限制与错误 | [API / CLI 兼容契约](docs/compatibility.md) |
| 了解模型来源与文件校验 | [固定模型清单](docs/model-manifest.json) · [模型准备教程](tools/model-prep/README.md) |
| 理解推理链路与实现边界 | [服务方案](docs/laya-server-plan.md) · [领域术语](CONTEXT.md) |
| 复查功能与部署证据 | [MVP 验收](docs/mvp-validation.md) · [安装验收](docs/validation/installer.md) |

## License

项目原创代码使用 [MIT License](LICENSE)，移植代码归属见 [NOTICE](NOTICE.md)。
模型权重及第三方运行库遵循各自许可证。
