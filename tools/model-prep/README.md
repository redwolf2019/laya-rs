# multilingual 模型准备

任务 [#7](https://github.com/redwolf2019/laya-rs/issues/7)。在仓库根目录执行。
这些 Python 工具只用于独立开发期导出/对照；Cargo、服务启动和发布镜像不调用它们。
最终运行文件及许可位于忽略目录 `models/multilingual/`，输入 checkpoint、下载元数据和
中间日志位于其 `prep/` 子目录。权重不入 Git，没有上传到第三方模型仓库。

## 固定输入与环境

官方源码 `he-jev/laya@c5d78730f3493e4fe16d61507ef4b78eef7318cf` 的 `rl_common.py`
保持原样，用同一个 `DecisionModel` 分别执行官方 PyTorch forward 和导出。
checkpoint 为 `convaiinnovations/laya-multilingual@052592a15d198d9ad47da779604259b10b47b7aa`。
导出适配来源、改动与完整 MIT 通知见 [NOTICE](NOTICE.md)。模型的来源声明和 Apache 文本
保存在 bundle 的 `licenses/`，清单记录每个文件实际读取后的 SHA-256 与字节数。

标准库不能执行 PyTorch checkpoint 或生成 ONNX，因此只在该开发环境安装所需工具：

| 工具 | 固定版本 | 用途 / 上游许可 |
| --- | --- | --- |
| CPython | 3.11.16 | 与冻结参考一致；PSF |
| PyTorch | 2.10.0，实际为 `2.10.0+cpu` | 官方模型、torch.export / ONNX exporter；BSD-3-Clause |
| Transformers | 5.0.0 | checkpoint 的 encoder config 标记此版本，读取 ModernBERT / rope_parameters；Apache-2.0 |
| ONNX / ONNXScript | 1.20.1 / 0.5.4 | 保存及检查图；Apache-2.0 / MIT |
| ONNX Runtime | 1.28.0 | Linux ARM64 CPU Session；MIT |
| NumPy / safetensors | 2.3.3 / 0.7.0 | 误差检查、读取固定权重；BSD-3-Clause / Apache-2.0 |
| tokenizers | 0.22.2 | Transformers 参考 tokenizer；Apache-2.0 |

完整传递依赖在 [requirements.lock](requirements.lock)。本次使用 PyPI 的 CPython 3.11
Linux aarch64 wheels，`pip check` 通过，没有 CUDA 依赖；Python 包不能由 Rust MSRV 推断兼容性。
这里的 Python tokenizer 版本不覆盖后续 Rust tokenizers 的候选与对照要求。
原生执行环境为 Debian 12 / glibc 2.36、Docker Desktop Linux ARM64，8 vCPU、12 GiB RAM、
swap=0；与 [#5 环境](../../docs/validation-environment.md)采用同一 VM。
这是 Apple M2 Max 上的虚拟机容器，不是 amd64 或裸机性能验收。

## 复现命令

先确认 Docker Desktop daemon 可用、宿主机有足够磁盘空间。本次生成的模型约 1.29 GB，
还需保存约 644 MB 原 checkpoint、tokenizer、Python 环境及导出中间内存；本机执行前剩余约 62 GiB。
下面使用固定镜像 digest，`laya-model-prep-7` 是本次专用容器名；重用已存在的本次容器时跳过创建。

```sh
rtk proxy mkdir -p models/multilingual/prep
rtk proxy docker run -d --name laya-model-prep-7 --platform linux/arm64 \
  --cpus 8 --memory 12g --memory-swap 12g \
  --mount "type=bind,source=$PWD,target=/work" --workdir /work \
  -e PYTHONUNBUFFERED=1 -e HF_HUB_OFFLINE=1 -e TOKENIZERS_PARALLELISM=false \
  python@sha256:a36c24f9cbdf4fd0f52d67f0823eeac19c2028c637cecc392d97f980d4fec56b sleep infinity
rtk proxy docker exec laya-model-prep-7 python -m pip install \
  -r tools/model-prep/requirements.lock > models/multilingual/prep/install.log 2>&1
rtk proxy docker exec laya-model-prep-7 python -m pip check
rtk proxy docker exec laya-model-prep-7 python tools/model-prep/prepare.py \
  > models/multilingual/prep/prepare.log 2>&1
rtk proxy docker exec laya-model-prep-7 python -m compileall -q tools/model-prep
rtk proxy docker exec laya-model-prep-7 python tools/model-prep/export.py \
  > models/multilingual/prep/export.log 2>&1
rtk proxy docker exec laya-model-prep-7 python tools/model-prep/verify.py \
  > models/multilingual/prep/verify.log 2>&1
```

每步必须退出码 0 才执行下一步。下载失败保留 `.part`；重跑会重新下载未完成文件。
已有文件也重新计算哈希；与发布方 LFS SHA-256 或 Git blob ID 不同就失败，不复用损坏文件。
`HF_HUB_OFFLINE=1` 禁止模型库另取浮动 encoder 权重，准备脚本仍按固定 URL 下载。
`strict=True` 加载所有 checkpoint 参数；原始 checkpoint 主要为 fp16 存储，加载后以 fp32
计算和导出，没有重新量化，也不能恢复原始 fp16 保存时已损失的精度。

采集环境，确认数值验证通过后生成小型交接记录：

```sh
rtk proxy docker exec laya-model-prep-7 sh -c '
  uname -a
  cat /sys/fs/cgroup/cpu.max /sys/fs/cgroup/memory.max /sys/fs/cgroup/memory.swap.max
  ldd --version | head -n 1
  python --version
  python -m pip check
  python -m pip freeze --all
' > models/multilingual/prep/environment.log
rtk proxy docker exec laya-model-prep-7 python tools/model-prep/record.py
rtk proxy docker stop laya-model-prep-7
```

`record.py` 重算图和运行文件哈希，生成 [model-manifest.json](../../docs/model-manifest.json)，
并复制本次原始日志及逐样例结果到 [验收目录](../../docs/validation/model-prep)。
导出与验证开始时删除旧成功报告；失败不会生成新的通过记录，也不得关闭 #7 或放宽阈值。
重建可能因 exporter 图命名等因素得到不同图哈希，须保留新清单并重新对照，不能把它称为原文件。

## 实测范围与使用边界

`[已验证/HIGH]` 2026-09-23 的真实 Linux CPU 结果见
[validation.json](../../docs/validation/model-prep/validation.json) 和
[原始验证日志](../../docs/validation/model-prep/verify.log)。
最终图为 ONNX IR 10 / opset 18，external-data location 为 `laya.onnx.data`。
浮点 initializer 为 FLOAT，官方参考参数与输出为 fp32，图未量化。
Session 显式只启用 CPUExecutionProvider，实际接口如下：

| 名称 | dtype | 实际 Session shape |
| --- | --- | --- |
| input_ids / attention_mask | int64 | `[batch, seq]` |
| marker_pos | int64 | `[batch, options]` |
| marker_mask | bool | `[batch, options]` |
| qtype | int64 | `[batch]` |
| logits | float32 | `[batch, options]` |
| act_probs | float32 | `[batch, 2]` |

特殊 token 实测为 CLS=2、SEP=1、MASK=4、PAD=0；encoder 配置的 CLS=1 不用于构造序列。
任务配置为 max_len=1024、head_max_len=256，三类型温度为 1，选项温度覆盖为空。

探针覆盖中英文 Choice/Score/Noul 单问题和混合问题、B=1/2/6/16、K=2/3/4/5/6/10/11/32、
长选项缩短、空状态/空指令、L=1024、同 batch 不同 L/K 的右侧 padding，另有 L=2 的纯张量探针。
输入由固定官方 `build_sequence` 和 `collate_items` 生成，双方接收完全相同的 token 张量。
真实候选 logits 使用 `atol=1e-4, rtol=1e-3`；act_probs 和未舍入主概率使用
`atol=1e-5, rtol=1e-4`，另检查 dtype、shape、有限性和 action 概率范围/行和。
padding logits 不参与答案比较，但检查有限性和形状。逐案例原始输出保留在日志与 JSON 内。

服务使用范围仍为 B=1–16、L≤1024、每行至少两项且 K≤32；qtype 为 0/1/2，
token ID 必须在词表内，marker 必须落在有效 token 上。L=2 探针不是有效 System One 文本请求。
空文本用例的实际长度见结果；未测试全部整数形状组合，也未承诺任意稠密组合的内存/吞吐。
ONNX 的符号维度不会执行 torch.export 的 min/max 检查，调用方仍须按契约校验，
不能把图能接受越界张量当作服务允许扩大范围的依据。

这些样例的 action 输出饱和为 `[1,0]`，action 数值对照只覆盖了该分布；
不据此声称业务校准质量。HTTP、Rust tokenizer/推理、四位舍入完整响应 golden、
Jev 客户端兼容性和性能验收属于后续任务。本次证明离线 bundle 在所记环境及样例上的对照结果。
日志保留 ORT CPU vendor、torch 导出轴名等警告，没有据这些警告推断性能。

## 后续 Agent 核验已有文件

本机产物为 `/Users/redwolf/MyProjects/laya-rs/models/multilingual`。
下面只用标准库，逐项检查 bundle 文件，不会重新下载或改写 manifest：

```sh
rtk proxy docker start laya-model-prep-7
rtk proxy docker exec -i laya-model-prep-7 python - <<'PY'
import hashlib, json
from pathlib import Path
m = json.loads(Path('docs/model-manifest.json').read_text())
root = Path(m['bundle_directory'])
for item in m['files']:
    p = root / item['path']
    assert p.stat().st_size == item['bytes'], p
    with p.open('rb') as f:
        assert hashlib.file_digest(f, 'sha256').hexdigest() == item['sha256'], p
print('PASS: bundle sizes and SHA-256')
PY
```

不存在本机文件时按前面的固定来源重建。模型加载任务只消费运行文件和 `licenses/`，
不需要挂载 `prep/`；发布服务时不能复制这个开发容器或安装 Python 依赖。
