# MVP 兼容契约

版本日期：2026-09-23。适用任务：[冻结契约 #4](https://github.com/redwolf2019/laya-rs/issues/4)。
本文是后续实现和验收的规范；[服务方案](laya-server-plan.md)说明实施顺序。
本文冻结行为；#9 已实现模型无关的类型与输入边界，见
[`src/system_one.rs`](../src/system_one.rs) 及 [`tests/system_one.rs`](../tests/system_one.rs)。
尚未实现 HTTP 服务。#10 已生成[完整官方参考 fixtures](../tests/fixtures/README.md)，
真实 ONNX 对照发现长选项 Score 四位舍入差异（4.5307 / 4.5306），未通过完整链路门槛。

## 1. 固定依据与边界

| 依据 | 固定版本 | 用途 |
| --- | --- | --- |
| [协议研究报告][protocol] | `03cdbac6cf6b4287b9f8cae7ec24bc59ac13a6fb` | 请求、序列、后处理与官方/移植差异 |
| [运行库研究报告][runtime] | `ab20013f6b37a3d3a84368c1cfb464a9fdb619d6` | multilingual 来源、导出与 CPU 运行库接收门槛 |
| [官方 API][api]、[序列及模型][common] | `he-jev/laya@c5d78730f3493e4fe16d61507ef4b78eef7318cf` | 语义 oracle，冲突时优先于移植 |
| [移植类型][types]、[导出器][export] | `receptron/laya@6478649e723122ca24bbf5fb69ed1010023c9750` | 公开字段形式、ONNX 导出参考，不覆盖官方运算行为 |
| [官方 checkpoint][checkpoint] | `convaiinnovations/laya-multilingual@052592a15d198d9ad47da779604259b10b47b7aa` | 默认模型来源，不能用英文模型替换 |
| [CPython JSON][python-json] | CPython `3.11.16` | HTTP 解码后的值、JSON 渲染和 Python round 的参考运行时 |

证据标记：第 2–5 节标为“源码依据”的行为是 `[已验证/HIGH，固定源码范围]`；
HTTP 校验、资源策略和容差是 #4 已定范围及本文明确的项目约定，不冒充上游行为。
运行库和模型接收状态见第 6 节，不能由源码阅读推导推理可用。
遇到与这些规则冲突的新事实，保留失败证据并请求决定，不静默改用 TS 语义、英文模型或放宽阈值。

只提供 `POST /v1/system-one`、`GET /healthz`、`GET /readyz`、`GET /metrics`。
固定参考是进程内 API，没有证实的 Jev HTTP 别名；未验证具体客户端前不声明 Jev 完全兼容。
一请求一次多问题 batch，原子返回全部答案或一个错误，不返回部分答案，不自动重试或跨请求合批。

## 2. HTTP 解码、字段与规范化

### 2.1 解码与顺序

项目 transport 采用 UTF-8 JSON。`Content-Type` 必须为 `application/json`，大小写不敏感；
允许参数，`charset` 如提供只能为 UTF-8。缺失或不匹配返回 415；不添加 `+json` 别名。
严格解码 UTF-8，不接受 BOM、尾随非空白、注释、尾逗号或未转义控制字符。
空白仅为 JSON 的空格、制表、回车和换行。

在这些 transport 约束内，值语义按 CPython 3.11.16 `json.loads`，加上以下项目规则：

| 输入 | 冻结行为 |
| --- | --- |
| 对象顺序 | 每层保持首次插入顺序，数字样式键也不重排；适用于 state、instructions、questions 和 Choice criteria |
| 重复对象键 | 后值覆盖，键仍占首次出现的位置；如 `{"a":1,"b":2,"a":3}` 得到顺序 a、b，a 的值为 3 |
| 整数词法 | 无小数点/指数时为任意精度整数；不经过 f64 或限为 i64/u64；`-0` 渲染为 `0` |
| 浮点词法 | 有小数点或指数时按 Python float（binary64）解析/渲染；`1.0`、`1e0` 渲染为 `1.0`，保留 `-0.0` |
| 大/小指数 | 合法 `1e400` / `-1e400` 解析为正/负 inf，在模型输入文本中渲染 `Infinity` / `-Infinity`；`1e-4000` / `-1e-4000` 按 Python 下溢为 `0.0` / `-0.0` |
| 非标准字面量 | 拒绝未加引号的 `NaN`、`Infinity`、`-Infinity`；字符串 `"Infinity"` 仍是合法字符串 |
| 整数位数 | 参考工具设置 `sys.set_int_max_str_digits(0)`；不继承 Python 的额外十进制位数限制，仍受 body 与深度上限约束 |
| Unicode | 接受合法 surrogate pair 并合成对应码点；拒绝孤立 surrogate（键和值均检查）；不自行做 Unicode 归一化 |
| 深度 | 完整请求的最外层对象深度为 1；每进入对象/数组加 1，标量不增加；默认最多 64 层 |
| 未知字段 | 顶层、问题定义、Noul criteria 中未知成员忽略；Choice 的全部成员都是选项，state/instructions 的全部内容都是数据 |

语法、字节、深度和 surrogate 检查覆盖整个原始文档，包括未知字段及被重复键覆盖的值；
不能先丢弃字段绕过限制。字段类型及问题/选项数量检查作用于解码、重复键覆盖后的有效值。
合法 JSON 数字不等于允许非有限温度或模型结果；后两者仍按第 5 节拒绝。
解码/编码依据为 [CPython JSON 入口][python-json]、[decoder][python-decoder]、[encoder][python-encoder]。

### 2.2 请求字段

下表结合[官方规范化和渲染][api]、[公开 criteria 类型][types]及项目严格校验。
所有名称区分大小写；问题名和选项键可以是任意合法 JSON 字符串，包括空字符串和数字样式字符串。

| 字段 | 必需性、类型、默认值 |
| --- | --- |
| 顶层 | 必须是对象 |
| `state` | 必需；任意 JSON 类型，含 null、布尔、数值、数组、对象或字符串；无默认值 |
| `questions` | 必需；问题名到问题对象的映射，默认有效问题数 1–16；按对象顺序生成 batch 行 |
| `questions.*.type` | 必需；字符串 `choice`、`score`、`noul` 之一，无默认值 |
| `questions.*.instructions` | 必需；任意 JSON 类型；空字符串与 null 均有效，缺失无默认值 |
| Choice `criteria` | 必需；字符串数组，或字符串键到字符串/null 的对象；不接受其他元素/值类型或整体 null |
| Score `criteria` | 必需；有序字符串数组；不接受对象、整体 null 或非字符串元素 |
| Noul `criteria` | 可省略，省略等价于 `{}`；提供时必须是对象，已识别 `false`/`true` 值必须为字符串；整体或已识别值为 null 均非法 |

缺字段与显式 null 不等价。未知字段不能填补缺失的已识别字段。
上游 Python 对某些非法 criteria 的偶然宽松行为不属于 HTTP 接受范围。

### 2.3 选项与文本

| 类型 | 顺序、数量与实际选项文字 |
| --- | --- |
| Choice 数组 | 转有序映射，值为 null；重复字符串折叠，保留第一次出现的位置；如 `["b","a","b"]` 为 b、a；折叠后默认 2–32 项 |
| Choice 对象 | 解码后的键顺序；默认 2–32 项；null 或空描述只渲染键，非空描述为 `key: description` |
| Score | 保留数组顺序及重复描述，默认 2–32 等级；第 i 项渲染 `level i: description`，i 从 0 开始 |
| Noul | 固定 2 项，false、true；文字分别为 `false: <描述>`、`true: <描述>` |
| Noul 默认描述 | 缺失或空字符串的 false 为 `no, the statement does not hold`，true 为 `yes, the statement holds` |

Choice/Score 至少 2 项是模型支持边界：官方 action head 使用 `topk(2)`；
不能用 confidence helper 对 k=1 的定义证明单选项模型可用。

字符串 state/instructions 原样作为文本，不加 JSON 引号。
非字符串 state 使用 `json.dumps(value, ensure_ascii=False)`；
非字符串 instructions 使用默认 `json.dumps(value)`（`ensure_ascii=True`）。
两者均保持对象顺序，分隔符为 `", "`、`": "`，不缩进、不排序、不压缩。
浮点采用 Python 的最短往返表示及指数格式，不能原样复用数值词法或套用 JS/Rust 默认显示。

例如输入对象 `{"10":1.0,"2":-0.0,"大":"😄","n":1e-7,"big":9007199254740993}`：

```text
state:        {"10": 1.0, "2": -0.0, "大": "😄", "n": 1e-07, "big": 9007199254740993}
instructions: {"10": 1.0, "2": -0.0, "\u5927": "\ud83d\ude04", "n": 1e-07, "big": 9007199254740993}
```

这是语言序列化示例，不是模型输出。精确空格、转义与顺序均进入 token IDs 验收。

## 3. 序列与 batch

源码依据：[build_sequence / render_options][common]。读取实际 tokenizer 的 CLS、SEP、MASK、PAD
ID 和 mask token 字符串；不得从 encoder 配置或 TS 硬编码拼写推导 ID。
固定 checkpoint 的任务预算为 `max_len=1024`、`head_max_len=256`，来源见[任务配置][model-config]；
未来接收的 bundle 必须验证这些值。encoder/tokenizer 的 8192 或训练参数 `max_prefixes=6`
不能替代 HTTP 或任务预算。

每个问题按以下次序构造，所有 tokenizer 调用均为 `add_special_tokens=False`：

1. 将规范化 instructions、各选项文字、序列化 state 中精确匹配的实际 mask token 字符串
   全部替换为一个空格；区分大小写，不额外清洗 CLS/SEP/PAD 或其他 special token。
2. 编码 `<type> question: <instructions>` 得到 header。
3. 每个选项编码一个前导空格加选项文字，取前 48 个 token，再前置 MASK ID。
4. `opt_budget = head_max_len - sum(option_ids.len)`；若 `<16`，令
   `per = max(4, floor((head_max_len - 16) / max(1,k)))`，每项保留前 per 个 ID，
   再算 opt_budget。per 包含 MASK；负数除法使用数学 floor。
5. header 保留前 `max(8,opt_budget)` 个 ID。拼接 CLS、header、SEP、全部 option_ids、SEP，
   记录每个 option_ids 起点为 marker。
6. `room = max(0,max_len-当前长度-1)`；取 state 编码的前 room 个 ID，追加 SEP。
   不使用训练路径的左侧截断或随机选项顺序。
7. 整体保留前 max_len 个 ID，仅保留 `<max_len` 的 markers；marker 数必须等于选项数。
   若丢 marker，整请求返回 400 `invalid_request`，不送模型，不为补末尾 SEP 改写截断结果。

`head_max_len` 是算法预算参数，极端参数下不保证头部严格等于该上限或保留末尾 SEP。
模型使用显式 marker 位置，不能扫描所有 MASK ID 当作选项；其他 special token 是否编码为
MASK 要用真实 tokenizer 验收，不能自行增添清洗规则。

源码依据：[collate_items][common]。一个请求一次 forward；B=有效问题数，L=最长序列长度，
K=最多选项数，按 questions 顺序组行，右侧 padding：

| 张量 | dtype / shape | 取值与补齐 |
| --- | --- | --- |
| `input_ids` | int64 `[B,L]` | 补实际 PAD ID |
| `attention_mask` | int64 `[B,L]` | 序列位置 1，padding 0 |
| `marker_pos` | int64 `[B,K]` | 有效 marker 在 `[0,L)`，补 0 |
| `marker_mask` | bool `[B,K]` | 有效候选 true，补 false |
| `qtype` | int64 `[B]` | Choice=0，Score=1，Noul=2 |

不得以逐问题 forward 静默替代动态 B，也不把多个问题串成一条输入。

Rust 实现为 [`SequenceBuilder::build`](../src/sequence.rs)：返回按行连续的 `Vec<i64>` /
`Vec<bool>` 与 `[B,L]` / `[B,K]` shape，`qtype.len()=B`；不初始化 ORT。
加载器传入已校验的 tokenizer、tokenizer 配置及任务预算。marker 丢失返回类型化
`sequence::Error::MarkerLost`，HTTP 层须映射为上述 400；配置、tokenizer 和分配失败
保持独立错误类别。专项对照范围见[序列验收记录](validation/sequence.md)。

## 4. 成功响应

HTTP 200、JSON 对象，源码依据：[官方结果构造][api]。字段全集如下：

| 路径 / 类型 | 值与语义 |
| --- | --- |
| `model` | 固定字符串 `"rl-agent"`，不改为 `"laya"` 或 bundle 名称 |
| `answers` | 原问题名到答案对象的映射，保留问题顺序 |
| `usage.input_tokens` | 所有行 attention_mask 之和；含 CLS/SEP/MASK；重复 state 每行计入；不计 padding |
| `usage.output_tokens` | 整数 0，无文本生成 |
| Choice | `type:"choice"`、`choice`（选中原键）、`probabilities`（所有原键到概率）、`confidence`、`rl_agent:{act_probability}` |
| Score | `type:"score"`、`score`、`legend`（`"0"` 起的字符串索引到原等级描述）、`probabilities`（相同索引到概率）、`confidence`、`rl_agent:{act_probability}` |
| Noul | `type:"noul"`、`noul`、`rl_agent:{act_probability}`；不添加 probabilities/confidence |

Choice probabilities 保留规范化选项顺序；Score legend/probabilities 按等级顺序。
其余对象字段的 JSON 排版不构成协议，数字无固定尾随零要求；字段不得因示例省略而缺失。
任何问题失败时不返回 answers 或部分 usage。

## 5. 温度与数值

源码依据：[官方 API][api]及 [temp_bucket / confidence_from_probs][common]。

| 规则 | 冻结行为 |
| --- | --- |
| 配置默认 | 缺少 `temperature` 时为 `[1,1,1]`，缺少 `temperature_by_options` 时为 `{}`；显式 null、形状错误或非法值不是“缺少” |
| 类型温度 | 三个值依次为 Choice、Score、Noul；所有配置温度必须有限且 `>0`，启动前校验，包括覆盖表中未被当前请求使用的值 |
| 桶 | k≤2 用 `2`，3–5 用 `3-5`，6–10 用 `6-10`，其余用 `11+`；键为 `<type>:<bucket>` |
| 选择 | 覆盖表存在该键时使用其值，否则回退对应类型温度；不得用 1 覆盖已有配置 |
| 主答案概率 | 每行仅取真实 k 项 logits；`z_i=logits_i/T`，`p_i=exp(z_i-max(z))/sum(exp(z_j-max(z)))` |
| Choice | 取未舍入 p 的最大项，精确并列取最早选项；近并列按实际大小，不人为加 tie 容差 |
| Score | `sum(i*p_i)`，i 从 0 开始，范围 0 到 k−1；不取 argmax、不转百分制 |
| Noul | `p[1]`，即 true 概率 |
| confidence | `1-H(p)/ln(k)`，`H(p)=-sum(p_i*ln(clip(p_i,1e-12,1)))`；不添加额外 clamp 或改用最大概率 |
| 舍入 | Choice/Score probabilities、confidence、Score score、Noul noul 均使用 CPython `round(float(x),4)`；不是 `Math.round(x*1e4)/1e4` |
| action 头 | 官方对 act logits 做 softmax，取第 0 项为 `rl_agent.act_probability`；不受主答案温度影响，不作四位舍入 |

官方主 logits 先转 float32 NumPy 数组再运算，Score 的索引乘法涉及 NumPy dtype 提升。
不能假定全部 f64 运算或数学公式相同即可过对照；参考环境的 NumPy/PyTorch 版本和逐阶段 dtype
由 fixture 产物锁定并记录，误差判断始终使用第 9 节已冻结阈值。
`round(0.03125,4)=0.0312`；中点及两侧要做独立测试。
各概率单独舍入后总和允许不为 1，不做二次归一化改变官方字段。

接收的 ONNX 输出必须为未校准 `logits:float32[B,K]` 和已 softmax 的
`act_probs:float32[B,2]`（[导出 wrapper][export]）；不得再次 softmax act_probs。
检查输出 shape/dtype、所有输出及计算中间值的有限性、概率范围 `[0,1]`；
未舍入概率每行和与 1 的差须满足第 9 节概率容差（参考值取 1），不以重新归一化掩盖错误。
非法模型输出、数值计算失败或不满足输出契约时整请求为 500 `inference_failed`。
配置温度非法在监听前非零退出，不回退或等到首请求才暴露。

## 6. 模型与运行库接收门槛

`[已验证/HIGH，研究报告与配置范围]` [运行库报告][runtime]固定的官方 multilingual checkpoint
没有 ONNX 图。`receptron/laya-onnx@68f27dfe5a27a54fb2b1fefc432f43f972e90868` 只有英文，
不能充当默认模型。社区 `ti3x-m/laya-multilingual-onnx@ab6836981ce0ea5937e605fb76ddac8960bd1e21`
声明固定 batch=1，不能满足本文动态多问题 batch；不通过逐问题执行绕过。
按已授权范围从固定官方 checkpoint 独立离线导出，或接收逐项通过同等验证的 multilingual bundle。

模型准备产物必须包含 `laya.onnx`、真实 external data `laya.onnx.data`、`laya_config.json`、
`tokenizer/tokenizer.json`、`tokenizer/tokenizer_config.json`，保存全部实际 SHA-256、大小、来源
revision、许可证/归属与导出锁定环境。external-data 相对路径必须吻合；不创建空 data 文件。
若改内嵌权重布局，先同步本契约和加载验收。固定导出器原样使用 `external_data=False`，
必须显式适配；Hub 快照还不包含导出器需要的 `rl_common.py`，从固定官方源码取用。
CPU FP32、未量化为验收基线。Python 只用于独立模型准备及参考验证，不进入应用构建/启动/推理。

`[推断/MED，尚未编译]` 报告的候选为 Rust 1.98.1、ort/ort-sys 2.0.0-rc.13、
ORT CPU 1.28.0、tokenizers 0.23.2；实际 Cargo.lock、原生 ABI 和 Linux ARM64 加载必须另行验收。
源码许可与模型许可分别记录；参考报告中的发布方 LFS 哈希不等于本地下载后校验。
在真实 Session 读取第 3/5 节张量名、dtype、shape、动态 B/L/K，并验证 B=1/B>1、混合 K 和长度边界。
真实 tokenizer 决定特殊 token ID；目前没有经过验收的 ID 表或模型 golden。

2026-09-23 #7 补充：独立 Linux ARM64 开发环境已完成固定官方 checkpoint 的 fp32 导出及
CPU ORT 对照；最终文件、动态接口、特殊 token ID 与验证范围见
[manifest](model-manifest.json)、[模型准备记录](../tools/model-prep/README.md)。
上段的“目前”描述 #4 冻结时状态；#7 不替代 Rust 推理、完整响应 golden 或 HTTP 验收。

2026-09-23 #8 补充：Rust 启动加载器内嵌上述 manifest，校验全部文件内容后加载 JSON、
Tokenizer 和独立 CPU Sessions，并逐槽执行固定 `tensor-L2` 探针。目录须保持只读且不可变；
图的精确 SHA-256 同时锁定 #7 已检查的 external-data 引用。任何文件（含配置）变化都须
先更新 manifest 并重新验收。加载成功尚不启动 HTTP；[Linux 记录](validation/model-loader.md)
仅证明固定张量加载和推理，不代表完整响应兼容。

`[已验证/HIGH，所选 ort 源码范围，见运行库报告]` `Session::run` 要求可变借用。
实际并发 2 使用两个独立可运行 Session/执行槽，复用同一磁盘 bundle；
单个 Mutex Session 外的 semaphore=2 不构成并发 2。输出借用结束后才归还槽；
内存共享、RSS 和性能收益必须实测，不先作承诺。

2026-09-23 #13 补充：固定图的 50 处 LayerNorm 在离线导出时展开为中心化方差及倒数乘法，
保持标准 ONNX 算子、FP32、原 epsilon 和权重，修复 long-padding 四位 Score 差异。
当前图摘要在 [manifest](model-manifest.json)，原图摘要及原始失败证据保留。
21 个固定请求已通过 Linux ARM64 Rust 序列、真实 ORT、后处理的完整对照，
见[后处理验收](validation/postprocess.md)。本契约的精确字段要求及数值容差没有变更。

## 7. 资源、错误与生命周期

以下是项目策略，不是性能测量。模型截断不替代 HTTP 资源校验。

### 7.1 CLI 与限制

CLI 是唯一运行配置入口，不实现第二套 YAML。`--model` 必须提供本地 bundle 目录；
`--listen` 默认 `0.0.0.0:8080`，必须可解析为 IP 和非零端口。
`--ort-library` 指定本地 ORT CPU 动态库文件，默认 `libonnxruntime.so`；建议使用绝对路径。
这是 #8 加载器新增的 CLI 路径参数；不从环境变量选择运行库，不在启动时下载。

| CLI 参数 | 默认 | 有效性 / 计量边界 |
| --- | ---: | --- |
| `--max-body-bytes` | 1048576（1 MiB） | 正整数字节；包括空白和未知字段，按实际读取的完整 body 检查，不信任 Content-Length |
| `--max-questions` | 16 | 正整数；有效问题数为 1 到该值 |
| `--max-options` | 32 | 整数且 ≥2；Choice/Score 规范化后为 2 到该值，Noul 固定 2 |
| `--max-json-depth` | 64 | 正整数；完整请求根对象计为第 1 层，解析时执行 |
| `--queue-capacity` | 32 | 非负整数；只计等待项，不含在途，0 表示无等待队列 |
| `--queue-timeout` | 30 | 正整数秒；从成功入队到获得执行槽 |
| `--inference-timeout` | 120 | 正整数秒；获得执行槽起，到完整推理结果可返回的 HTTP 等待上限，不含排队 |
| `--shutdown-grace` | 120 | 正整数秒；从开始退出到整个进程必须结束的上限 |
| `--threads` | 8 | 正整数，ORT 每 Session intra-op threads |
| `--inter-op-threads` | 1 | 正整数，ORT 每 Session inter-op threads |
| `--max-concurrency` | 2 | 正整数，实际可同时运行的执行槽数量 |

所有整数必须能无损转换为所用平台/原生 API 的参数；乘加、张量分配和计时换算须检查溢出。
拒绝负值、非整数、超出表示范围及与实际 bundle 支持范围冲突的配置；不得截断或饱和转换。
允许显式减小部署限制用于测试；调高配置不能跳过模型/资源验收，提高默认上限须另有资源证据。
模型预算读取固定 bundle，不能用 CLI 的问题/选项上限修改模型配置。

### 7.2 错误表与校验顺序

所有表内错误返回 JSON `{"error":{"code":"...","message":"..."}}`，
不附 stack、内部路径、输入正文、任意问题名或原始异常。下表 message 为固定文本，
实现如需字段定位只可附静态字段路径（如 `questions.*.criteria`），不能插入用户键值。

| HTTP | code | 条件 | message |
| --- | --- | --- | --- |
| 400 | `invalid_request` | 非法 JSON/Unicode、字段或 criteria 类型、未知 type、问题/选项/深度越界、marker 丢失 | `Invalid request` |
| 413 | `payload_too_large` | body 字节超限 | `Request body too large` |
| 415 | `unsupported_media_type` | 媒体类型缺失或不符 | `Unsupported media type` |
| 429 | `queue_full` | 没有空闲槽，等待容量已满 | `Inference queue full` |
| 503 | `queue_timeout` | 入队后未在队列时限内获得槽 | `Inference queue timeout` |
| 503 | `unavailable` | 未就绪或已开始退出 | `Service unavailable` |
| 504 | `inference_timeout` | 获得槽后 HTTP 执行等待超时 | `Inference wait timeout` |
| 500 | `inference_failed` | 模型执行、tokenizer 内部故障、张量或数值输出失败 | `Inference failed` |

校验次序：路由进入时检查就绪/退出 → 媒体类型 → 有界读取 body → JSON/深度/Unicode →
必需字段及各问题（questions 顺序）类型、规范化数量 → 再检查准入状态 → 获取槽或入队。
前面阶段失败即返回，不继续消费完整 body 或调用模型；读取中一旦超字节上限即 413。
序列构造在获得槽后、模型调用前完成；marker 不足仍为输入拒绝。
语法合法但单候选属于模型支持边界，超问题/选项/深度属于资源拒绝，
三者都映射 400，但测试须分别标记，不能笼统称为非法 JSON。

### 7.3 准入、取消与退出

队列按成功入队顺序先入先出；有等待者时新请求不能抢走其执行槽。
取得槽前取消或 queue timeout 的请求移出队列，不启动推理。
重复 HTTP 请求是独立请求，各占一次额度，不去重、不复用答案、不自动重试。

执行槽/permit 由实际阻塞工作持有，覆盖序列构造、Session 执行及所需输出处理，
直到工作结束和资源可复用。HTTP 504 或检测到客户端断开只结束等待；
已经开始的 CPU 工作继续占槽，结果由服务接收并释放，不能丢任务句柄。
不要把 Tokio 任务取消或超时当作底层推理停止。

同一请求只决出一个 HTTP 终态：状态切换时已观察到退出则拒绝尚未执行的请求；
队列/执行期限已到则分别超时，不再发放槽或返回晚到成功。已经提交的终态不可改写。
开始退出后，在途请求可在各自执行等待期限及 grace 内完成；排队请求全部结束为 unavailable。

启动先校验配置、文件/哈希、tokenizer、原生库和全部执行槽，再开始监听；任一失败非零退出。
监听期间推理资源失去可用性时 `/readyz` 为 503，不能伪造成功。
`GET /healthz` 存活返回 200 `{"status":"ok"}`；
`GET /readyz` 可用且允许准入时返回 200 `{"status":"ready"}`，否则使用 503 unavailable envelope。
队列满仅表示忙碌，不把 readyz 改为未就绪。

收到退出信号即停止推理准入，readyz 转为 503，等待项清空为 unavailable。
在途真实工作最多等待 shutdown grace；全部结束后正常退出。
grace 到期记录未完成工作数量，以非零码退出整个进程，不声称已取消线程；
不能只停止 HTTP 后留下后台线程继续运行。

## 8. 指标与日志

所有时间使用单调时钟。以下计数只针对匹配 `POST /v1/system-one` 的请求，
包括解码、资源与准入失败；其他路由不加入这些请求/错误计数。

| 名称 | 类型、观测边界 |
| --- | --- |
| `laya_requests_total` | counter；进入路由一次加 1，多问题仍只加 1 |
| `laya_request_duration_seconds` | histogram；路由进入到响应确定或观察到断开，一请求一次；含读取/解码/排队/执行等待，不含超时后的后台工作和网络送达时间 |
| `laya_inference_duration_seconds` | histogram；每次实际 Session run 开始到结束（成功或错误）一次，不含排队、分词或后处理；HTTP 超时/断开后仍记录真实结束 |
| `laya_queue_size` | gauge；成功入队加 1，获槽/超时/取消/退出移出时减 1；不含已占槽请求 |
| `laya_inference_inflight` | gauge；取得执行槽加 1，实际阻塞工作结束并归还槽时减 1；包含该槽中的序列/输出处理，HTTP 等待结束不提前减 |
| `laya_errors_total{reason}` | counter；每个失败请求恰好一次，reason 为错误表八个 code 或 `client_cancelled` |

除 errors 的固定 reason 外，不加问题名、任意路径、模型输入等动态标签。
首个终态确定计数归属：HTTP 失败按其 code，检测到断开且此前未确定响应则为 client_cancelled；
成功响应确定后发生的传输断开不追记推理失败。晚到后台失败仅记结构化日志及真实 run 耗时，
不再次增加同一请求的 errors 或 request duration。500 模型失败只计一次，不同时计“HTTP 失败”和“模型失败”。
排队取消应立即回落 queue；在途取消保持 inflight 到真实工作结束。
强制进程退出前记录剩余数量，不把尚未结束的工作人为记成 gauge=0。

Tracing 记录静态结果类别、耗时和资源数量；默认不记录 state、instructions、问题名或候选正文。
HTTP 响应和日志不输出密钥、凭证、原始异常中的敏感内容。

## 9. 验收阈值与场景矩阵

以下是预先冻结的验收门槛，不是已经通过的实测结果。
参考工具使用第 1 节官方 revision 与 checkpoint、CPython 3.11.16；锁定其余依赖版本，
记录生成命令、源码/模型/tokenizer SHA-256、CPU provider、平台、CPU/内存配额及逐阶段数据。
不得用 TS 假 tokenizer 或实现自身输出生成“期望值”。

| 比较项 | 通过条件 |
| --- | --- |
| 规范化文本/顺序、token IDs、markers、mask、qtype、shape、usage | 逐项完全相同；文本按 UTF-8 字节比较 |
| 原始主 logits（真实候选项） | 每项 `abs(actual-reference) <= 1e-4 + 1e-3*abs(reference)` |
| 未舍入主概率、act_probability / act_probs | 每项 `abs(actual-reference) <= 1e-5 + 1e-4*abs(reference)` |
| 官方四位舍入字段、离散 Choice、其他响应字段 | 字段和值精确一致（JSON 空白和无意义尾随零除外），不能仅满足未舍入容差 |
| 非有限结果、错误路径 | 必须按第 5/7 节拒绝；NaN 不得因比较为 false 漏过 |

logits 的 padding 槽只验 shape、mask 与有限性，不作为真实候选参与后处理；
官方/ORT 的有效槽仍必须逐项比较。近并列导致 Choice 不同或跨舍入边界时，即便 logits 过容差也失败。
定位失败阶段，保留数据；禁止自动放宽阈值、改期望或以部分答案通过。

| 场景 | 必须覆盖的输入/边界 | 验证接口与预期 |
| --- | --- | --- |
| 中文、英文三类型 | 每类型单问题；混合多问题 | 固定官方 API 对真实 CPU ONNX/Rust；全响应、usage 与所有中间张量 |
| Choice | 对象 null/空描述，数组，重复项，数字样式键，空键 | 规范化顺序、折叠后 1 项拒绝/2 项接受；并列取首项 |
| Score / Noul | Score 重复等级；Noul 无 criteria、单边、空描述、自定义、null | 等级不折叠；默认文字、false/true 顺序及严格字段校验 |
| 完整 JSON | state/instructions 各种类型、嵌套对象/数组、空容器、null、bool | Python 两种 dumps 模式，精确空格、Unicode 与键顺序 |
| 数字 | 1/1.0/1e0、-0/-0.0、1e-7、2^53+1、越 i64/u64、超过 4300 位整数、正负溢出/下溢 | CPython 3.11.16 语言探针；不丢整数精度，不用原始数值字面量冒充渲染结果 |
| JSON 拒绝 | 非标准常量、语法错误、孤立 surrogate、非法 UTF-8、重复键覆盖中的非法文本 | HTTP 400；合法 surrogate pair、重复键覆盖/首次位置另有接受样例 |
| 未知字段 | 顶层/问题/Noul criteria 未知成员，已识别类型错误，覆盖后的有效值 | 未知字段不改变语义，但其 body/深度/Unicode 仍受检 |
| marker | instructions/state/选项中的真实 MASK、相邻 MASK、CLS/SEP/PAD、其他 special token | 真实 tokenizer 的清洗文本、IDs、显式 marker；不以 whitespace mock 代替 |
| 截断 | 选项正文 48 前后、opt_budget 16 前后、per 下限 4、header 下限 8、state room=0/1 | 官方 builder 对照；合成预算边界测试与实际 bundle 测试分别标记 |
| 总长度 | 等于/超过 max_len、最后可用 marker、marker 丢失、末尾 SEP 被裁 | 原序列前缀和 400 路径，不能补 token |
| batch / padding | B=1/2/16，各行不同 L/K；同请求/拆开请求 | 单 forward 行映射、动态维度、padding/usage；两种 batch 形式各自与同形官方结果比较 |
| 温度 | 三类型，k=2/3/5/6/10/11/32，覆盖/缺桶/缺表/缺类型数组 | 选桶和回退；0、负、NaN、Infinity、null、错形启动失败 |
| 数值 | 等 logits、近并列、极大/极小、非有限、零概率、均匀概率 | 稳定 softmax、熵、Score 期望、tie、有限性检查 |
| 舍入/action | 0.03125 及两侧、概率舍入和不为 1、act_probs 多于四位 | 官方 round 精确值，不重复 softmax action、不额外归一化 |
| HTTP/资源 | 四路由、八错误；body 上限与+1、深度 64/65、问题 0/1/16/17、选项 1/2/32/33 | 状态/code/envelope；Choice 按折叠后计数；未知字段、分块 body 不能绕过限制 |
| 队列与取消 | 小配置下 FIFO、满队列、queue timeout、排队取消、在途取消/504、重复请求 | 未获槽不执行；真实结束前不超并发；slot/queue/inflight 生命周期与一次错误计数 |
| 晚到与竞态 | 超时/断开后成功或失败、期限与获槽/结果同时可见 | 一个终态、一次请求/错误计数；后台结果被接收，实际 run 耗时仍记录 |
| 启动/退出 | 文件/哈希/原生库/tokenizer/Session 失败；退出时排队及在途、grace 到期 | 监听前失败；readyz 503；等待项 unavailable；到期整个进程非零退出并记录数量 |
| Linux CPU | Docker Desktop Linux ARM64、FP32、动态 B/L/K、真实 bundle | 依赖和数值实测；记录容器配额/虚拟化，不外推 amd64 或裸机性能 |

本票完成依据为固定源码/两份报告逐项核对、Markdown 链接与 diff 检查。
后续语言/纯逻辑测试只证明对应边界；真实 tokenizer、模型对照、Linux 加载与性能验收分别记录，
缺权重或未运行时标为“未执行”，不能凭 mock、静态检查或文档冻结宣称通过。

[protocol]: https://github.com/redwolf2019/laya-rs/blob/03cdbac6cf6b4287b9f8cae7ec24bc59ac13a6fb/docs/research/mvp-protocol.md
[runtime]: https://github.com/redwolf2019/laya-rs/blob/ab20013f6b37a3d3a84368c1cfb464a9fdb619d6/docs/research/mvp-runtime.md
[api]: https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_agent_api.py
[common]: https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_common.py
[types]: https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/src/types.ts
[export]: https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/export/export_onnx.py
[checkpoint]: https://huggingface.co/convaiinnovations/laya-multilingual/tree/052592a15d198d9ad47da779604259b10b47b7aa
[model-config]: https://huggingface.co/convaiinnovations/laya-multilingual/blob/052592a15d198d9ad47da779604259b10b47b7aa/rl_agent_config.json
[python-json]: https://github.com/python/cpython/blob/41388c9cb160d0886d5ca00d2e6c8782608a4549/Lib/json/__init__.py
[python-decoder]: https://github.com/python/cpython/blob/41388c9cb160d0886d5ca00d2e6c8782608a4549/Lib/json/decoder.py
[python-encoder]: https://github.com/python/cpython/blob/41388c9cb160d0886d5ca00d2e6c8782608a4549/Lib/json/encoder.py
