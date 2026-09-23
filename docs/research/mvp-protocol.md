# System One 协议、序列与后处理参考基线

研究日期：2026-09-23。对应 [固定 System One 协议、序列与后处理的参考基线](https://github.com/redwolf2019/laya-rs/issues/2)。

本报告固定可引用的源码、列出实际差异并给出 fixture 范围。用户已确认：以官方 Python 固定版本为语义依据，
保留完整 JSON 输入能力，另设序列化对照任务；TypeScript 移植与 ONNX 导出器用于交叉核对。
用户也允许独立的一次性离线导出和开发参考工具，服务 build/start/inference 仍不依赖 Python 或 Node.js。
这里的“官方”指项目方案指定的 `he-jev/laya`，不把其“Jev-compatible”自述当作 Jev 客户端实测。

## 证据与版本

| 资料 | 固定 revision | 许可证据 |
| --- | --- | --- |
| `he-jev/laya` | `c5d78730f3493e4fe16d61507ef4b78eef7318cf` | [README 元数据](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/README.md#L1-L6) 声明 Apache-2.0；该 revision 的树未提供独立 LICENSE。报告不据此补造版权归属或 NOTICE。 |
| `receptron/laya` | `6478649e723122ca24bbf5fb69ed1010023c9750` | [LICENSE](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/LICENSE#L1-L21) 为 MIT，版权 Receptron 2026；其 [README](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/README.md#L118-L120) 另称权重 Apache-2.0。源码与模型许可应分别记录。 |

来源通过 GitHub API 固定 SHA 后，读取该 SHA 的 raw 文件；没有下载模型权重。
以下 `[源码已验证，HIGH]` 只表示直接阅读了对应代码，不表示该代码已经实际推理通过。
本轮额外运行了 Python 3.11.16 / Node v24.16.0 的纯序列化和舍入探针，结果见差异表。
没有运行 PyTorch、Tokenizer、ONNX Runtime、真实模型或 Linux 性能测试；没有生成 token IDs、golden 模型答案或数值容差。

## 请求及响应

`[源码已验证，HIGH]` 两个入口分别为 `RLAgent.system_one(state, questions)` 与
`Laya.systemOne(state, questions)`；都是进程内 API。
本次检查的两个固定仓库没有建立 HTTP 请求路径、认证头或错误 envelope 契约。
项目的 `POST /v1/system-one`、`GET /healthz`、`GET /readyz`、`GET /metrics` 来自
[服务方案](../laya-server-plan.md#5-http-api)，不能由本报告推导任何 Jev URL 别名。
见 [Python 入口](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_agent_api.py#L31-L78)、
[TS 入口](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/src/laya.ts#L62-L152)。

`[源码已验证，HIGH]` `questions` 是问题名到问题定义的映射；顺序决定 batch 行顺序。
每个问题必须提供 `type` 和 `instructions`。TS 的公开类型将 instructions 写为
`string | object`，Python 则对任何非字符串值调用 `json.dumps`，没有同等运行时校验。
因此 Python 的宽松接受行为不能直接变成 HTTP 类型承诺。
见 [请求类型](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/src/types.ts#L1-L25)、
[Python 规范化](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_agent_api.py#L31-L38)。

| 类型 | criteria 形式 | 规范化与选项文字 |
| --- | --- | --- |
| Choice | 字符串数组，或选项键到字符串 / null 的对象 | 数组转有序映射，值为 null；重复数组元素折叠。值为 null 或空字符串时只渲染键；非空值渲染 `key: description`。 |
| Score | 有序字符串数组 | 每项渲染 `level i: description`，i 从 0 开始。数组次序就是等级次序。 |
| Noul | 可省略；对象可含字符串 `false`、`true` | 始终 false 在前、true 在后。缺失或空字符串的 false 描述为 `no, the statement does not hold`，true 描述为 `yes, the statement holds`；分别加 `false: `、`true: ` 前缀。 |

上述文字及空值处理见 [官方 render_options](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_common.py#L35-L44)
和 [移植 renderOptions](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/src/sequence.ts#L18-L35)。
没有缺失 instructions 的默认值；没有缺失 Choice / Score criteria 的有效默认值。

`[源码已验证，HIGH]` 完整答案字段如下，字段不应因示例省略而删掉。
`answers` 使用原问题名，Choice probabilities 使用原选项键，Score 的 legend / probabilities 使用字符串索引。

| 类型 | 完整字段与含义 |
| --- | --- |
| Choice | `type: "choice"`，`choice`，`probabilities: {key: number}`，`confidence`，`rl_agent: {act_probability}` |
| Score | `type: "score"`，`score`，`legend: {"0": 原等级文字, ...}`，`probabilities: {"0": number, ...}`，`confidence`，`rl_agent: {act_probability}` |
| Noul | `type: "noul"`，`noul`，`rl_agent: {act_probability}`；没有附加 probabilities / confidence |
| 顶层 | `model`，`answers`，`usage: {input_tokens, output_tokens: 0}` |

见 [完整类型](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/src/types.ts#L27-L61)
和 [官方结果构造](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_agent_api.py#L66-L78)。
官方 `model` 固定为 `"rl-agent"`，移植固定为 `"laya"`，都不是自动读取 multilingual 模型名。

## 序列、预算与输入张量

`[源码已验证，HIGH]` 下列顺序来自
[官方 build_sequence](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_common.py#L29-L77)
和 [TS buildSequence](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/src/sequence.ts#L103-L135)。

1. 字符串 state 直接使用；其他 state 在 Python 中为 `json.dumps(state, ensure_ascii=False)`，分隔符是 `", "` 与 `": "`，对象保持解析后的插入顺序。不能排序键或改为紧凑 JSON。
2. 字符串 instructions 直接使用；其他 instructions 在 Python 中为默认 `json.dumps`，也带分隔空格，但 `ensure_ascii=True`。这与 state 的 Unicode 策略不同。
3. 从 tokenizer 获取 CLS / SEP / MASK / PAD token ID；官方同时读取实际 `mask_token` 字符串。TS 加载器则硬编码四个 token 的拼写后查询 ID。不能在 Rust 中照抄未核实的数字 ID，所选 tokenizer 必须单独核验。
4. instructions、各选项文字、序列化后的 state 中，精确匹配的 mask token 字符串替换成一个空格；不做大小写折叠。源码没有清洗 CLS / SEP / PAD 字面量，也没有承诺其他 token 拼写不可能编码为 MASK；这些情况需要真实 tokenizer 探针。
5. 使用 `add_special_tokens=False` 编码 `"<type> question: <instructions>"`。选项分别编码 `" " + option_text`，只取前 48 个 token，再在前面加一个 MASK ID。
6. 令 `opt_budget = head_max_len - sum(option_ids.len)`。若小于 16，令 `per = max(4, floor((head_max_len - 16) / max(1, k)))`，每个 option_ids 保留前 per 项，重新计算 opt_budget。per 包含 MASK 位置，不能理解成 option 文字预算。
7. header 保留前 `max(8, opt_budget)` 项。拼成 `CLS + header + SEP + 各 option_ids + SEP`；每个 option_ids 起点记录为 marker。
8. `room = max(0, max_len - 当前长度 - 1)`。state 编码后保留前 room 个 token，再追加 SEP。API 不启用训练/episode 路径的左侧截断或随机 option_order。
9. 最后把整个序列截成前 max_len 项，并保留 `< max_len` 的 markers。API 检查 markers 数是否仍等于渲染出的选项数，否则报错。极端参数下最后 SEP 也可能被裁掉；不能把“总有末尾 SEP”当作无条件保证。

官方 multilingual 配置声明 `max_len=1024`、`head_max_len=256`、三个 temperature 均为 1、
`temperature_by_options={}`；这些是该 checkpoint 的源码配置，不能替代未来 bundle 的真实配置验证。
见 [固定 multilingual 配置](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/multilingual/rl_agent_config.json#L1-L26)。

`[源码已验证，HIGH]` 每个问题一行，问题共享 state 的逻辑值，但各自有完整序列。
B 为问题数，L 为该请求最长序列，K 为该请求最多选项数；右侧 padding，
没有把多个问题拼成一条序列，也没有自动分成多个 forward。
见 [collate_items](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_common.py#L265-L293)
及 [TS 张量构造](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/src/laya.ts#L77-L106)。

| 张量 | dtype / shape | padding |
| --- | --- | --- |
| input_ids | int64 `[B,L]` | 实际 PAD ID |
| attention_mask | int64 `[B,L]` | 真实序列位置为 1，padding 为 0 |
| marker_pos | int64 `[B,K]` | 不存在的候选位置为 0 |
| marker_mask | bool `[B,K]` | 真实选项为 true，补齐项为 false |
| qtype | int64 `[B]` | Choice=0，Score=1，Noul=2 |

`usage.input_tokens` 为全部 attention_mask 的和，即每行未 padding 的长度之和。
CLS / SEP / MASK 计入；同一 state 在多问题间重复计入；padding 不计入。
它不是原始 JSON 字节数，也不是只对 state 做一次分词的长度。
见 [官方 usage](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_agent_api.py#L50-L57)。

## 校准、数值与结果

`[源码已验证，HIGH]` 校准的步骤如下；源码本身没有完整的非法温度/非有限输出防御，
Rust 必须遵守项目方案的有限正温度及有限输出要求，并把新增错误行为写入契约。

- 每行只取真实 k 个 logits，忽略 batch 的补齐选项。使用温度键 `<type>:<bucket>`，bucket 为 k≤2 的 `2`、3–5 的 `3-5`、6–10 的 `6-10`、大于 10 的 `11+`。先按选项数查表，再回退到 qtype 对应的 temperature。官方初始化时缺少 temperature 回退 `[1,1,1]`，缺少 temperature_by_options 回退 `{}`；TS 直接假定配置对象存在，仅对缺失条目及类型温度使用 `??` 回退。
- `z_i = logits_i / T`，`p_i = exp(z_i - max(z)) / sum(exp(z_j - max(z)))`。主答案校准不改变 act_probs。
- Choice 取未舍入 p 的最大项；并列时取最先出现者，不能先对 probabilities 四位舍入再决定。
- Score 是 `sum(i * p_i)`，i 从零开始；不是 argmax，也不缩放为 0–100。
- Noul 为第二项 `p[1]`。
- confidence 是 `1 - H(p)/ln(k)`，官方 `H(p)=-sum(p_i * ln(clip(p_i,1e-12,1)))`；k<2 时函数返回 1，但不能据此声称模型支持独立单选项推理。
- Choice / Score probabilities、confidence、Score 的 score、Noul 的 noul 都保留四位小数。官方使用 Python `round(float(x),4)`，移植使用 `Math.round(x*1e4)/1e4`，二者不同。输出 JSON 数字不承诺固定显示四位，分别舍入后 probabilities 之和也不保证恰为 1。
- `rl_agent.act_probability` 为 act 头第 0 项的 softmax 概率，不做四位舍入。官方 Python 对 act logits 做 softmax；ONNX 导出 wrapper 已经输出 act_probs，Rust 不得再 softmax 一遍。

依据：[温度桶](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_common.py#L358-L361)、
[官方后处理](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_agent_api.py#L25-L26)、
[官方后处理计算](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_agent_api.py#L56-L78)、
[confidence](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_common.py#L212-L218)、
[ONNX wrapper](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/export/export_onnx.py#L31-L38)。

`[源码已验证，HIGH]` 官方将 logits 转成 float32 NumPy 数组后计算；TS 从 Float32Array
读取，再以 JavaScript Number 运算。Python 的 Score 索引乘法还会涉及 NumPy 的 dtype 提升。
所以即使校准公式相同，近并列概率、舍入边界和低概率熵也要固定 fixture，
不能拿“公式一致”替代数值误差验证。报告不指定未经实测的容差。

## 差异与本项目必须确认的契约

| 项目 | 固定源码事实 | 本项目处理依据 |
| --- | --- | --- |
| instructions 对象 | 官方 Python dumps 默认带空格及 ASCII 转义；TS JSON.stringify 紧凑且保留 Unicode | 已确认官方 oracle；state 与 instructions 需要两种 JSON 渲染模式。 |
| JSON 键顺序 | Python 保持插入顺序；TS Object.keys / entries 对整数样式键重排 | 官方模式须保留各层 JSON 顺序，包括 questions / Choice criteria；不能使用会排序键的默认对象存储。 |
| JSON 数字 | Python 区分 int / float 并保留任意精度整数；JS Number 不保留这些区别 | 用户已确认保留完整 JSON，安排独立序列化 parity 验收；不得静默限整数或让调用方改字符串。 |
| model 字段 | 官方 `rl-agent`；TS `laya` | 按已确认官方语义返回 `rl-agent`；如需对外暴露 bundle 身份，另写项目契约，不改此兼容字段。 |
| 舍入 | 官方 Python round；TS Math.round | 采用官方 Python round 语义，使用可精确表示的中点及两侧作为测试。 |
| k=1 | confidence helper 有定义，但模型 `topk(2)`，导出 options 维度声明 min=2 | 推断：全部问题都只有 1 项时不安全；混合 batch 的行为也未验证。推荐 HTTP 拒绝 Choice/Score 少于 2 项，除非专门验证并批准其他行为。 |
| 空问题集 | TS 显式 throw；Python collate 返回 None，调用者仍访问 b | Rust 使用明确 4xx 错误；不要模拟 Python 内部异常。 |
| 其他非法输入 | 缺字段、错类型、空候选、未知 type 多由 KeyError / TypeError / 下游错误暴露，没有一致 envelope | 定义服务器校验顺序、status/code/message、安全日志规则；不继承偶发异常文本。 |
| marker 放不下 | 最终 marker 计数检查；报错文字提 head_max_len，实际裁切按 max_len | 资源限制独立存在；测试实际序列和 marker 保留，不以报错文字推断严格 head 上限。 |
| HTTP 兼容 | 两个固定实现没有 HTTP transport 合约 | 先只实现本项目已定路径。只有具体 Jev SDK/version 和实测路径证据到位才添加别名并声明该客户端兼容。 |

k=1 的依据为 [模型 forward](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_common.py#L100-L124)
及 [导出动态维度](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/export/export_onnx.py#L53-L61)。
错误依据为 [Python 入口](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_agent_api.py#L31-L55)
及 [TS 检查](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/src/laya.ts#L63-L72)。

`[已验证，HIGH；仅语言运行时探针]` 使用同一 JSON 输入：

```json
{"10": 1.0, "2": -0.0, "n": 1e-7, "大": "😄", "big": 9007199254740993}
```

Python 3.11.16 的 `json.dumps(json.loads(text), ensure_ascii=False)`：

```text
{"10": 1.0, "2": -0.0, "n": 1e-07, "大": "😄", "big": 9007199254740993}
```

Node v24.16.0 中，对 `JSON.parse(text)` 执行与固定 TS `pyJsonDumps` 相同的函数：

```text
{"2": 0, "10": 1, "n": 1e-7, "大": "😄", "big": 9007199254740992}
```

Python 默认 dumps 将键 `大` 渲染为 `\u5927`、emoji 渲染为 `\ud83d\ude04`；
JS JSON.stringify 原样保留，并去掉分隔空格。
`round(0.03125,4)` 实测为 `0.0312`，`Math.round(0.03125*1e4)/1e4` 为 `0.0313`。
这些探针没有调用模型；不能由它们推定真实推理误差。

## Golden fixtures 场景与生成来源

本表是待执行的验收范围，不是已经生成的 golden 集合。
参考工具允许只在开发/验收环境运行，服务的 build/start/inference 不调用 Python 或 Node.js。

| 场景 | 必须捕获和比较 | 生成来源 / 目的 |
| --- | --- | --- |
| 中文、英文三种类型各单问题；三种类型混合多问题 | 原请求、normalized instructions/options/state、token IDs、markers、全部输入张量、原始 logits/act_probs、完整答案/usage | 固定 Python build_sequence + collate_items + RLAgent.system_one；所选 bundle 的真实 ORT 另跑一次；验证端到端链路。 |
| Choice 对象含 null、空描述；数组形式；重复数组元素；数字样式键 `10`、`2`；并列最大概率 | 实際 option 顺序、选中键、probabilities 键及取胜规则 | 固定 Python 规范化/后处理；HTTP 对重复元素的拒绝或折叠须先冻结。 |
| Score 2、3、5、6、10、11 等级；Noul 无 criteria、只写一边、两边为空、两边自定义 | option 文字、legend 顺序、bucket、score / noul | 固定 render_options / temp_bucket；验证默认值、索引和桶边界。 |
| 原样字符串 state；嵌套 JSON、数组、null、boolean、空对象；对象 instructions | 序列化字节、解析后的键顺序及 token IDs | 固定 Python json 版本和 tokenizer；区分 state 与 instructions 的 Unicode 策略。 |
| 非 BMP Unicode、换行/制表符、引号、反斜杠、组合字符、孤立 surrogate、重复 JSON 键 | 字节、是否拒绝、token IDs | Python 对照 + 项目输入规范；重复键、孤立 surrogate 等边界由契约明确。 |
| 数字 `1`/`1.0`、`-0.0`、`1e-7`、大/小指数、`2^53+1`、整数边界 | 所有合法 JSON 数字精确按官方规则序列化 | Python json 探针；Rust 数字不能在解析时提前丢精度。 |
| instructions/state/选项各含字面 MASK，重复相邻 MASK；CLS/SEP/PAD 文本；其他 tokenizer special tokens | 清洗后的文字、MASK ID 数量、marker 位置 | 使用实际 tokenizer，验证仅预定 markers 被模型视为选项；不能用 whitespace mock 替代。 |
| 长 instructions、长选项 48 token 前后；总 option budget 16 前后；per 最小 4 前后 | header/option 截断、marker、state room | 固定 build_sequence；token 长度按真实 tokenizer 定义。 |
| state room 为 0、1、边界内外；总长度等于/超过 max_len；marker 正好位于最后边界 | 全序列、末尾 SEP 是否保留、marker 数、错误 | 固定 build_sequence 与 API 计数检查；不要为匹配示意图补加 token。 |
| 多问题各有不同 L/K；单个请求和拆开请求 | 右 padding、marker_mask、qtype、usage；原始输出误差 | 固定 collate 与真实模型；验证 batch 行映射和 attention 统计。 |
| 三种 qtype 的温度覆盖、缺 bucket 回退；T=0/负/NaN/Infinity | 选中温度、合法结果或配置错误 | 固定桶/后处理 + 项目有限正温度规则；非法值不进入模型。 |
| logits 相等、近并列、极大极小、非有限；p 含零、均匀分布、k=1 helper | 未舍入 p、confidence、Choice tie、失败类型 | 模型无关后处理 fixture；k=1 helper 不代表模型可用。 |
| 舍入中点 0.03125 及两侧；probabilities 舍入和不为 1；Score 中间等级；act_probs 非四位数 | 精确答案字段、未重复 softmax 的 act_probability | Python 后处理和手工可算输入；后续固定 NumPy/Python 版本及容差。 |
| 空问题、空选项、缺字段、非法类型、未知 type、超资源限制 | HTTP status、code、message；不得调用推理 | 本项目契约 fixture；上游没有稳定 HTTP 错误 oracle。 |

每个真实 fixture 应附：生成命令、生成环境版本、源码 SHA、bundle revision / 文件 SHA256、
tokenizer 配置、CPU provider、完整请求与输出、误差测量，以及比较策略。
token IDs / markers /张量形状应精确相等；数值容差须在看实现结果前定义，
再用独立参考运行校验，不能为让测试通过回调阈值。
TS 自带 [sequence 测试](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/test/test_sequence.ts#L1-L91)
使用 whitespace 假 tokenizer，只能作为算法场景线索，不能直接充当 multilingual golden。

## 尚未验证的问题及最小后续动作

| 问题 | 最小验证 / 决策 |
| --- | --- |
| 官方与 TS 差异如何进入实现验收？ | 用户已选官方；契约任务登记本表差异，序列化和后处理测试以固定官方代码为准。 |
| 完整 JSON 如何无损解析再按官方规则渲染？重复键/选项、未知字段如何处理？ | 完整 JSON 已确认，独立序列化任务覆盖任意精度整数及浮点渲染。重复键等未定义无效输入由本项目契约显式裁决，禁止借此拒绝全部浮点或大整数。 |
| 具体 Jev 客户端需要什么 URL / headers？ | 用户提供目标 SDK 与版本后读取其固定 transport 并执行客户端请求；此前仅承诺本项目 HTTP 路径。 |
| 所选 multilingual bundle 的真实 token ID、输入输出、动态维度是否符合导出脚本？ | 准备 bundle 的任务读取实际 config/tokenizer/ONNX 元数据，再以 CPU Session 最小运行核验。导出脚本不是发布工件证明。 |
| Python / NumPy / Tokenizer 版本选哪组，数值如何比较？ | 参考 fixtures 任务固定可运行环境，记录版本与误差分布，预先定绝对/相对容差以及近并列 Choice 的判定规则。 |
| 单选项、单等级是否支持？ | 推荐统一拒绝 k<2。若要求支持，分别测独立 batch 与混合 batch，记录 ONNX 真实约束及 act 头语义，不从 helper 推断。 |
| 源码复制归属和权重许可材料够不够？ | 模型准备/许可任务保存发布方实际许可文件及通知；移植 MIT 源码保留其版权通知，官方无独立 LICENSE 的事实单列。 |

报告交付前检查项：固定源码读取、纯语言探针、报告本地链接存在性和 diff 检查。
真实 tokenizer、golden 生成、ONNX 推理、Linux 部署及 Jev 客户端兼容均留待各自执行票验收。
