# Laya Rust HTTP Server 方案

状态：待实现。本文根据用户提供的方案整理；本次只初始化工程配置与文档。
版本日期：2026-09-23。

## 1. 目标与边界

构建名为 `laya-server` 的 Rust HTTP 服务，在 Linux CPU 上加载一个共享的
Laya multilingual 模型，为多个客户端回答类型化问题。

目标能力：

- 中文、英文及模型支持的其他语言。
- Choice 选择、Score 评分、Noul 命题概率。
- Jev-compatible `system_one` 请求与响应语义；兼容程度以对照验证为准。
- 受控 CPU 推理并发、排队观测、健康检查和 Prometheus 指标。

运行服务不需要 Python、Node.js、PyTorch、CUDA 或 GPU。
应用实现使用 Rust；ONNX Runtime 自身是原生运行库，因此不承诺整个二进制依赖栈
仅包含 Rust。使用现成 ONNX bundle，不在本项目的构建或启动流程中引入 Python 导出。

Laya 的职责是有限答案空间上的判断，不是聊天或文本生成，也不替代通用 LLM。
典型用途包括工具路由、工单分类、任务风险评分和是否升级到人工或更大模型的判断。

## 2. 推理链路

```text
HTTP 客户端
  → Axum：解析与校验请求
  → 并发准入与排队
  → Sequence Builder：按参考格式构造每个问题的输入
  → HuggingFace Tokenizers
  → ONNX Runtime CPU
  → 校准与概率分布
  → Choice / Score / Noul 响应
```

模型常驻服务进程；客户端之间共享推理资源，不为每个请求重新加载权重。
同步 CPU 推理在 Tokio 的阻塞执行边界内运行，避免占用异步工作线程。

## 3. 模型资源

