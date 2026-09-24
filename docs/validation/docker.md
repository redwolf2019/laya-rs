# Linux CPU 运行镜像验收（#18）

日期：2026-09-24。任务：[#18](https://github.com/redwolf2019/laya-rs/issues/18)。
基线提交 `d1ecdd4d0cf6eb0bd1032f026b971a713d504970`，本票未修改 Rust、Cargo 依赖或模型。
下列结果为 `[已验证/HIGH]`，范围限于 Docker Desktop Linux ARM64、固定 bundle 和所列请求。
未推送镜像或代码；未验收 amd64、远程 CI、裸机或稳态性能。

## 构建与产物

[Dockerfile](../../Dockerfile) 从固定 Rust 1.98.1 镜像执行 `cargo build --locked --release`，
校验官方 ARM64 ORT 1.28.0 包 SHA-256，最终阶段使用固定 Debian 12 slim 镜像，
安装 ca-certificates、libstdc++6、libgcc-s1 及其必要依赖。
[构建原始输出](docker/build.log) 记录全部命令、下载与编译结果。
`.dockerignore` 采用构建输入白名单，模型、Git、宿主 target、导出工具与凭证文件不进入上下文。
从 `git archive d1ecdd4` 加本票 Dockerfile/.dockerignore 的独立临时目录再次构建，
[干净目录日志](docker/clean-build.log) 记录成功；两次 image ID 一致。此次复核复用了 BuildKit 缓存。
Debian 软件源没有做快照锁定，因此不承诺日后重建逐字节一致；每次交付需重新记录产物摘要。

| 产物 | 实测值 |
| --- | --- |
| 本地标签 / 平台 | `laya-rs:18` / `linux/arm64` |
| OCI manifest digest | `sha256:e3df4c8b3864440240bf034daee4adbb8f8ff1a0a0ca4aefaa8b1567dd980c17` |
| image ID（config digest） | `sha256:56128072f8b929ae37afe02ea1e3ce13505313908a23e76556ab80f5a4e26564` |
| 镜像大小（Docker inspect） | 151,889,042 bytes，非压缩传输大小 |
| 服务二进制 SHA-256 | `5540f9c7f257f12d416ec39c94df21dd8eb7cf8acff00a2ec8bed9192ceef319` |
| 模型图 SHA-256 | `5a028e8dac51de3c430514c47e047fc89911edaaa72762c3e880a3591cc0184c` |
| 权重 SHA-256 | `6ef993ee707fe1d6a75529f4a1f8347ff89966ff955d332ea3ebe4e5171a9ac1` |
| ORT 主库 SHA-256 | `f1ec1a08eb99bd6e5401340f0a2b101381bf4694415480291dc13bcaa30f9ec7` |

[image.json](docker/image.json) 另含 `docker save` 归档摘要和干净构建 image ID。
本地 Docker driver 不支持 buildx OCI exporter，改从 `docker save` 的 OCI index 读取 manifest，
重算其 SHA-256 并核对 config digest。未发布 registry，`RepoDigests` 为空，不能把本地标签当远端拉取地址。
逐层检查归档的 8 个 layer、5391 个路径，无模型权重、Python/Node/Rust/C 编译器可执行文件
或 PyTorch/CUDA/NVIDIA 库，见[层检查](docker/layers.log)。二进制归档只在 `/tmp`，不进入 Git。

## 运行依赖与权限

[native.log](docker/native.log) 保存服务/ORT 文件 SHA-256、完整 Debian 包名与版本、原生库目录。
[smoke.log](docker/smoke.log) 包含三个 ELF 的实际 `ldd` 和服务 `/proc/1/maps`；所有动态依赖解析成功，
真实进程映射了镜像内 ORT 主库和挂载的权重。依赖为 glibc loader、libc、libm、libgcc_s、
libstdc++、libdl、librt、libpthread；onig 编入 Rust 产物，没有另装系统 onig 运行包。
最终镜像没有 ORT 头文件、CMake/pkg-config 开发文件、编译工具或模型准备工具。

使用 [#5 原生探针](check-linux.sh) 中相同 C 程序，在已有开发容器编译后只读挂入最终镜像，
以其默认非 root 身份、`--network none` 实际加载最终 ORT。
[providers.log](docker/providers.log) 为 ORT version=1.28.0、API=28、provider count=1、
唯一 CPUExecutionProvider，Environment 创建与释放成功。保留 CPU vendor 警告，不推断其性能影响。
这个诊断程序没有写入镜像；生产加载器还会自行检查 CPU provider 并执行真实张量探针。

服务以 UID/GID 65532 运行，二进制为 PID 1；模型和根文件系统只读，禁用额外 capabilities。
脚本检查实际 UID、mount 的 RW=false、写入失败、证书与许可证文件，并验证以下启动失败：

| 输入 | 退出码 / 日志类别 |
| --- | --- |
| 模型目录未挂载 | 2 / existing local directory |
| 空模型目录 | 1 / bundle file is missing or unreadable |
| 图的首字节损坏，大小不变 | 1 / SHA-256 differs from manifest |
| Linux volume 内图文件 mode=000 | 1 / bundle file is missing or unreadable |
| 将 libc.so.6 作为 ORT 动态库 | 1 / native ONNX Runtime load or ABI check failed |

权限测试首次直接 bind 宿主 mode=000 文件，Docker Desktop 自身在创建挂载时失败（126），
[原始失败记录](docker/initial-bind-failure.log) 保留。改用 Linux volume 后服务实际启动并退出 1，
没有将 Docker 层面的失败当作应用权限验收。容器与临时 volume 均由脚本清理。
ABI 检查用实际 ELF 的缺失 ORT 导出符号触发；没有冒称测试过所有旧 ORT 版本。

服务 MIT/Apache、移植 NOTICE、Rust 标准库版权和编译依赖的 Cargo.toml/许可/NOTICE 均随镜像保留；
crate 内 oniguruma 等原生子组件的许可也递归收集。ORT LICENSE / ThirdPartyNotices 单独保留，
Debian 的版权文件留在系统包目录，模型许可仍随 bundle 挂载。

## 真实 HTTP 与退出

环境延续 [#5](../validation-environment.md)：Apple M2 Max 上的 Docker Desktop，Linux ARM64，
8 vCPU、12 GiB RAM、swap=0；intra=1、inter=1、concurrency=1。
[环境日志](docker/environment.log) 记录实际 daemon 与 VM 配额；脚本在容器内断言 cgroup 配额。
所有验收容器的 `OOMKilled=false`，没有 OOM 或因资源不足替换模型。此配置不代表吞吐建议。

[验收脚本](docker-smoke.py) 在宿主使用 Python 标准库组织 Docker/HTTP 检查；
Python 不进入服务或构建阶段，普通部署只需 README 中的 Docker/curl 命令。
固定 #10 的 21 个官方样例经 release 服务全部通过：中文/英文 Choice、Score、Noul、
混合 6/16 问题、长文本、JSON 和候选边界。舍入字段、离散结果、usage 和完整 envelope 精确比较，
act_probability 保留 `abs <= 1e-5 + 1e-4*abs(reference)`，不放宽已有门槛。
HTTP 不暴露 token/logits，本票不重复声称 HTTP 能直接验收中间张量。
health=200、ready=200；21 次请求后 requests=21、实际 run count=21、queue=0、inflight=0。
六个指标及类型完整保存在 smoke 日志；OpenMetrics 独立解析沿用 [#17](observability.md)。

| Docker stop 场景 | 实际观测 |
| --- | --- |
| 空闲 | SIGTERM 后排空退出 0 |
| 执行中 + 排队 | ready=503、health=200、排队 HTTP=503、queue=0；执行中 HTTP=200，回收全部任务后退出 0 |
| HTTP 已超时但 CPU 未结束 + 排队 | HTTP=504 时 inflight=1；stop 后 ready=503、排队 HTTP=503；grace=1 秒到期，记录 inflight=1/tracked_tasks=1 并退出 1 |

stop timeout 分别为 130/11 秒，大于服务 grace=120/1 秒，没有 Docker SIGKILL 抢先结束。
检查每次退出后 Running=false、Pid=0、端口不再 ready；脚本删除容器，顺序反复启动可用。
日志保留 shutdown_started、shutdown_complete 或 shutdown_grace_expired 及真实资源数。
归档日志仅去除行尾空白和文件末尾空行，保留原始事件与结果。

## 复现与检查

构建与正常启动采用 [README](../../README.md#docker-部署linux-arm64-cpu) 命令。开发期完整验收：

```sh
rtk proxy docker build --platform linux/arm64 -t laya-rs:18 .
rtk proxy python3 docs/validation/docker-smoke.py laya-rs:18 models/multilingual
rtk proxy docker image inspect laya-rs:18
rtk proxy docker save -o /tmp/laya-rs-18.docker.tar laya-rs:18
rtk proxy tar -xOf /tmp/laya-rs-18.docker.tar index.json
```

原生探针复用现有 `laya-loader-8` 开发容器（含 gcc 与只读 `/ort`），不修改服务镜像：

```sh
rtk proxy sh -c 'sed -n "/^#include <stdio.h>/,/^C$/p" docs/validation/check-linux.sh | sed "$ d" > /tmp/laya-docker-ort-probe.c'
rtk proxy docker cp /tmp/laya-docker-ort-probe.c laya-loader-8:/tmp/docker-ort-probe.c
rtk proxy docker exec laya-loader-8 gcc -Wall -Wextra -Werror -I/ort/include \
  /tmp/docker-ort-probe.c -L/ort/lib -Wl,-rpath,/opt/onnxruntime/lib \
  -lonnxruntime -o /tmp/docker-ort-probe
rtk proxy docker cp laya-loader-8:/tmp/docker-ort-probe /tmp/laya-docker-ort-probe
rtk proxy docker run --rm --network none --read-only \
  --mount type=bind,source=/tmp/laya-docker-ort-probe,target=/probe,readonly \
  --entrypoint /probe laya-rs:18
```

宿主 fmt、clippy `--locked --all-targets -- -D warnings` 和完整 `cargo test --locked` 通过，
76 passed / 8 ignored，见[检查日志](docker/checks.log)。一个 ignored 是父测试启动的子进程入口；
另七个为模型/tokenizer 专项，本次未逐个重跑；真实模型直接通过最终镜像的 HTTP 验收。
没有新增 Rust 逻辑，既有接口测试与信号子进程测试继续有效。

## Standards

按 code-review 技能，以任务开始提交 `d1ecdd4` 到提交前暂存区为范围，独立子代理对照仓库标准。
0 项标准违反，0 项 baseline smell；核对容器 finally 清理、日志读取失败时仍删除容器、
临时目录/volume 回收、超时及 fixture 比较，并实际执行 Python AST 解析。
该轴没有独立重跑 Docker 或模型。

## Spec

另一独立子代理逐项对照 #18 及 #17，核对验收脚本、日志和文档，0 项规格问题。
另行执行 `docker image inspect`，确认实际 image ID、linux/arm64 和 UID/GID 65532 与记录一致。
该轴没有重跑真实模型。两个轴均无待修复项；本地文档链接、脚本语法/函数长度与 diff 空白检查通过。
