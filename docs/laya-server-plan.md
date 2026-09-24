# Laya Rust HTTP Server 方案

状态：CLI、固定 bundle 加载、Rust CPU 张量探针、System One 类型、输入规范化与 Python JSON 文本渲染已实现；
Sequence Builder 与五个批处理输入已实现并通过固定 tokenizer 对照；后处理已通过固定 logits 对照，同步 engine 入口及四个 HTTP 路由已贯通。
已通过 #8 固定张量验收；#13 已修复 LayerNorm 数值差异，21 个固定请求的 Rust 真实推理
与完整答案在 Linux ARM64 CPU 通过对照，见[后处理验收](validation/postprocess.md)。
#14 已把该链路接入可复用 engine，失败恢复与运行记录见[engine 验收](validation/engine.md)。HTTP 真实对照见 [HTTP 验收](validation/http.md)。
#15 已接入有界 FIFO 调度、真实阻塞任务持槽和句柄回收，Linux N=1/2 实测见[调度验收](validation/scheduler.md)。
版本日期：2026-09-24。

字段、序列、校准、HTTP 错误、资源限制和验收阈值以
[MVP 兼容契约](compatibility.md)为准；本文记录目标、工程边界与实施顺序。

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
仅包含 Rust。优先使用经核验的现成 ONNX bundle；若没有满足固定参考版本的
multilingual bundle，允许在独立的模型准备环节进行一次性离线导出。
Python 仅可用于该准备环节和开发期参考对照，不进入服务构建、启动或推理流程。

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