默认目标模型：`laya-multilingual`。模型公开资料列出的目标规格为
mmBERT-base、322M 参数、1024 context；实际限制必须读取所选 bundle 的配置，
不能仅凭模型名称硬编码。参考：[Laya 项目](https://github.com/he-jev/laya)。

期望的本地 bundle：

```text
models/multilingual/
├── laya.onnx
├── laya.onnx.data
├── laya_config.json
└── tokenizer/
    ├── tokenizer.json
    └── tokenizer_config.json
```

预导出 bundle 的候选来源为
[receptron/laya-onnx](https://huggingface.co/receptron/laya-onnx)，
选择 `multilingual` 子目录。实施前固定 revision，确认文件齐全、许可证、
文件校验值及 ONNX Runtime CPU 兼容性。本次不下载或提交权重。

## 4. 技术栈与模块

采用 Rust、Axum 0.8、Tokio、Serde / serde_json、HuggingFace Tokenizers、
ort / ONNX Runtime、Tracing 和 Prometheus 指标。
Tower / tower-http 以及 ndarray 按实际使用需要引入，不为目录完整而添加依赖。

原方案中的版本号是选型线索，不是已经验证的依赖组合。
实施时确认 `ort` 实际发布版本、对应 ONNX Runtime ABI 和 Linux CPU 支持，
选择兼容组合并提交 `Cargo.lock`。只启用所需的 CPU 与库功能。

建议的职责划分如下；允许把短小、紧密相关的代码放在同一模块，
待职责或规模需要时再拆分：

| 位置 | 职责 |
| --- | --- |
| `src/main.rs` | 启动、状态装配与退出 |
| `src/api/` | System One、Health、Readiness、Metrics 路由 |
| `src/laya/engine.rs` | 统一推理入口与执行资源 |
| `src/laya/model.rs` | ONNX Session、张量输入与输出 |
| `src/laya/tokenizer.rs` | 加载和使用 HF Tokenizer |
| `src/laya/sequence.rs` | 请求规范化、序列、选项标记、截断与 padding |
| `src/laya/calibration.rs` | 温度参数、softmax 与结果概率 |
| `src/laya/types.rs` | 类型化问题、答案及使用量 |
| `src/config.rs` | 命令行参数和配置校验 |
| `src/error.rs` | HTTP 错误映射 |
| `src/metrics.rs` | 指标注册、计数和耗时观测 |

最终还应包含 `Cargo.toml`、`Cargo.lock`、`Dockerfile` 和运行说明。
本次不创建空代码模块或不可运行的部署文件。

## 5. HTTP API

| 路由 | 语义 |
| --- | --- |
| `POST /v1/system-one` | 对 state 回答 questions |
| `GET /healthz` | 进程存活，返回 `{"status":"ok"}` |
| `GET /readyz` | 模型、Tokenizer、ORT Session 均可用于推理时就绪 |
| `GET /metrics` | Prometheus 文本指标 |

`/v1/system-one` 是本项目约定路径。若目标 Jev 客户端使用不同路径，
在核对协议后提供对应别名；仅路径相似不能证明完整兼容。

请求示例：

```json
{
  "state": {"body": "客户说重复扣款，希望立即退款。"},
  "questions": {
    "department": {
      "type": "choice",
      "instructions": "应该由哪个部门处理？",
      "criteria": {
        "billing": "支付、退款、账单",
        "technical": "技术问题",
        "sales": "销售问题"
      }
    },
    "priority": {
      "type": "score",
      "instructions": "这个请求的处理优先级如何？",
      "criteria": ["low", "medium", "high", "critical"]
    },
    "urgent": {
      "type": "noul",
      "instructions": "这个请求是否紧急？"
    }
  }
}
```

响应顶层包含 `model`、按问题名索引的 `answers` 和 `usage`。
`usage.input_tokens` 按参考实现统计实际输入；
模型不生成文本，`usage.output_tokens` 为 0。

| 类型 | 核心结果 |
| --- | --- |
| Choice | `choice` 为被选键，`probabilities` 为各候选项分布 |
| Score | `score` 为等级索引的期望值，范围 0 到等级数减 1；附等级分布 |
| Noul | `noul` 为 P(true)，范围 0 到 1 |

兼容性不仅限于上面的简化字段。
[参考类型定义](https://raw.githubusercontent.com/receptron/laya/main/src/types.ts)
还包含 Choice / Score 的 `confidence`、Score 的 `legend` 和答案的
`rl_agent.act_probability`。完整响应、请求的可选形式、舍入规则和错误语义
必须以固定版本的参考实现为依据，不将省略字段的示例当作完整协议。

对空问题集、无候选项、无评分等级、不支持的类型、非法字段类型以及超出限制的
请求返回明确错误。实施时明确最大请求体、问题数、选项数、排队容量和等待时限；
这些资源边界不能只依赖模型的 token 截断。

## 6. Sequence Builder 与兼容性

不能用 `state + question` 替代模型训练格式。参考序列具有以下结构：

```text
[CLS] <type> question: instructions [SEP]
[MASK] option0 [MASK] option1 ... [SEP]
state [SEP]
```

实现必须逐项对齐：

- 字符串状态与 JSON 状态的序列化；JSON 键顺序、空格、Unicode 和数值表示。
- Choice 候选项顺序，以及 Score 等级从零开始的有序索引。
- Noul 的 false / true 顺序及默认选项说明。
- 特殊 token 的真实 ID、选项 marker 位置和用户文本中 marker 的处理。
- instructions 与每个选项的 token 预算、状态截断、padding 和 attention mask。
- 一个请求中的多问题批处理布局，以及模型实际要求的其他输入张量。
- input_tokens 的统计边界、输出字段、置信度和 act_probability 的计算。

参考来源：
[Laya 官方实现](https://github.com/he-jev/laya) 与
[ONNX 移植的序列实现](https://raw.githubusercontent.com/receptron/laya/main/src/sequence.ts)。
参考代码只用于理解和验证，部署服务不调用 Python 或 Node.js。
如移植代码，遵循来源许可证并保留必要归属。

## 7. 校准和后处理

流程为 `logits → temperature scaling → softmax → 类型化答案`。
softmax 需要采用数值稳定的实现，拒绝非有限输出，温度必须为有限正数。

兼容性优先：读取 bundle 的温度配置，按参考实现选择问题类型和选项数量对应的参数。
参考 `laya_config.json` 类型包含 `temperature` 和 `temperature_by_options`，
不能为了统一默认值而覆盖已经存在的校准参数。

原方案中的 `temperature = 1.0` 仅适合作为明确选择的未额外校准基线。
若需要额外的业务校准，应记录参数来源并验证与兼容模式的差异。
最终温度选择行为属于模型对照验收的一部分。

概率不等于对单次结果的保证；危险操作不能仅凭一个未经业务验证的概率阈值执行。
这类用途应结合业务规则，并按需要转交人工或更大模型。

## 8. CPU 并发与共享资源

以 16 核 / 32 GB Linux CPU 服务器为初始调优场景：

| 参数 | 初始值 |
| --- | --- |
| ORT intra-op threads | 8 |
| ORT inter-op threads | 1 |
| 最大推理并发 | 2 |

这是 benchmark 起点，不是对所有硬件通用的最优配置。

使用 Tokio Semaphore 限制实际推理并发，推理移入阻塞任务。
permit 必须覆盖真实 CPU 工作的整个生命周期：即使 HTTP 请求超时或客户端断开，
仍在运行的阻塞推理也不能提前释放名额。对等待队列提供容量与超时限制。

实施前确认所选 `ort` 版本对 Session 的可变借用和并发执行约束。
共享模型不等于可以任意并发调用同一个 Session；
若采用多个 Session 或执行槽，需要测量其额外 RSS 和实际并发收益。
不能把串行 Session 外的 semaphore 大小直接当作真实推理并发。

## 9. 可观测性与生命周期

Prometheus 指标：

| 名称 | 类型与含义 |
| --- | --- |
| `laya_requests_total` | counter，System One HTTP 请求数 |
| `laya_request_duration_seconds` | histogram，请求总耗时，含排队 |
| `laya_inference_duration_seconds` | histogram，实际推理耗时，不含排队 |
| `laya_queue_size` | gauge，等待推理的请求数 |
| `laya_inference_inflight` | gauge，正在执行的推理数 |
| `laya_errors_total` | counter，请求或推理错误数 |

指标标签只使用有界类别，不将状态文本、问题正文或任意客户端输入放入标签。
队列和 inflight 在成功、错误、超时与取消路径均应正确回落。

模型未加载或加载失败时不能返回就绪成功；如果服务此时仍监听，
`/readyz` 返回 503。存活与就绪分别表达进程状态和推理可用性。
退出时停止接受新推理，处理或明确终止等待请求，按约定处理在途工作。

## 10. 启动与部署目标

以下命令是目标接口，当前尚不可执行：

```sh
./laya-server \
  --model ./models/multilingual \
  --listen 0.0.0.0:8080 \
  --threads 8 \
  --max-concurrency 2
```

优先完成命令行配置。YAML 是后续按需扩展，不与命令行并行实现两套配置来源。
线程数和并发数必须大于零，模型路径及监听参数错误应在启动时明确报告。

使用多阶段 Docker 构建，最终镜像包含服务二进制、所需原生运行库及证书等基础文件。
模型和 tokenizer 通过只读目录挂载；不在镜像中安装 Python、Node.js、PyTorch
或 GPU 运行时。示例部署目标：

```sh
docker run --cpus=8 --memory=8g -p 8080:8080 \
  -v ./models:/models:ro laya-rs \
  --model /models/multilingual --listen 0.0.0.0:8080 \
  --threads 4 --max-concurrency 2
```

需要 HTTPS、访问控制或多实例时，可在前面部署 Nginx / Envoy。
每个服务实例独立测量 CPU 与内存，不假设跨进程共享模型内存。

## 11. 实施顺序与验收

### 阶段一：锁定兼容性依据

- 固定参考源码 revision、模型 bundle revision、文件校验值及许可证。
- 确认 ONNX 输入输出名称、dtype、shape、动态维度与 CPU 支持。
- 固定可编译的依赖组合及对应 ONNX Runtime 版本。
- 准备中文、英文及三种问题类型的参考样例、token IDs 和期望输出。
- 明确 API 完整字段、限制、错误语义和数值比较容差。

验收：兼容性依据可复现；Rust 不需要通过运行 Python / Node.js 才能启动或推理。

### 阶段二：最小真实推理链

- 建立 Cargo 工程、类型、Sequence Builder、Tokenizer 和 CPU Session。
- 加载 bundle 配置并实现校准与 Choice / Score / Noul 后处理。
- 对照参考样例验证 JSON 序列化、marker、截断、token IDs 和最终答案。
- 覆盖长状态、长选项、多问题、中文 Unicode 与无效输入。

验收：真实模型得到有效输出，关键序列与参考一致，数值误差满足事先确定的容差；
没有权重时明确标记模型测试未执行，不能用模拟结果宣称兼容。

### 阶段三：HTTP 服务与资源控制

- 实现四个路由、请求校验、统一错误、共享执行资源、并发与排队限制。
- 增加 Tracing、Prometheus 指标、就绪状态和退出处理。
- 验证并发上限、超时和客户端取消时的 permit / gauge 生命周期。

验收：多客户端可共享服务；资源有界；健康检查和指标反映真实状态。

### 阶段四：Linux 部署与 benchmark

- 完成 Dockerfile、模型准备说明、构建与启动文档。
- 检查运行镜像的实际依赖，确认不需要 Python、Node.js、PyTorch 或 GPU。
- 在并发 1、2、4、8 下记录 P50 / P95 / P99、QPS、CPU、RSS 与队列长度。
- 固定模型、CPU 型号、线程配置、请求内容和 token 数，区分冷启动与预热后结果。

验收：Linux CPU 部署可复现；给出实测线程与并发建议，不预设延迟或吞吐承诺。
