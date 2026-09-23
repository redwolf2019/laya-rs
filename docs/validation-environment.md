# Linux CPU 验收环境

验收日期：2026-09-23。任务：[#5](https://github.com/redwolf2019/laya-rs/issues/5)。
运行库输入来自 [#3 的固定研究](research/mvp-runtime.md)。

`[已验证/HIGH]` 本机 Docker Desktop 的 Linux ARM64 容器已运行系统信息、ELF 依赖检查和
官方 ONNX Runtime 1.28.0 原生加载探针，退出码为 0。探针检查 C API 28、唯一的
`CPUExecutionProvider`，并创建、释放 ORT Environment。没有加载模型或执行推理。

原始记录：[宿主机与官方资产元数据](validation/host-20260923.txt)、
[Linux 与原生库输出](validation/linux-arm64-20260923.log)。命令及其输出原样保存，
包含 apt 安装和 ORT 警告；`readelf --version-info` 只保留实际需求的 Version needs 部分。

## 实测配置

以下为这次执行的快照，来源是上述原始记录，置信度均为 HIGH。
后续 benchmark 必须重新采集，不能把动态余量当作预留资源。

| 层级 | 实际配置 |
| --- | --- |
| 宿主机 | Apple M2 Max；Darwin arm64；macOS 27.0 / 26A428 |
| 宿主机资源 | 12 个逻辑 CPU；68,719,476,736 bytes（64 GiB）RAM |
| 容器引擎 | Docker Desktop 4.85.0 (235549)；Engine 29.6.2；containerd v2.2.5；runc 1.3.6 |
| Docker context | `desktop-linux`，本机 Docker Desktop Linux VM |
| Linux kernel | `6.12.76-linuxkit`，`aarch64` |
| VM 可见资源 | 8 vCPU；16,748,113,920 bytes（约 15.60 GiB）RAM；cgroup v2；overlay2 |
| 容器 CPU | `--cpus 8`；`cpu.max=800000 100000`；有效 CPU 集 `0-7`；未绑核 |
| 容器内存 | `--memory 12g --memory-swap 12g`；`memory.max=12884901888`；`memory.swap.max=0` |
| 容器用户空间 | Debian GNU/Linux 12 (bookworm)，Linux ARM64 |
| glibc / C++ runtime | glibc 2.36，`libc6=2.36-9+deb12u14`；`libstdc++6=12.2.0-14+deb12u1` |
| 宿主机剩余磁盘 | `df -k .` 为 66,136,028 KiB（约 63.07 GiB） |
| 容器 overlay 剩余磁盘 | 233,794,948 KiB（约 222.96 GiB）；是虚拟磁盘内的空闲空间 |
| 只读挂载所在磁盘 | 容器 `df -k /assets` 为 66,000,000 KiB 可用 |

`[已验证/HIGH]` 宿主机进程包含 `com.docker.virtualization`；容器 `lscpu` 为 Apple、8 CPU，
Model name 为 `-`。CPU 型号来自宿主机 `sysctl`，不从容器的空型号推测。
本次 ARM64 宿主、daemon、镜像、ELF 与容器架构一致，没有请求 amd64 模拟执行。
这是虚拟机中的容器验收，不能描述为 16 核 / 32 GB 裸机测量，也不代表 Linux amd64 性能。

Docker Desktop 的 settings-store 文件读取被系统拒绝；本次没有修改其配置。
表中的 VM RAM 是 daemon 报告的有效总量，不冒充设置页的名义分配值；
具体 VMM 后端及 Rosetta 开关没有核实。容器内存上限由本次运行参数显式设定，低于 VM 可用总量。
宿主机与其他容器仍共享资源。虚拟磁盘空闲量不能突破宿主磁盘实际可用量；下载模型、导出或构建前
重新检查两层磁盘余量。是否足够承载完整模型及并发 Session，须由后续 RSS/OOM 实测判断。

## 官方 CPU 原生库

`[已验证/HIGH]` 来源为微软官方 [v1.28.0 release](https://github.com/microsoft/onnxruntime/releases/tag/v1.28.0)
及 [Release API](https://api.github.com/repos/microsoft/onnxruntime/releases/tags/v1.28.0)。
本次下载 `onnxruntime-linux-aarch64-1.28.0.tgz`（8,116,278 bytes），宿主机重新计算 SHA-256，
容器内解压前再次校验；两次均与官方资产 digest 一致：

```text
e15ff8b5d85afe6c144d97c6fd432254bf76a219daaf17658087d6ecb3e8f0bb
```

`[已验证/HIGH]` 包内主库及 providers_shared 库都是 AArch64 ELF。主库 `DT_NEEDED` 为：
`libdl.so.2`、`librt.so.1`、`libpthread.so.0`、`libstdc++.so.6`、`libm.so.6`、
`libgcc_s.so.1`、`libc.so.6`、`ld-linux-aarch64.so.1`。providers_shared 库依赖
`libstdc++.so.6`、`libm.so.6`、`libgcc_s.so.1`、`libc.so.6`。
`ldd` 全部解析成功，无 CUDA/GPU 动态库依赖。
主库所需最高版本为 `GLIBC_2.27`、`GLIBCXX_3.4.21`、`CXXABI_1.3.11`；
providers_shared 的版本需求为 `GLIBC_2.17`。这不等于已经在所有满足这些符号版本的系统运行过。

探针用发行包自带 `onnxruntime_c_api.h` 编译，输出：

```text
onnxruntime cpuid_info warning: Unknown CPU vendor. cpuinfo_vendor value: 0
ORT version: 1.28.0; requested C API: 28
Provider count: 1
CPUExecutionProvider
PASS: native ORT load, API 28, CPU-only provider, environment creation
```

CPU vendor 警告保留为后续推理与性能检查的观察项；本次未判断其性能影响。
诊断容器内安装检查工具前后均未发现 `python`、`python3`、`node` 或 `nvidia-smi`。
运行命令不传 `--gpus` 或 GPU 设备，服务只允许 CPU provider；不能因宿主机存在 GPU 就声明容器使用 GPU。
ORT 包的许可证和 ThirdPartyNotices 随原包保留，后续发布镜像须保留适用许可。

## 最小复现

在仓库根目录执行。宿主 shell 使用 RTK；容器内无需安装 RTK，外层 `rtk proxy` 保留原始输出。
先启动已有 Docker Desktop；只有后续 daemon 检查成功才继续：

```sh
rtk proxy open -a Docker
rtk proxy docker --context desktop-linux info
```

若启动尚未完成，等待应用显示 Engine running 后重试 `info`；失败时停止验收。
准备独立临时目录并下载官方包，不将二进制或模型权重加入 Git：

```sh
validation_assets="$(rtk proxy mktemp -d /tmp/laya-validation.XXXXXX)"
validation_image='debian@sha256:3783cc01769c7b2b1b83a5c5ad96c815348e28ed7da68e2e3687004faa906251'
rtk proxy curl -fL --retry 2 \
  https://github.com/microsoft/onnxruntime/releases/download/v1.28.0/onnxruntime-linux-aarch64-1.28.0.tgz \
  -o "$validation_assets/onnxruntime-linux-aarch64-1.28.0.tgz"
rtk proxy shasum -a 256 "$validation_assets/onnxruntime-linux-aarch64-1.28.0.tgz"
rtk proxy docker --context desktop-linux pull --platform linux/arm64 "$validation_image"
rtk proxy docker --context desktop-linux image inspect "$validation_image" \
  --format '{{json .RepoDigests}} {{.Id}} {{.Os}}/{{.Architecture}}'
```

本次镜像来自 Docker Official Image `debian:bookworm-slim`，复现固定上面的 repository digest；
实际 ARM64 image ID 是 `sha256:813cd0370d827a30abbef1cf99f88217f87fda9612dd5241c9015ba182d32428`。
执行 [检查脚本](validation/check-linux.sh)，把新日志写到临时目录，保留本次基线：

```sh
rtk proxy docker --context desktop-linux run --rm --platform linux/arm64 \
  --network bridge --cpus 8 --memory 12g --memory-swap 12g \
  --mount "type=bind,source=$validation_assets,target=/assets,readonly" \
  --mount "type=bind,source=$PWD/docs/validation/check-linux.sh,target=/check-linux.sh,readonly" \
  "$validation_image" bash /check-linux.sh \
  > "$validation_assets/linux-arm64.log" 2>&1
validation_status=$?
rtk proxy cat "$validation_assets/linux-arm64.log"
rtk proxy test "$validation_status" -eq 0
```

脚本检查架构、资源限制、包哈希、ELF 依赖、版本和 CPU provider，任何命令失败立即退出。
诊断工具通过 Debian 仓库安装 `gcc libc6-dev binutils file`，版本在日志内；APT 仓库随时间变化，
此命令用于复验环境，不承诺诊断容器逐字节重建。原生库本身由固定 SHA-256 校验。
当前没有 Cargo 工程，未执行 Rust typecheck、fmt、clippy 或 Rust 测试套件。

## 后续使用与 benchmark 输入

参考/导出环境另用专用目录和工具链锁定记录；允许其中安装 Python/PyTorch 等工具。
Rust 服务的构建与运行使用独立容器，只消费已验收 bundle 和官方 ORT CPU 库，
不复用参考环境的 Python/Node 安装。本票的临时 C 探针只诊断原生库，不是服务实现或发布镜像。

后续 Linux 容器沿用上述 `--platform` 和资源参数，在 `docker run` 的镜像参数之前追加：

```text
--mount "type=bind,source=$PWD/models/multilingual,target=/models/multilingual,readonly"
```

模型目录须先由模型准备任务生成；缺失时让 `--mount` 失败，不创建空目录伪装 bundle。
模型文件名、校验值和许可证沿用 [服务方案](laya-server-plan.md) 及模型验收产物。
Rust 构建镜像、服务镜像、模型挂载后的推理尚未在本票运行。

每次 benchmark 保存本页两份记录中的采集命令输出，再记录当次服务提交、镜像 digest、
模型及 ORT 哈希、容器 CPU/RAM/swap/绑核参数、ORT intra/inter-op 线程数、Session 数、
请求批量与输入长度、预热方式、重复次数、延迟/吞吐/RSS，以及宿主机其他负载。
内核、Docker Desktop、镜像或资源配置变化后重新生成环境记录。
本票没有吞吐、延迟、Jev 兼容性或模型正确性结论；实际模型验收属于后续任务。