原候选 [receptron/laya-onnx](https://huggingface.co/receptron/laya-onnx) 在已核实的
revision `68f27dfe5a27a54fb2b1fefc432f43f972e90868` 只有英文 bundle，
没有 `multilingual` 子目录，不能作为中文 MVP 的模型来源。
官方 multilingual checkpoint 为
[convaiinnovations/laya-multilingual](https://huggingface.co/convaiinnovations/laya-multilingual/tree/052592a15d198d9ad47da779604259b10b47b7aa)。
模型准备任务须核验预导出候选的来源和内容，必要时从固定官方 checkpoint 独立离线导出；
固定最终 bundle revision 或导出环境、文件清单、许可证、实际校验值及 CPU 对照依据。
权重不入 Git。研究与后续验证见
[模型与运行库基线](https://github.com/redwolf2019/laya-rs/issues/3)。

## 4. 技术栈与模块

采用 Rust、Axum 0.8、Tokio、Serde / serde_json、HuggingFace Tokenizers、
ort / ONNX Runtime、Tracing 和 Prometheus 指标。
Tower / tower-http 以及 ndarray 按实际使用需要引入，不为目录完整而添加依赖。

候选组合为 Rust 1.98.1、ort 2.0.0-rc.13、ONNX Runtime CPU 1.28.0、tokenizers 0.23.2，
依据为[固定运行库研究](https://github.com/redwolf2019/laya-rs/blob/ab20013f6b37a3d3a84368c1cfb464a9fdb619d6/docs/research/mvp-runtime.md)。
这些版本已由 #8 在 Linux ARM64 完成构建和固定张量 smoke，见[加载记录](validation/model-loader.md)。
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

`/v1/system-one` 是本项目约定路径，MVP 不新增未经证实的 Jev 路径别名。
一次请求原子返回全部答案或错误；不返回部分答案，不自动重试。
成功响应的 `model` 固定为官方值 `"rl-agent"`。

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
[参考类型定义](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/src/types.ts)
还包含 Choice / Score 的 `confidence`、Score 的 `legend` 和答案的
`rl_agent.act_probability`。完整响应、请求的可选形式和舍入规则，以官方
[he-jev/laya 固定版本](https://github.com/he-jev/laya/tree/c5d78730f3493e4fe16d61507ef4b78eef7318cf)
为语义基线；TypeScript / ONNX 移植用于导出和实现参考，差异不得静默覆盖官方行为。
保留完整 JSON 输入能力，另行验证跨语言数字和对象序列化。
HTTP 路径、资源限制与错误映射属于本项目契约，不能仅凭本地推理 API 宣称 HTTP 兼容。
详细差异见[协议基线研究](https://github.com/redwolf2019/laya-rs/issues/2)，
不将省略字段的示例当作完整协议。

完整字段、默认值、Choice 重复项折叠与 JSON 规则见[兼容契约第 2–5 节](compatibility.md)。
state/instructions 接受任意 JSON 类型；以 CPython 3.11.16 为解码/渲染参考，保留整数精度、
浮点语义和对象顺序。重复对象键以后值覆盖并保留首次位置；未知字段忽略但仍受资源/语法校验。
拒绝孤立 surrogate 与非标准 NaN/Infinity 字面量；合法指数溢出/下溢按 Python 输入文本语义处理。

默认 body 最多 1 MiB、1–16 个问题、规范化后 Choice/Score 2–32 项、Noul 2 项、JSON 深度 64。
这些资源边界独立于 token 截断；可调 CLI 和校验次序见[兼容契约第 7 节](compatibility.md)。
统一错误体为 `{"error":{"code":"...","message":"..."}}`，只用静态文本或静态字段路径提示：

| HTTP | code | 条件 |
| --- | --- | --- |
| 400 | `invalid_request` | JSON/字段/问题非法，数量或深度越界 |
| 413 | `payload_too_large` | body 超限 |
| 415 | `unsupported_media_type` | 媒体类型不符 |
| 429 | `queue_full` | 等待队列已满 |
| 503 | `queue_timeout` | 排队超时 |
| 503 | `unavailable` | 未就绪或退出 |
| 504 | `inference_timeout` | 执行等待超时 |
| 500 | `inference_failed` | 模型或数值失败 |

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
[Laya 官方实现](https://github.com/he-jev/laya/tree/c5d78730f3493e4fe16d61507ef4b78eef7318cf) 与
[ONNX 移植的序列实现](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/src/sequence.ts)。
参考代码只用于独立的模型准备、理解和开发期验证，部署服务不调用 Python 或 Node.js。
如移植代码，遵循来源许可证并保留必要归属。

## 7. 校准和后处理

流程为 `logits → temperature scaling → softmax → 类型化答案`。
softmax 需要采用数值稳定的实现，拒绝非有限输出，温度必须为有限正数。

兼容性优先：读取 bundle 的温度配置，按参考实现选择问题类型和选项数量对应的参数。
参考 `laya_config.json` 类型包含 `temperature` 和 `temperature_by_options`，
不能为了统一默认值而覆盖已经存在的校准参数。

缺少 temperature 时按官方回退 `[1,1,1]`，缺少选项表时回退 `{}`；
桶覆盖优先于类型温度，不覆盖 bundle 已有值。非法温度启动失败。
按官方 Python round 舍入四位；Choice 在未舍入概率上取最大，并列取最早项。
action 头输出不作四位舍入，ONNX act_probs 不重复 softmax。
完整公式与真实对照阈值见[兼容契约第 5、9 节](compatibility.md)，MVP 不引入业务再校准。

概率不等于对单次结果的保证；危险操作不能仅凭一个未经业务验证的概率阈值执行。
这类用途应结合业务规则，并按需要转交人工或更大模型。

## 8. CPU 并发与共享资源

以 16 核 / 32 GB Linux CPU 服务器为初始调优场景：

| 参数 | 初始值 |
| --- | --- |
| ORT intra-op threads | 8 |
| ORT inter-op threads | 1 |
| 最大推理并发 | 2 |

这是保守默认值和 benchmark 起点，不是对所有硬件通用的最优配置。
队列默认容量 32、等待 30 秒；获槽后的 HTTP 执行等待默认 120 秒，退出 grace 默认 120 秒。
CLI 暴露这些限制、输入上限、线程数和实际并发；参数与有效性规则见[兼容契约](compatibility.md)。

使用 Tokio Semaphore 限制实际推理并发，推理移入阻塞任务。
permit 必须覆盖真实 CPU 工作的整个生命周期：即使 HTTP 请求超时或客户端断开，
仍在运行的阻塞推理也不能提前释放名额。对等待队列提供容量与超时限制。

固定候选 ort 的 `Session::run` 要求可变借用；实际并发 2 需要两个独立可运行的执行槽。
槽持有 permit 到阻塞工作及输出处理结束；HTTP 超时或断开后仍接收后台结果并回收资源。
测量多 Session 的额外 RSS 和实际并发收益；串行 Session 外 semaphore=2 不构成真实并发 2。

当前 `scheduler::Client` 复用上述 Session，`Scheduler::run(&mut self)` 由服务所有者持续驱动，
关闭准入后等待全部 JoinSet 结果；取消该驱动 future 不转移或丢弃句柄，须恢复驱动完成回收。
CLI 已把 HTTP 服务与调度器所有者并行驱动；#17 的信号退出流程在 grace 到期时必须终止整个进程。

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
每个 System One HTTP 请求计一次，多问题不重复计数；错误按终态原因恰好计一次，
晚到后台失败不再计同一请求的 errors。请求耗时包含排队，实际 Session run 耗时不含排队；
HTTP 等待结束后，run 耗时仍观测到真实工作结束。完整边界见[兼容契约第 8 节](compatibility.md)。

模型、tokenizer、原生库或 Session 加载失败时，在开始监听前非零退出。
监听期间不可用或退出时 `/readyz` 返回 503；存活检查不代表推理可用。
退出停止准入，排队请求结束为 unavailable，在途真实工作最多等 shutdown grace；
到期记录未完成数量并非零退出整个进程，不声称 CPU 线程已经取消。

## 10. 启动与部署目标

MVP 首轮部署验收与 benchmark 使用本机 Docker Desktop 的 Linux ARM64 容器，
记录实际 CPU、内存配额及虚拟化环境；结果不代表 16 核 / 32 GB 裸机或 Linux amd64。
其他平台的可用性和性能需在对应环境另行验证。

CLI 启动先校验 bundle、加载 CPU 资源并跑探针，成功后才监听 HTTP。
#16 的 metrics 仅编码空 registry；完整指标、结构化请求日志、信号与 grace 验收留在 #17。
构建和当前可执行检查见 [README](../README.md#构建与-cli)：

```sh
./target/debug/laya-server \
  --model ./models/multilingual \
  --ort-library /opt/onnxruntime/lib/libonnxruntime.so \
  --listen 0.0.0.0:8080 \
  --threads 8 \
  --max-concurrency 2
```

命令行配置是唯一入口，MVP 不引入 YAML 第二套配置来源。
线程数和并发数必须大于零，输入/队列/时间参数按[兼容契约第 7 节](compatibility.md)校验；
模型路径及监听参数错误在启动时明确报告，不开始监听。

使用多阶段 Docker 构建，最终镜像包含服务二进制、所需原生运行库及证书等基础文件。
模型和 tokenizer 通过只读目录挂载；不在镜像中安装 Python、Node.js、PyTorch
或 GPU 运行时。以下仅是未来部署目标，当前尚无该镜像：

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

- 固定参考源码 revision、模型 bundle revision 或独立离线导出环境、文件校验值及许可证。
- 确认 ONNX 输入输出名称、dtype、shape、动态维度与 CPU 支持。
- 固定可编译的依赖组合及对应 ONNX Runtime 版本。
- 准备中文、英文及三种问题类型的参考样例、token IDs 和期望输出。
- 实现并核对[兼容契约](compatibility.md)已冻结的 API、限制、错误与数值容差，不重新决定语义。

验收：兼容性依据可复现；Rust 不需要通过运行 Python / Node.js 才能启动或推理。

### 阶段二：最小真实推理链

- 建立 Cargo 工程、类型、Sequence Builder、Tokenizer 和 CPU Session。
- 加载 bundle 配置并实现校准与 Choice / Score / Noul 后处理。
- 对照参考样例验证 JSON 序列化、marker、截断、token IDs 和最终答案。
- 覆盖长状态、长选项、多问题、中文 Unicode 与无效输入。

验收：真实模型得到有效输出，关键序列与参考一致，数值误差满足事先确定的容差；
没有权重时明确标记模型测试未执行，不能用模拟结果宣称兼容。
逐项执行[兼容契约场景矩阵](compatibility.md)：token IDs/marker/mask/usage 精确相等；
logits 使用 `abs <= 1e-4 + 1e-3*abs(reference)`，未舍入概率及 act_probability 使用
`abs <= 1e-5 + 1e-4*abs(reference)`；官方舍入字段和离散 Choice 精确一致，失败不自动放宽。

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
