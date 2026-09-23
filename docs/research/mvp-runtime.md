# MVP 模型资产与 Rust CPU 运行库核实

研究日期：2026-09-23。对应 [核实 multilingual bundle 与 Rust CPU 运行库候选组合](https://github.com/redwolf2019/laya-rs/issues/3)。

本报告核对固定源码、发布元数据和小配置文件，没有下载模型权重、导出模型、编译 Rust 或执行推理。
下文 `[已验证/HIGH]` 只表示对应来源或文件已读取；运行兼容性、RSS、吞吐量没有实测。

## 结论与交接边界

- `[已验证/HIGH]` 方案指定的 `receptron/laya-onnx/multilingual` 不存在于已核实的
  `68f27dfe5a27a54fb2b1fefc432f43f972e90868`。该仓库发布的是根目录 English 421M
  ModernBERT-large bundle；不能用它验收中文默认模型。[固定文件清单](https://huggingface.co/api/models/receptron/laya-onnx/revision/68f27dfe5a27a54fb2b1fefc432f43f972e90868?blobs=true)、[模型卡](https://huggingface.co/receptron/laya-onnx/blob/68f27dfe5a27a54fb2b1fefc432f43f972e90868/README.md)。
- `[已验证/HIGH]` 官方 multilingual 原始 checkpoint 可固定为
  `convaiinnovations/laya-multilingual@052592a15d198d9ad47da779604259b10b47b7aa`，
  但它没有 ONNX 图。用户已允许必要时单独离线导出；应用构建、启动和部署仍不能依赖 Python。[固定文件清单](https://huggingface.co/api/models/convaiinnovations/laya-multilingual/revision/052592a15d198d9ad47da779604259b10b47b7aa?blobs=true)。
- `[推断/MED]` 社区预导出值得先做一次有界接收检查；存在固定 batch=1、输出含义变化及来源移植差异，
  不能直接当作当前多问题批处理的已验证替代品。不满足接收门槛则使用固定官方 checkpoint 离线导出，见下文。
- `[推断/MED]` 建议编译探针从 Rust `1.98.1`、`ort =2.0.0-rc.13`、
  ONNX Runtime CPU `1.28.0`、`tokenizers =0.23.2` 开始。这是已发布、可追溯的候选组合，
  不是已编译组合。完整锁文件、平台原生依赖和真实模型执行必须在阶段一执行票完成。

## 官方 multilingual 输入资产

`[已验证/HIGH]` 模型卡声明 Apache-2.0；Hub 元数据列出 321,908,998 个参数，
权重类型统计为 F16 321,908,995 个、F32 3 个。原始权重文件是 safetensors，不是 CPU ONNX。
资产来源固定到上面的完整 revision，下载路径使用 `resolve/<revision>/<path>`，不能使用 `main`。
[模型卡](https://huggingface.co/convaiinnovations/laya-multilingual/blob/052592a15d198d9ad47da779604259b10b47b7aa/README.md)、[发布元数据](https://huggingface.co/api/models/convaiinnovations/laya-multilingual/revision/052592a15d198d9ad47da779604259b10b47b7aa?blobs=true)。

| 文件 | 字节数 | SHA-256 | 核实范围 |
| --- | ---: | --- | --- |
| `model.safetensors` | 643835514 | `9d628fd971b700382ac6f65920a86f149777b2e748e0c955fb3b19695aa8f204` | 发布方 LFS 元数据，未下载 |
| `tokenizer/tokenizer.json` | 34363188 | `609d8f4c067cd3950f88594c5a802616cea245823836ef5848ee4fc40aab5b6f` | 发布方 LFS 元数据，未下载 |
| `rl_agent_config.json` | 472 | `25061739243b617ad88d1219ba6f8a9c86c5881ca28df024fa2d9b3b2fcc30c6` | 本轮读取实际字节并计算 |
| `encoder/config.json` | 1938 | `83f6916d13ef0f556ac461f28308dc2bffa7ebeadee8ec9e2db5812020ea5bb4` | 本轮读取实际字节并计算 |
| `tokenizer/tokenizer_config.json` | 502 | `424b69444bf7b5809dc2cd2e36d0bd71b8055124dd24274d6db3c655d38205e7` | 本轮读取实际字节并计算 |

许可证文本、模型卡和来源记录也应随模型准备产物保存；权重不进入 Git。

`[已验证/HIGH]` [任务配置](https://huggingface.co/convaiinnovations/laya-multilingual/blob/052592a15d198d9ad47da779604259b10b47b7aa/rl_agent_config.json)为：

| 项 | 固定值 / 含义 |
| --- | --- |
| encoder | `jhu-clsp/mmBERT-base` |
| `max_len` | 1024，完整任务输入序列上限 |
| `head_max_len` | 256，参考构造器的头部预算参数；不等于候选项数 |
| `head_layers` | 2 |
| `temperature` | `[1.0, 1.0, 1.0]` |
| `temperature_by_options` | `{}` |
| `max_prefixes` | 6，训练样本的 conversation prefix 采样参数；不是 HTTP 问题数上限 |
| `amp_dtype` | `bf16`，原训练/推理配置；不表示 CPU ONNX 应选择 BF16 |

`[已验证/HIGH]` encoder 的 `max_position_embeddings` 和 tokenizer 的 `model_max_length`
均为 8192，不能替换任务 `max_len=1024`。
encoder 配置中 `cls_token_id=1`，tokenizer 配置却指定 `cls_token="<bos>"`；必须用实际
`tokenizer.json` 查 token ID，并按官方构造器的 tokenizer 属性验收，不能从 encoder 的 CLS 值硬编码。
[encoder 配置](https://huggingface.co/convaiinnovations/laya-multilingual/blob/052592a15d198d9ad47da779604259b10b47b7aa/encoder/config.json)、[tokenizer 配置](https://huggingface.co/convaiinnovations/laya-multilingual/blob/052592a15d198d9ad47da779604259b10b47b7aa/tokenizer/tokenizer_config.json)。

`[已验证/HIGH]` 官方 `build_sequence` 每个选项先截为至多 48 个正文 tokens，再按头部预算缩短；
`max_prefixes` 不限制 options。模型的 action head 使用 `topk(2)`，不能假定单候选 K=1 的张量能直接运行。
业务候选项数、问题数、请求体、队列容量和等待时限不是模型配置给出的值，必须写入服务契约；
模型测试要验证 K=2、更多选项、混合候选数 padding 和超界输入。
[固定官方实现](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/rl_common.py)。

## 指定的 English bundle 只能用作对照资料

`[已验证/HIGH]` `receptron/laya-onnx` 查询时只有 `main` 分支，没有 tags；固定 revision 文件包括
`.gitattributes`、`README.md` 与以下五个运行文件，没有 `multilingual/`。
[分支查询](https://huggingface.co/api/models/receptron/laya-onnx/refs)、[固定树](https://huggingface.co/api/models/receptron/laya-onnx/revision/68f27dfe5a27a54fb2b1fefc432f43f972e90868?blobs=true)。

| 文件 | 字节数 | 发布方标识 |
| --- | ---: | --- |
| `laya.onnx` | 3807291 | LFS SHA-256 `a874eb254b58b0fcb1e7ad56fbb188c29d64e08c9a46b689433e1f52c66dba1e` |
| `laya.onnx.data` | 1685258240 | LFS SHA-256 `487746363a8da57bcadb4345352997d22a0fb90d70aa22c6856668d023242aba` |
| `laya_config.json` | 369 | Git blob SHA-1 `54fdeafc06abca5d4232a02857238ff53c975c38` |
| `tokenizer/tokenizer.json` | 3583228 | Git blob SHA-1 `2f4d8583e507b7466d2490e2d6c045647a822698` |
| `tokenizer/tokenizer_config.json` | 308 | Git blob SHA-1 `9fd800115c5c92353220aa66addfce67a9135f32` |

这里的 Git blob SHA-1 不是内容 SHA-256；本轮没有计算该 bundle 文件的本地 SHA-256。
它的配置是 `max_len=512`、`head_max_len=192`，温度分别为
`1.6369030475616455, 1.2514300346374512, 1.983399510383606`，并有按选项数的温度表，
不能移植到 multilingual。
[固定配置](https://huggingface.co/receptron/laya-onnx/blob/68f27dfe5a27a54fb2b1fefc432f43f972e90868/laya_config.json)。

## 社区预导出的有界接收检查

`[已验证/HIGH，元数据范围]` 最完整的候选是
`ti3x-m/laya-multilingual-onnx@ab6836981ce0ea5937e605fb76ddac8960bd1e21`：

| 文件 | 字节数 | 发布方 SHA-256 |
| --- | ---: | --- |
| `onnx/model.onnx` | 2925003 | `9582b7070f2aef724f99242ecbe43a1cc46a2d74c11f75518fcaab70e460c495` |
| `onnx/model.onnx_data` | 1287635968 | `71e49271ebbc2785806834dfc4c9b14766bf118bac46e78f483a820d39c2d2b7` |
| `laya_config.json` | 127 | `b1a3dd900e0fd358d1adc325b4234f86b5026bb240ddf7191f7f102f63a1c450` |
| `tokenizer/tokenizer.json` | 34363188 | `609d8f4c067cd3950f88594c5a802616cea245823836ef5848ee4fc40aab5b6f` |
| `tokenizer/tokenizer_config.json` | 502 | `424b69444bf7b5809dc2cd2e36d0bd71b8055124dd24274d6db3c655d38205e7` |

来源：[固定 manifest](https://huggingface.co/ti3x-m/laya-multilingual-onnx/blob/ab6836981ce0ea5937e605fb76ddac8960bd1e21/manifest.json)、[Hub LFS 元数据](https://huggingface.co/api/models/ti3x-m/laya-multilingual-onnx/revision/ab6836981ce0ea5937e605fb76ddac8960bd1e21?blobs=true)。这些值未在本项目下载后重新计算。

其模型卡声明 Apache-2.0，manifest 指向上述官方 checkpoint revision，但源码指针是
`NandhaKishorM/laya@c7527708f9f5220c669d8aa385077cd28d04708a` 的 fork，具体模型代码在
`laya/common.py`。源码差异尚未审计。
标准 FP32 输出名为 `logits`、`act_probs`，配置为 1024/256 和全 1 温度。
README 明确固定 batch=1，动态 sequence/options；manifest 的 `maxQuestionsPerBatch=1`。
发布方 `validation-fp32.json` 自报 CPU 的 logits 最大绝对误差 `7.152557373046875e-05`，
atol/rtol 均 0.001；这是发布方记录，不是本项目测量。README 的复现环境使用 ORT 1.30.0，
所以也不能据此保证本报告候选 1.28.0 可加载。
[模型卡](https://huggingface.co/ti3x-m/laya-multilingual-onnx/blob/ab6836981ce0ea5937e605fb76ddac8960bd1e21/README.md)、[发布方验证记录](https://huggingface.co/ti3x-m/laya-multilingual-onnx/blob/ab6836981ce0ea5937e605fb76ddac8960bd1e21/validation-fp32.json)、[fork 固定树](https://github.com/NandhaKishorM/laya/tree/c7527708f9f5220c669d8aa385077cd28d04708a/laya)。

`[建议/MED]` 接收门槛：审计模型 forward 与固定官方实现的差异；下载所有必要文件并核对 SHA-256；
读取真实 Session 的名称、dtype、shape、动态维度；按项目的多问题批处理契约试 B=1 与 B≥2；
使用固定官方中文/英文、三种问题类型的 token IDs、原始 logits、act probability 和最终答案对照。
如果契约要求单次动态多问题批处理，这个 B=1 候选不满足，应直接走离线导出，不静默改为逐问题运行。
不得仅因名为 `act_probs` 就免除范围、有限性及归一化校验。

其他候选只做了小文件筛查：
`mizchi/laya-multilingual-onnx@d9d003d543e63d6d3375c21d44624136bd1e0bad` 是经 MLX checkpoint
转换的 FP16 图，输出是 `act_logits`；
`soyelmismo/laya-multilingual-onnx@0966c4fa58da6878b39e7e14cb5e93313b82d828` 有 FP32 图，
但所读模型卡未给固定导出源码或完整接口验证依据。本轮没有继续扩展候选搜索。
[mizchi 模型卡](https://huggingface.co/mizchi/laya-multilingual-onnx/blob/d9d003d543e63d6d3375c21d44624136bd1e0bad/README.md)、[soyelmismo 模型卡](https://huggingface.co/soyelmismo/laya-multilingual-onnx/blob/0966c4fa58da6878b39e7e14cb5e93313b82d828/README.md)。

## 独立离线导出的执行输入与验收

固定三个输入：官方 checkpoint `052592a15d198d9ad47da779604259b10b47b7aa`、
官方源码 `he-jev/laya@c5d78730f3493e4fe16d61507ef4b78eef7318cf`、
导出参考 `receptron/laya@6478649e723122ca24bbf5fb69ed1010023c9750`。
官方 `rl_common.py` 的本轮实际字节 SHA-256 为
`8d83611d480c971d640a7b7d3aa2f2219c5e8455e9cc2329fd073681bd8be23e`。

`[已验证/HIGH]` [导出参考脚本](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/export/export_onnx.py)
从 `model_dir/rl_common.py` 导入 `build_model`，而官方 multilingual Hub 快照没有该 Python 文件；
执行票必须从固定官方源码单独取文件，不把 `snapshot_download` 成功当作导出环境齐全。
脚本读取本地 encoder config、严格载入 state dict、关闭 `reference_compile`，
wrapper 将 action logits 做 softmax 后命名为 `act_probs`。
它声明 opset 18、动态 batch/seq/options，其中 seq 最小 8、options 最小 2；这些是导出脚本声明，
不是本报告已从模型图读取的结论。

| 张量 | 参考导出声明 | 必须验证 |
| --- | --- | --- |
| `input_ids` | int64 `[B,L]` | 真实名称、dtype、动态 B/L、token ID 边界 |
| `attention_mask` | int64 `[B,L]` | 与输入同形、padding 行为 |
| `marker_pos` | int64 `[B,K]` | 有效 marker `< L`，padding 索引安全 |
| `marker_mask` | bool `[B,K]` | 有效候选与 padding 区别 |
| `qtype` | int64 `[B]` | choice=0、score=1、noul=2 |
| `logits` | float32 `[B,K]` | 未校准，masked slots 的实际行为 |
| `act_probs` | float32 `[B,2]` | softmax 后概率，不再次当 logits 处理 |

`[已验证/HIGH]` 当前脚本 `prog.save(..., external_data=False)`，但同 revision
[下载器](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/src/download.ts)
硬列 `laya.onnx.data`。执行票必须明确输出合同：优先按现有方案产出
`laya.onnx`、`laya.onnx.data`、`laya_config.json`、两个 tokenizer 文件，并核对图中的 external-data
相对路径。不创建空 `.data` 文件掩盖差异。若改成内嵌权重，应同批更新合同、加载校验和准备文档。

导出环境单独固定 Python/PyTorch/Transformers/ONNX/ONNXScript/safetensors/ORT 版本与锁定记录；
本报告没有查验可运行的 Python 版本组合，不把 README 的无版本 `pip install` 当锁定方案。
先以 CPU FP32、未量化作为数值基线；保留许可证/归属、来源 revision、工具版本、文件大小、下载后
SHA-256 与导出后 SHA-256。应以实际中文/英文三种问题类型、多问题及边界长度对照官方 forward，
不能只运行脚本内 B=2/L=40/K=4 的随机样例。
参考源码和权重声明 Apache-2.0，receptron 导出代码 MIT；复制时保留适用的声明。
[官方许可声明](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/README.md)、[receptron 许可证](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/LICENSE)。

## Rust / ORT 候选与部署方式

| 组件 | 固定候选 | 已核实事项 |
| --- | --- | --- |
| Rust | 1.98.1 | 官方 stable manifest 日期 2026-09-03，Linux x86_64 GNU 可用 |
| `ort` / `ort-sys` | `=2.0.0-rc.13` | crates.io 非 yanked；源码 `002f41a8e175eac7f6695ff361d2e51a50874c48`；Rust最低1.88；MIT OR Apache-2.0 |
| ONNX Runtime | 1.28.0 CPU | 官方源码 `da9b5e364c465de65c49d91e696cd6485270757f`；MIT；提供 Linux x64/aarch64 二进制 |
| `tokenizers` | `=0.23.2` | crates.io 非 yanked；源码 `88a4498ad4ea1a9487b0a9b0ff881383fd5a06a3`；Apache-2.0 |

来源：[固定 Rust manifest](https://static.rust-lang.org/dist/channel-rust-1.98.1.toml)、
[ort 发布索引](https://index.crates.io/3/o/ort)、
[ort Cargo.toml](https://github.com/pykeio/ort/blob/002f41a8e175eac7f6695ff361d2e51a50874c48/Cargo.toml)、
[ORT 发布资产](https://github.com/microsoft/onnxruntime/releases/tag/v1.28.0)、
[ORT 许可证](https://github.com/microsoft/onnxruntime/blob/da9b5e364c465de65c49d91e696cd6485270757f/LICENSE)、
[tokenizers 发布索引](https://index.crates.io/to/ke/tokenizers)、
[tokenizers Cargo.toml](https://github.com/huggingface/tokenizers/blob/88a4498ad4ea1a9487b0a9b0ff881383fd5a06a3/tokenizers/Cargo.toml)。

`[建议/MED]` 首个探针采用 `ort` 的 `default-features=false`，功能为 `std,load-dynamic,api-28`，
显式加载对应架构的官方 CPU 动态库。无需 `ndarray` 就能用 owned Vec/shape 构造张量；
不启用 GPU、下载模型或训练特性。`tokenizers` 先用 `default-features=false, features=["onig"]`，
保持参考正则能力，不开启 `http`、进度条、训练用 `esaxx_fast`。
onig 引入原生正则依赖，最终锁文件/镜像仍需检查 C/C++ 工具链与原生链接结果；
实际 tokenizer 文件成功加载和完整 token IDs 对照才证明所选 feature 集够用。

`[已验证/HIGH]` `load-dynamic` 关闭 ort-sys 的构建期链接；`api-28` 选择 C API 版本 28。
加载器检查 `OrtGetApiBase` 和运行库版本，过旧运行库返回加载错误。
应用必须先调用可失败的 `ort::init_from(path)` 并处理错误，再创建任何 Session；
依赖首次隐式加载的路径有 panic 分支，不适合作为配置错误处理。
`EnvironmentBuilder::commit()` 返回是否设置成功的 bool，不是 `Result`。
[固定加载实现](https://github.com/pykeio/ort/blob/002f41a8e175eac7f6695ff361d2e51a50874c48/src/lib.rs)、
[环境初始化](https://github.com/pykeio/ort/blob/002f41a8e175eac7f6695ff361d2e51a50874c48/src/environment.rs)、
[ort-sys 功能](https://github.com/pykeio/ort/blob/002f41a8e175eac7f6695ff361d2e51a50874c48/ort-sys/Cargo.toml)。

| 官方 CPU 包 | 字节数 | 官方发布资产 SHA-256 | 本轮范围 |
| --- | ---: | --- | --- |
| `onnxruntime-linux-x64-1.28.0.tgz` | 9125960 | `a3e1b79d7bb1bf09696ce675f49e4064e6c81f6202b8225624fff0e93f8d6407` | 已下载到内存计算 SHA-256，与发布元数据相同；未运行 |
| `onnxruntime-linux-aarch64-1.28.0.tgz` | 8116278 | `e15ff8b5d85afe6c144d97c6fd432254bf76a219daaf17658087d6ecb3e8f0bb` | 发布元数据，未下载 |

来源：[官方 Release API](https://api.github.com/repos/microsoft/onnxruntime/releases/tags/v1.28.0)。
x64 包中含 `libonnxruntime.so.1.28.0` 与 `libonnxruntime_providers_shared.so`。
静态扫描主库字符串观察到 GLIBC 版本最高 `2.27`、GLIBCXX 最高 `3.4.21`；
这不是 ELF 动态依赖分析，也不是在目标发行版加载成功的证明。后续在 Linux 容器执行
`readelf --version-info`、`readelf -d`、`ldd`，确定实际所需符号与依赖并随镜像交付。
不要从二进制中的 CUDA provider 文件名字符串推断 CPU 包依赖 CUDA。

`[已验证/HIGH]` ort 自带的 Pyke 预编译路径明确要求 x86_64 CPU 有 AVX2；
这里选择官方 CPU 动态库以便分别核实平台要求，不能把该要求直接套到所有 ORT 包。
[AVX2 检查源码](https://github.com/pykeio/ort/blob/002f41a8e175eac7f6695ff361d2e51a50874c48/src/environment.rs)。
MVP 优先 GNU/glibc Linux 容器；musl/Alpine 不是已验证目标。
Darwin arm64 上的编译或运行不能替代 Linux 验收；可在 Docker Linux arm64 上做对应架构验证，
Linux amd64 另跑原生硬件验证，不能用模拟架构性能作为部署建议。16 核/32 GB 是调优场景，不是构建或验收前置硬门槛。

## Session 所有权、并发及最小探针

`[已验证/HIGH]` 所选版本 `Session` 实现 Send/Sync，但所有 `run_*` 方法仍要求 `&mut self`。
`run` 返回结果的生命周期关联 Session 借用；处理或复制所需输出后，才能释放执行槽。
作者明确建议并发时每线程一个 Session 或使用批处理。
[固定 Session 源码](https://github.com/pykeio/ort/blob/002f41a8e175eac7f6695ff361d2e51a50874c48/src/session/mod.rs)。

`[推断/HIGH]` 一个 `Mutex<Session>` 外加大小为 2 的 semaphore 仍只有一次 `run` 真正执行。
如契约配置并发 2，应准备两个独立 Session/执行槽并复用同一个固定磁盘模型资源；
不能保证跨 Session 权重内存共享，也不能先承诺加倍吞吐。实际 RSS 和收益由 benchmark 决定。
初始 intra-op=8/inter-op=1 只是方案中的起点；每 Session 都创建线程池时要计入总线程和超卖。
同步 `run` 放到 Tokio 阻塞边界，permit 留在实际工作拥有的对象里直到 CPU 工作结束；
HTTP 超时、取消等待或连接断开不能提前释放名额。退出时停止准入并接收所有在途任务结果。

下一个执行票的最小探针应输出可检查记录，而不是只返回“模型加载成功”：

1. 建立实际使用的 Cargo 工程，固定工具链和依赖、提交 `Cargo.lock`；验证 fmt/clippy/test 与 Linux 编译。
2. 显式加载固定 ORT CPU 动态库，记录版本与 provider；加载 tokenizer，核对特殊 token 与参考 token IDs。
3. 下载后哈希核验通过的 bundle 才能创建 Session；用 `inputs()`/`outputs()` 记录 name/dtype/shape/动态符号。
4. 运行固定官方中文和英文 Choice/Score/Noul 样例，以及 B=1/B>1、混合 K、最大长度和 padding；
   核对原始 logits、act probabilities、校准后概率和最终 JSON。数值容差由对应兼容性契约提前给出。
5. 在 Linux CPU 容器读取原生依赖并运行服务 smoke；将缺少权重、下载失败、运行库不匹配、动态维度失败
   各自报告为失败或未执行，不允许 mock 或跳过的模型测试使真实验收变绿。

该探针无需先有 16 核机器，无需实现跨请求动态合批或量化。多 Session RSS/收益、并发 1/2/4/8
性能矩阵留到部署 benchmark 票；本研究没有性能数据。

## 本轮检查记录

- 已读取固定 Hub 文件清单、LFS 元数据、模型卡、配置与源码；已对三个官方小配置、官方模型源码和
  ORT x64 发布压缩包的实际读取字节计算 SHA-256。以上哈希范围在表内逐项区分。
- 已核实 crates.io 稀疏索引中的候选发布及 `yanked=false`，GitHub tag 对应的源码 commit，
  Rust 官方发布 manifest，以及 ORT Linux x64/aarch64 发布资产。
- 未下载 tokenizer 大文件或模型权重；未读取 ONNX 图、创建 Session、导出、编译、真实推理或压测。
- 后续 bundle 来源、文件布局、实际图接口和已编译锁文件应由执行票把实测结果写回方案；
  本报告不能作为 Jev 完全兼容、模型可用或运行镜像已通过验收的证明。
