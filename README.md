# laya-rs

**简体中文** | [English](README.en.md)

**在 Linux CPU 上自托管 Laya，用 Rust HTTP 服务完成分类、评分和命题判断。**

[![Rust 应用实现](https://img.shields.io/badge/Rust-runtime-F46623?logo=rust&logoColor=F46623&labelColor=24292F)](#构建与-cli)
[![Linux x86_64 与 ARM64 原生部署](https://img.shields.io/badge/Linux-x86__64%20%7C%20ARM64-FCC624?logo=linux&logoColor=FCC624&labelColor=24292F)](docs/installation.md)
[![Docker 仅支持 Linux ARM64](https://img.shields.io/badge/Docker-ARM64-2496ED?logo=docker&logoColor=2496ED&labelColor=24292F)](#docker-部署linux-arm64-cpu)
[![原创代码采用 MIT 许可证](https://img.shields.io/badge/License-MIT-238636?labelColor=24292F)](LICENSE)

`laya-rs` 是 Laya System One（System-1）模型的 Rust 推理运行时，服务程序名为 `laya-server`。
输入文本或结构化 JSON，以及一组类型化问题，即可得到选项、评分或概率。
默认使用 `laya-multilingual`，支持中文、英文及模型支持的其他语言；
模型常驻内存，多个客户端通过 HTTP 共享推理资源。

[安装部署](#一键安装--卸载linux-x86_64arm64) · [调用 API](#调用-api) ·
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
| 模型与部署 | 调用 TypeSafe 托管 API，使用其提供的模型名 | 自托管固定 `laya-multilingual` ONNX bundle，在 Linux CPU 上推理 |
| 推理入口 | `POST /v1/systemone` | `POST /v1/system-one`，路径含连字符 |
| 模型选择 | 请求必填 `model`，可通过 `GET /v1/models` 查询 | 启动时加载固定模型；不提供模型列表或逐请求切换 |
| 响应标识 | `model` 表示实际回答的模型 | 沿用 Laya 参考响应，`model` 固定为 `"rl-agent"` |
| 鉴权与运维 | 使用 TypeSafe API key | 自行设置共享 Bearer 密钥，提供健康检查、指标、有界排队和优雅退出 |

对比核对于 **2026-09-24**：`[已验证/HIGH，公开文档与仓库契约范围]`。
JEV 字段与路由依据 [TypeSafe OpenAPI](https://api.typesafe.ai/openapi.json)，
laya-rs 行为依据 [API 兼容契约](docs/compatibility.md)；本项目未执行 JEV API 对照。

**laya-rs 不能直接宣称为 JEV 的即插即用替代。** 当前兼容基线是固定版本
`he-jev/laya@c5d7873` 的进程内 `system_one` 语义。迁移客户端时，除 URL 和密钥外，
还须核对请求校验、响应字段和错误处理：例如本项目要求每题提供 `instructions`，
并保留 Laya 的 `rl_agent.act_probability` 扩展字段。
固定 Laya 样例通过对照不代表与 JEV 模型答案一致；本项目也没有与 JEV 同环境、同负载的性能对比。

需要在自有 Linux 环境提供本地决策 API，可使用 laya-rs；需要 JEV 模型本身，
应使用 TypeSafe 的服务。已有 JEV 集成可先复用业务问题定义，再按本项目契约适配并验证。

## 选择部署方式

| 方式 | 适用环境 | 准备工作 |
| --- | --- | --- |
| [一键安装](#一键安装--卸载linux-x86_64arm64) | Linux x86_64 / ARM64 宿主机或虚拟机 | 交互终端、root 或 sudo；自动下载程序、运行库和模型 |
| [Docker 部署](#docker-部署linux-arm64-cpu) | Linux ARM64 容器 | Docker、固定模型包；在本地构建镜像 |
| [源码构建](#构建与-cli) | Linux 开发环境 | Rust 工具链、固定模型包、ONNX Runtime CPU 动态库 |

运行服务不需要 Python、Node.js、PyTorch 或 GPU。ONNX Runtime 是原生依赖，
“Rust 实现”指应用代码；自行导出模型时，才需要独立的 Python/PyTorch 准备环境。

### 推荐最低配置（仅运行服务）

以下配置用于运行预编译服务和固定的 `laya-multilingual` 模型，不包含源码编译或模型导出：

| 资源 | 推荐配置 |
| --- | --- |
| CPU | 4 核 / 4 vCPU |
| 内存 | 6 GiB |
| 磁盘 | 至少 5 GiB 可用空间，不含操作系统；用于程序、模型、下载解压和日志 |
| GPU / 显存 | 不需要 |

这是依据[安装验收](docs/validation/installer.md)、
[模型体积](docs/model-manifest.json)和[性能记录](docs/mvp-validation.md)给出的起步建议，
严格的硬件下限尚未测定。长文本、多问题或更高推理并发需要按实际负载验证并增加资源；
日志长期保留也需另留磁盘空间。

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

密钥仅 root 可读，保存在 `/etc/laya-server/token.env`，读取命令为
`sudo cat /etc/laya-server/token.env`。就绪检查：`curl -fsS http://127.0.0.1:8080/readyz`。
完整的配置、鉴权调用、服务管理、升级和卸载说明见 **[原生安装教程](docs/installation.md)**。
目前适配 systemd、OpenRC、runit 和 dinit，未知环境明确拒绝；
各发行版、架构及服务管理器的实际验证范围见 [安装验收](docs/validation/installer.md)。

## 调用 API

以下示例假设客户端已配置 `LAYA_API_TOKEN`。使用一键安装时，可先按
[检查与调用](docs/installation.md#检查与调用)读取安装配置并发出鉴权请求。
服务启动后，发送一条包含三种问题的请求：

```sh
curl -fsS http://127.0.0.1:8080/v1/system-one \
  -H "Authorization: Bearer $LAYA_API_TOKEN" \
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
curl -fsS http://127.0.0.1:8080/metrics -H "Authorization: Bearer $LAYA_API_TOKEN"
```

## Docker 部署（Linux ARM64 CPU）

需要 Docker 和固定版本的模型包。当前 Dockerfile 只支持 `linux/arm64`；
以下配置沿用已验收的 8 vCPU / 12 GiB 资源配额，Docker Desktop 需预留相应资源。
验收环境与性能数据见 [MVP 验收报告](docs/mvp-validation.md)。

### 1. 准备模型

可从 [GitHub Releases](https://github.com/redwolf2019/laya-rs/releases) 下载固定的
`model-<id>.tar.gz`，解压到 `models/multilingual/`。原生安装器会自动完成这一步。
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

如需从源码运行，在 Linux 上安装 Rustup、C/C++ 编译工具（`gcc g++ libc6-dev pkg-config`），
并准备模型包及 ONNX Runtime CPU 1.28.0 动态库。
仓库固定 Rust 1.98.1；原生库获取与校验见[运行环境说明](docs/validation-environment.md#官方-cpu-原生库)。

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

以下为截至 **2026-09-24** 的仓库验收记录，`[已验证/HIGH，限所列模型、输入与环境]`：

- **Laya 语义对照**：21 个固定请求在 Linux ARM64 CPU 上通过真实推理对照，
  四位舍入字段与离散答案精确一致，logits 和未舍入概率满足契约容差。
  见 [后处理验收](docs/validation/postprocess.md) 与 [HTTP 验收](docs/validation/http.md)。
- **负载与生命周期**：Docker Desktop Linux ARM64 的 12 组负载共 2,180 个请求通过
  固定 Laya 答案比较，并完成资源、信号和退出回归。环境、原始数据和复现命令见
  [MVP 验收报告](docs/mvp-validation.md)，不将其延迟或吞吐量外推到其他硬件与输入。
- **原生发布包**：x86_64 与 ARM64 发布 CI 均通过带鉴权的中文三类型真实推理。
  systemd 虚拟机生命周期、其他服务管理器及公开安装命令的不同验证范围见
  [安装验收](docs/validation/installer.md)。

这些记录验证实现与固定参考的一致性，不等同于业务分类准确率评测、全部语言评测或 JEV 客户端兼容认证。

## 常见问题

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
不采用上游 GPU 数据或 JEV 托管 API 数据作为本服务的性能承诺。

## 文档导航

| 要做什么 | 文档 |
| --- | --- |
| 安装、升级、管理服务或卸载 | [原生安装教程](docs/installation.md) |
| 接入客户端、处理限制与错误 | [API / CLI 兼容契约](docs/compatibility.md) |
| 了解模型来源与文件校验 | [固定模型清单](docs/model-manifest.json) · [模型准备教程](tools/model-prep/README.md) |
| 理解推理链路与实现边界 | [服务方案](docs/laya-server-plan.md) · [领域术语](CONTEXT.md) |
| 复查功能与部署证据 | [MVP 验收](docs/mvp-validation.md) · [安装验收](docs/validation/installer.md) |

## License

项目原创代码使用 [MIT License](LICENSE)，移植代码归属见 [NOTICE](NOTICE.md)。
模型权重及第三方运行库遵循各自许可证。
