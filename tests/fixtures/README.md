# System One 官方参考 fixtures（#10）

`[已验证/HIGH]` `system-one/` 来自固定官方 `RLAgent.system_one` 的 Linux CPU FP32
实跑，共 21 个真实请求。官方结果是期望值，`onnx` 仅为待比较结果。
20 个请求全部通过冻结门槛，`long-padding` 的 Score 出现四位舍入差异：
官方 `4.5307`，最终 #7 bundle 为 `4.5306`。因此 manifest 的 `status` 保持 `failed`，
生成命令退出 1；不能把 fixture 的完整性测试通过解释为完整模型兼容通过。
原始首轮失败保存在 [initial-failure.json](../../docs/validation/fixtures/initial-failure.json)。

来源、平台、完整 Python 包版本、模型/tokenizer/源码 SHA-256 和容器配额见
[provenance.json](system-one/provenance.json)，输入权重沿用
[#7 manifest](../../docs/model-manifest.json)，不进入 Git。
官方 API SHA 为 `c5d78730f3493e4fe16d61507ef4b78eef7318cf`，checkpoint revision 为
`052592a15d198d9ad47da779604259b10b47b7aa`。官方源码未修改，来源许可沿用
[model-prep NOTICE](../../tools/model-prep/NOTICE.md)。

## 覆盖与格式

所有文件均为 UTF-8 严格 JSON，`schema_version=1`。文件 digest 位于
[manifest.json](system-one/manifest.json)，独立的 Rust [消费测试](../fixtures.rs)
验证 SHA-256、必需字段和张量/请求约束，不调用 Python、Node 或模型。

| 文件 | 范围 |
| --- | --- |
| `zh-*` / `en-*` | 中文、英文 Choice/Score/Noul 单问题 |
| `mixed-6` / `mixed-16` | 混合类型、不同候选数/长度、B=6/16，右侧 token/marker padding |
| `options-*` | k=2/3/5/6/10/11/32，真实温度 fallback |
| `long-padding` | 长状态、长指令、长选项、48 token 选项裁切、头部压缩、L=1024；保留失败 |
| `marker` | 从实际 tokenizer 读取 `<mask>` 与所有特殊 token；相邻 MASK，state/指令/选项注入；`[MASK]` 另作普通文字 |
| `normalization` | Choice 重复折叠、空键，Score 重复等级，Noul 默认/单边/空说明，问题顺序 |
| `json-order-numbers` | 嵌套插入顺序/重复键、整数样式键、空格/转义/中文/emoji/surrogate pair、负零、1.0、指数边界、溢出/下溢、越 i64/u64/2^53 整数 |
| `huge-integer` / `scalars` | 4301 位整数；null/bool/空容器/数值种类 |
| `artificial` | 显式人工 logits：三类型温度桶覆盖与 fallback、近并列/精确并列、零概率、32 项均匀概率、0.03125 舍入中点及两侧 |
| `rejected` | 按项目契约拒绝的 20 个输入，包括畸形 JSON、非法 UTF-8、孤立 surrogate、未知/覆盖字段中的非法文本、缺字段、null criteria、折叠后单选项 |

真实样例的 `request_json` 保存完整原始请求文本，必须从该字符串重新解码。
不能先用普通 JSON value 解析后再序列化请求，可能丢掉整数/浮点类别、重复键和顺序。
`state_text`、每行 `instructions_text` / `option_texts` 为官方序列化与规范化文本。
`rows` 按问题顺序，`option_keys` 按候选顺序；Noul 固定 false/true。
`tokenizer_calls` 逐项保存清洗后的真实调用文本及未裁切 token IDs，顺序为
header、各 option（含前导空格）、state；不是 tokenizer decode 的反推文本。

`tensors` 保存实际 forward 的 input_ids、attention_mask、marker_pos、marker_mask、qtype，
`tensor_dtypes` 记录 dtype，shape 由完整矩形数组表示；`rows[].length` 是未 padding 长度。
`logits` 是官方原始主 logits，`act_logits` / `act_probs` 来自官方 action 头，包含 padding
槽的有限哨兵值。行内保存温度桶、取值/来源、scaled logits、未舍入概率及 dtype。
官方主运算为 NumPy float32；Score 的 `np.arange(k) * p` 以 int64/float32 提升为 float64，
最后转 Python float 后 round。`response` 是未修改官方 API 的完整答案及 usage。

`onnx` 保存相同张量输入对应的 logits、act_probs 和完整 response。
生成器从固定官方 API 提取未改写的后处理 AST 给 ONNX 使用，直接接收 act_probs，
不再 softmax；每个官方样例都确认 AST 重放结果与完整官方 API 结果完全相同。
`comparison` 使用契约原始阈值逐值比较；action 按概率容差，其余答案字段精确比较。
人工样例的来源单独标注，不声称来自模型。`rejected` 是项目契约数据，不能由官方
Python 对非法输入的偶然宽松行为产生；Rust `Request::from_slice` 验证预期 status/code。

## 复现与失败保留

复用 [固定 #7 开发环境](../../tools/model-prep/README.md)。在仓库根目录运行，输出目录必须
不存在；同名目录和文件均拒绝覆盖。生成器逐次重新检查 bundle、全部来源和锁定环境。
缺少 API 文件时仅下载固定 revision，并检查硬编码摘要。服务构建/运行不执行此工具。

```sh
rtk proxy docker start laya-model-prep-7
rtk proxy docker exec laya-model-prep-7 python tools/model-prep/fixtures.py \
  --output models/multilingual/prep/fixtures-new-run
rtk cargo test --locked --test fixtures
```

当前固定数据会以退出 1 报告已知差异，但仍保存全部样例、逐样例结果及失败 manifest。
其他异常保留此前已经写出的证据，不生成成功 manifest。不要删除失败后重新挑选样例。
重新生成到新目录，比较 manifest 中每个文件的 digest；`command` 含输出路径，允许不同，
其余 fixture 内容应可比。改动源码、环境或输入后须保存新的 provenance 并人工审查差异，
不能用新的 ONNX 输出覆盖官方期望。

实际命令输出见 [generate.log](../../docs/validation/fixtures/generate.log)、
[repeat.log](../../docs/validation/fixtures/repeat.log)。
两次命令均退出 1（同一 Score 差异），24 个产物文件逐字节摘要全部相同，首轮失败的
输入、张量、原始输出与答案未变，见[复现比较记录](../../docs/validation/fixtures/reproducibility.json)。
容器镜像/配额实查见 [container.log](../../docs/validation/fixtures/container.log)。
再次指定已有目录实际报 `FileExistsError`，24 个文件摘要保持不变。
本任务没有实现 Rust Sequence Builder、后处理或 HTTP，也不证明它们兼容；合成预算参数
的 marker 丢失、room=0/1 等实现测试仍属于后续序列任务。真实 action 概率仍饱和为 `[1,0]`，
非饱和 action 的人工值仅验证后处理字段保真，不证明模型校准质量。

本次 `cargo fmt --check`、`cargo clippy --locked --all-targets -- -D warnings` 通过，
`cargo test --locked` 为 30 passed / 1 ignored；ignored 的既有 Linux 模型 smoke 未在本次重跑。
code-review 从任务起点 `600f0809d6d49d25ba4d2ec4713f88e80ad37bae` 审查当前变更：
Standards 0 项，Spec 0 项需修改发现；Spec 明确保留上述模型兼容失败，#10 保持开放。
