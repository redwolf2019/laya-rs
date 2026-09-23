# Rust CPU 加载验收（#8）

日期：2026-09-23。来源：[任务 #8](https://github.com/redwolf2019/laya-rs/issues/8)、
[#7 manifest](../model-manifest.json)、[固定张量来源](../../tools/model-prep/verify.py)。

`[已验证/HIGH]` Rust 在 Linux ARM64 加载真实 multilingual bundle，创建两个独立 CPU
Session，逐槽执行启动探针，再复用每个 Session 执行一次。固定输入为 #7 `tensor-L2`：
input_ids=`[[2,1]]`、attention_mask=`[[1,1]]`、marker_pos=`[[0,1]]`、
marker_mask=`[[true,true]]`、qtype=`[0]`。这只是模型张量探针，不是有效问题的 Sequence Builder。

两槽结果均为 logits=`[1.0814669,-1.09079]`、act_probs=`[1,0]`，shape 均为 `[1,2]`。
对照 #7 保存的官方 checkpoint 输出，满足冻结的 logits `1e-4 + 1e-3*abs(ref)`、
action `1e-5 + 1e-4*abs(ref)` 容差。所有输出有限，action 范围及和也通过检查。
原始记录：[smoke.log](model-loader/smoke.log)。action 饱和、L=2、B=1 是本次探针覆盖边界；
完整中文/英文三类型、动态 batch、序列和答案对照仍由后续任务验收。

Tokenizer 使用真实文件，核对 CLS=2、SEP=1、MASK=4、PAD=0 的词表 ID 和编码结果；
关闭隐式 padding/truncation，任务预算保持 1024/256。生产加载器内嵌 manifest，先逐文件
检查大小和 SHA-256，再解析配置。固定图内容哈希锁定 #7 已检查的 `laya.onnx.data` 引用，
无需重新实现 ONNX protobuf 解析；加载时也校验被引用文件内容。模型目录须保持不可变、只读。

## 环境与依赖

`[已验证/HIGH]` 本机 Docker Desktop Linux ARM64，Apple M2 Max 宿主；8 vCPU 配额、
12 GiB RAM、swap=0，Debian 12 / glibc 2.36。这是虚拟化验收，不代表 amd64 或裸机性能。
环境原始输出见 [environment.log](model-loader/environment.log)，宿主基线见
[#5 环境](../validation-environment.md)。

- 构建镜像：`rust@sha256:ff521445a372125ed4f76e1453a1f8098f2d05332d1601d30db1c1f62757e730`。
- 离线运行镜像：`debian@sha256:3783cc01769c7b2b1b83a5c5ad96c815348e28ed7da68e2e3687004faa906251`。
- 原生运行库：官方 `onnxruntime-linux-aarch64-1.28.0.tgz`，包 SHA-256 沿用 #5；实际加载
  `libonnxruntime.so.1.28.0` 的 SHA-256 为 `f1ec1a08eb99bd6e5401340f0a2b101381bf4694415480291dc13bcaa30f9ec7`。
- 图 SHA-256：`a0a46af6144f0e7ff66612461be9064f528b1734bc87911463d3c7fc306ce7ef`。
  external data：`6ef993ee707fe1d6a75529f4a1f8347ff89966ff955d332ea3ebe4e5171a9ac1`。

应用原先仅用标准库；JSON、SHA-256、Tokenizer 和 ORT 不由标准库提供，因此加入以下实际依赖。
版本、MSRV、许可证来自下载后的发布 manifest（`cargo metadata --locked`），原生/工具链支持
由本次 Linux 构建和运行验证；文档来源：[ort](https://docs.rs/ort/2.0.0-rc.13/ort/)、
[tokenizers](https://docs.rs/tokenizers/0.23.2/tokenizers/)、
[sha2](https://docs.rs/sha2/0.11.0/sha2/)、[serde_json](https://docs.rs/serde_json/1.0.151/serde_json/)。

| 依赖 | 版本 / MSRV | 许可与构建约束 |
| --- | --- | --- |
| ort / ort-sys | 2.0.0-rc.13 / 1.88 | MIT OR Apache-2.0；std/load-dynamic/api-28，无下载和 GPU feature |
| tokenizers | 0.23.2 / 未声明，本次以 1.98.1 编译 | Apache-2.0；仅 onig，无 HTTP feature |
| serde / serde_json | 1.0.229 / 1.56；1.0.151 / 1.71 | MIT OR Apache-2.0 |
| sha2 | 0.11.0 / 1.85 | MIT OR Apache-2.0；流式读取，不把权重整体读入哈希缓冲区 |

锁文件共 105 个第三方包（含平台依赖），本机缓存的对应 `.crate` 压缩包共 9,980,798 bytes。
主要传递依赖有 tokenizers 的 Rayon、onig/onig_sys、
serde，以及 ort 引入的 ndarray 和 libloading；应用没有直接引入 ndarray。
onig_sys 在构建时编译 C，准备 `gcc g++ libc6-dev pkg-config`；运行二进制的 `ldd` 仅显示
libc/libm/libgcc，ORT 在启动时动态加载，其 C++/glibc 依赖与许可证仍须随部署保留。
本次 release 二进制为 6,079,080 bytes（未额外 strip）；体积不是跨平台保证。

## 复现

先按 #7 准备 bundle、按 #5 获取并验证官方 ORT 包。在 Linux Rust 1.98.1 构建环境中执行：

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
LAYA_TEST_MODEL=/work/models/multilingual \
LAYA_TEST_ORT=/ort/lib/libonnxruntime.so \
  cargo test --release --locked --bin laya-server linux_real_model_smoke -- --ignored --nocapture
cargo build --release --locked
```

从宿主操作容器时用 `rtk proxy docker exec ...`。LAYA_TEST_* 仅为显式真实测试提供路径，
应用配置仍只走 CLI。缺模型、缺原生库或非 Linux 时该 ignored 测试会失败，不会跳过后报成功。
普通 CI 不提供权重/原生库，明确显示此测试 ignored / NOT RUN。

本次构建容器名称 `laya-loader-8`，配置为 `--platform linux/arm64 --cpus 8 --memory 12g
--memory-swap 12g`，仓库只读挂载到 `/work`，官方 ORT 目录只读挂载到 `/ort`，
`laya-target-linux` volume 挂载到 `/target`，`CARGO_TARGET_DIR=/target`。
Cargo registry 使用独立 `laya-cargo-registry` volume；APT 只安装上面的构建工具。

离线验收使用独立 Debian 镜像和 `--network none`，只读挂载 bundle、ORT 与 release 二进制：

```sh
/target/release/laya-server --model /model --ort-library /ort/lib/libonnxruntime.so
```

默认 intra=8、inter=1、Session 数=2。先报告资源已初始化，再报告 HTTP 未实现，退出码为 1。
smoke 另覆盖 intra=2、inter=2（启用图并行）及两槽重复使用；尚未测量并发吞吐和 RSS。

`[已验证/HIGH]` 离线容器无 python/python3/node/nvidia-smi。分别传不存在的库、libc.so.6
（缺少 ORT API）及下列编译出的旧版本 ABI 探针，均明确失败、退出码 1，没有 panic、
输入路径回显或资源初始化成功信息。该探针仅用于原生错误路径，不充当模型：

```c
#include "onnxruntime_c_api.h"
static const OrtApi* ORT_API_CALL no_api(uint32_t version) { (void)version; return 0; }
static const char* ORT_API_CALL old_version(void) { return "1.27.0"; }
const OrtApiBase* ORT_API_CALL OrtGetApiBase(void) {
    static const OrtApiBase base = {no_api, old_version};
    return &base;
}
```

在构建容器用 `cc -shared -fPIC -I/ort/include /tmp/laya-old-ort.c -o /target/liboldort.so`，
再以 `--ort-library /target/liboldort.so` 启动。原始输出见 [offline.log](model-loader/offline.log)。
ORT 的 `Unknown CPU vendor` 警告原样保留，本次不判断它对性能的影响。

## 检查范围

宿主和 Linux 的 fmt、clippy、普通全量测试均通过：14 passed、1 ignored。
Linux 原始检查见 [checks.log](model-loader/checks.log)；另显式执行真实 smoke：1 passed。
纯逻辑失败测试覆盖文件缺失/截断/同大小损坏、JSON/温度/预算非法、张量类型/名称/shape/
动态符号不符、原生库缺失。它们不能代替真实推理；真实证据为上述独立 smoke 和离线启动。
无 HTTP 或 ready 状态，本票没有宣称服务已就绪、完整 Jev 兼容或性能达标。

code-review 从任务起点 `0c83713c2f5e9ca73aca98167e2de94ccf1d25ec` 审查本批暂存变更：
独立 Standards 轴 0 项、Spec 轴 0 项。两轴均为源码/记录评审，没有重跑真实模型。
