# MVP 四阶段验收与 Linux CPU benchmark（#19）

日期：2026-09-24。任务：[#19](https://github.com/redwolf2019/laya-rs/issues/19)，
地图：[#1](https://github.com/redwolf2019/laya-rs/issues/1)。服务基线为
`2846a8ef7b549e1b76423189cba3db01beec6793`；本票只增加开发验收工具、结果和文档，
不修改 Rust、依赖、模型、官方期望或容差。

**结论：[已验证/HIGH] 固定 MVP 的四阶段强制门槛在本机 Docker Desktop Linux ARM64 通过。**
12 组正式负载共 2,180 请求，全部通过官方答案比较；Linux 模型、HTTP、资源、信号和部署回归通过。
该结论限于本文的固定输入、模型和环境，不扩展到 amd64、裸机或全输入空间。

## 上游证据与固定输入

[上游快照](validation/mvp/upstream.json) 保存 #2–#19 的需求与 resolution，
[提交核验](validation/mvp/commits.json) 保存完整 SHA：17 个上游提交对象均存在且属于基线历史。
下表依据产物、源码与原始结果，不以 Issue 的关闭状态代替验收。

| 票 | 固定提交 | 交付及核验入口 |
| --- | --- | --- |
| #2 | `03cdbac` | [协议研究](research/mvp-protocol.md)，官方源码 `c5d78730f3493e4fe16d61507ef4b78eef7318cf` |
| #3 | `ab20013` | [运行库研究](research/mvp-runtime.md)，模型 checkpoint `052592a15d198d9ad47da779604259b10b47b7aa` |
| #4 | `81d413d` | [冻结契约](compatibility.md)，字段、限制、错误、序列、数值门槛 |
| #5 | `37bbf66` | [Linux 环境](validation-environment.md)，CPU provider、ABI 与虚拟化边界 |
| #6 | `24774ab` | Cargo 工程、CLI 和锁定依赖，当前完整回归见下文 |
| #7 | `0c83713` | [导出记录](validation/model-prep/verify.log)，19 个真实 CPU 探针；旧图由 #13 替代 |
| #8 | `d169873` | [加载验收](validation/model-loader.md)，文件、Tokenizer、Session、逐槽探针 |
| #9 | `600f080` | [协议测试](../tests/system_one.rs)，非法 JSON、类型、资源限制与静态错误 |
| #10 | `4b1bd81` | [固定 fixtures](../tests/fixtures/README.md)，21 个官方请求；历史失败保留 |
| #11 | `5fea286` | [JSON 文本对照](../tests/json_text.rs)，Unicode、数字、顺序 |
| #12 | `e209c25` | [序列验收](validation/sequence.md)，五个输入、token/marker/mask/usage 精确比较 |
| #13 | `444fc1d` | [LayerNorm 修正与后处理](validation/postprocess.md)，21 个完整答案门槛通过 |
| #14 | `493bca6` | [engine 验收](validation/engine.md)，生产链路、logits、失败后 Session 恢复 |
| #15 | `3b03d8b` | [调度验收](validation/scheduler.md)，真实执行槽、FIFO、取消、超时、回收 |
| #16 | `cc5ff26` | [HTTP 验收](validation/http.md)，四路由、流式上限、八类错误、断连 |
| #17 | `d1ecdd4` | [观测及退出](validation/observability.md)，六指标、12 个信号场景、grace |
| #18 | `2846a8e` | [镜像验收](validation/docker.md)，干净构建、非 root、只读、实际依赖、stop |

特别核对了 #10 的 `manifest.status=failed`：这是旧图 long-padding Score 四位舍入的真实失败，
不是当前模型未通过。#13 展开 50 个 LayerNorm 后更换图，权重与官方期望不变；
[当前 manifest](model-manifest.json) 的 validation 记录 21 个请求通过。
本票重新计算 bundle 全部文件及 24 个 fixture 文件摘要，并核对 manifest 指向的历史/新图证据摘要。
实际文件值见 [input-hashes.json](validation/mvp/input-hashes.json)。

| 固定输入 | SHA-256 / 版本 |
| --- | --- |
| ONNX 图 | `5a028e8dac51de3c430514c47e047fc89911edaaa72762c3e880a3591cc0184c` |
| external data | `6ef993ee707fe1d6a75529f4a1f8347ff89966ff955d332ea3ebe4e5171a9ac1` |
| ORT CPU 主库 | `f1ec1a08eb99bd6e5401340f0a2b101381bf4694415480291dc13bcaa30f9ec7` |
| 服务 release 二进制 | `5540f9c7f257f12d416ec39c94df21dd8eb7cf8acff00a2ec8bed9192ceef319` |
| OCI manifest | `sha256:e3df4c8b3864440240bf034daee4adbb8f8ff1a0a0ca4aefaa8b1567dd980c17` |
| image ID / config digest | `sha256:56128072f8b929ae37afe02ea1e3ce13505313908a23e76556ab80f5a4e26564` |
| Rust / ort / ORT / tokenizers | 1.98.1 / 2.0.0-rc.13 / 1.28.0 / 0.23.2 |

OCI manifest 从 #18 的本地 `docker save` 归档重算摘要并核对 config digest 与实际 image inspect，
见 [image-verified.json](validation/mvp/image-verified.json)。所有测量按 image ID 启动。
镜像没有发布 registry；不能将本地标签或上述 OCI digest 当成可公开拉取的地址。
完整依赖锁定在 [Cargo.lock](../Cargo.lock)，原生包版本在每配置的 environment.log 与 #18 native.log。

## 测量环境与协议

[宿主记录](validation/mvp/host.log)：Apple M2 Max，12 个逻辑 CPU，64 GiB RAM，
macOS 27.0 / 26A428；Docker Engine 29.6.2，Linux `6.12.76-linuxkit`、aarch64。
Docker Desktop VM 可见 8 vCPU、16,748,113,920 bytes RAM。每个服务容器显式限制
8 CPU、12 GiB、swap=0，cgroup v2；未绑核。其他两个开发容器在采集时 CPU=0%，保持空闲，
测量期间不并行运行模型回归或构建。宿主其他工作与虚拟化仍可能引入噪声。

每个配置独立创建服务进程；容器 UID/GID 65532，根文件系统和 bundle 只读。
`container.json` 保存实际 Cmd、镜像 ID、CPU/RAM 配额与挂载，`environment.log` 保存内核、
CPU 信息、cgroup 限制、二进制与 ORT 摘要；`exit.json` 保存退出码和 OOMKilled。
工具在异常配置清理前也保存 State，输出目录内的配置日志保留服务日志和清理时状态；
失败组保存实际完成数，已有成功组统计不覆盖。此次矩阵完整终端记录在
[benchmark.log](validation/mvp/benchmark.log)，采集期间仅修复失败路径，成功测量/统计逻辑未变
（[采集版本记录](validation/mvp/collector-version.json)）。

- 服务配置 `(intra-op threads, Session slots)` 为 `(1,1)`、`(2,1)`、`(2,2)`；
  inter-op 固定 1，执行图为顺序模式。每槽一个独立 Session。
- 各配置按客户端并发 1、2、4、8 顺序运行。客户端采用闭环：每个线程等响应后再发下一次，
  无重试、无思考时间；不模拟固定到达率，也不做 coordinated-omission 校正。
- 请求固定为 #10 [mixed-6.json](../tests/fixtures/system-one/mixed-6.json) 中的原始 request_json：
  中英文 Choice/Score/Noul 各一题，每请求 6 题，`usage.input_tokens=228`、output_tokens=0。
  各行有效 token 数为 41/44/38/28/36/41，输入 shape=[6,44]，marker shape=[6,4]。
  不混入更短请求以提高 QPS，不合并请求。正文、SHA 和协议保存在 [protocol.json](validation/mvp/benchmark/protocol.json)。
- queue-capacity=32，queue-timeout=30 秒，inference-timeout=120 秒，shutdown-grace=120 秒。
  客户端 socket timeout=150 秒；请求通过宿主 Python urllib/Docker 端口转发，新 HTTP 连接，
  延迟包含连接、传输、排队、分词、推理与响应处理。
- 每组先顺序预热至少 10 秒且至少 10 个成功请求，预热样本单独保存。
  正式测量同时满足至少 60 秒、至少 100 个完成 HTTP 请求后停止发新请求，再等在途请求结束。
  每组上限为 300 秒发请求或 10,000 次尝试，尾部最多等待一次客户端超时。
  不足门槛时保留实际数目，百分位/QPS 为 null，配置失败使命令非零退出。
- 所有 HTTP 200 都逐字段核对官方答案：舍入字段、Choice、usage、envelope 精确相等，
  action 概率使用 `abs <= 1e-5 + 1e-4*abs(reference)`。错误、超时、拒绝和答案不符分别记录。
  延迟百分位仅统计通过答案比较的请求，使用排序后的 nearest-rank `ceil(p*N)`。
  成功 QPS=成功数/最后一次完成相对测量起点的秒数，包含尾部排空。
- error rate 为所有非成功尝试/全部尝试；timeout rate 含排队、推理和客户端超时；
  rejection rate 为 HTTP 429/503 比例，三者可能重叠。连接失败不是已完成 HTTP 请求。
- 每约 1 秒保存一次 `/proc/1/stat`、`/proc/1/status`、memory.events 和完整 `/metrics`。
  CPU%=PID 1 的 utime+stime 增量/CLK_TCK/采样边界墙钟秒数×100，100% 代表一个核，
  不包含 docker exec 采样进程；CPU 窗口略宽于请求窗口。
  RSS 是服务进程 VmRSS，报告采样峰值；queue/inflight 报告采样最大值，短峰可能漏采。
  RSS 不等于 cgroup 内存，也不等于权重文件大小。

冷启动另存 `cold.json`：create_to_ready 包含 Docker create/start、bundle 摘要、加载与逐槽探针、
HTTP ready 轮询。另测第一个业务请求。未清理宿主/VM 文件缓存，且服务启动必跑探针；
这是新进程启动测量，不是冷磁盘或未预热模型的首个 forward。每配置仅一次，无冷启动分位数。

## 实测结果

`[已验证/HIGH]` 12 组均满足至少 60 秒和 100 个完成请求，共 2,180 个测量请求，全部通过官方答案比较。
每组错误率、超时率、拒绝率均为 **0%**；三个容器正常退出 0，OOMKilled=false。
[逐组复核](validation/mvp/benchmark-audit.json) 重算全部统计，核对 HTTP/run counter 增量、
结束时 queue/inflight=0、槽/队列上限与 memory.events 的 oom_kill=0。
CPU、RSS 和 queue/inflight 为上述采样口径，不把客户端并发当作真实执行并发。

| intra / slots | 客户端 | 完成数 | 秒 | P50 ms | P95 ms | P99 ms | 成功 QPS | CPU % | RSS 峰值 MiB | queue / inflight 最大值 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1/1 | 1 | 100 | 70.66 | 704.3 | 720.2 | 733.7 | 1.415 | 99.7 | 660.8 | 0 / 1 |
| 1/1 | 2 | 101 | 71.07 | 1405.4 | 1431.8 | 1436.8 | 1.421 | 100.0 | 660.8 | 1 / 1 |
| 1/1 | 4 | 103 | 72.91 | 2811.5 | 2952.3 | 2984.4 | 1.413 | 100.0 | 709.0 | 3 / 1 |
| 1/1 | 8 | 107 | 77.56 | 5769.1 | 5979.5 | 6080.6 | 1.380 | 99.9 | 709.0 | 7 / 1 |
| 2/1 | 1 | 160 | 60.15 | 372.6 | 395.7 | 425.6 | 2.660 | 199.3 | 661.5 | 0 / 1 |
| 2/1 | 2 | 162 | 60.49 | 739.9 | 788.7 | 862.0 | 2.678 | 199.8 | 661.5 | 1 / 1 |
| 2/1 | 4 | 164 | 61.11 | 1479.8 | 1552.8 | 1600.8 | 2.684 | 199.9 | 661.5 | 3 / 1 |
| 2/1 | 8 | 166 | 62.88 | 3015.3 | 3140.7 | 3173.5 | 2.640 | 199.9 | 661.5 | 7 / 1 |
| 2/2 | 1 | 159 | 60.08 | 374.9 | 400.5 | 431.0 | 2.646 | 199.3 | 1117.2 | 0 / 1 |
| 2/2 | 2 | 315 | 60.17 | 379.9 | 407.1 | 416.4 | 5.235 | 398.2 | 1180.3 | 0 / 2 |
| 2/2 | 4 | 318 | 60.71 | 756.7 | 800.5 | 820.8 | 5.238 | 399.4 | 1228.5 | 2 / 2 |
| 2/2 | 8 | 325 | 61.43 | 1499.2 | 1573.5 | 1584.6 | 5.291 | 399.4 | 1228.5 | 6 / 2 |

每行的 `requests.jsonl` 保存逐请求起止偏移、延迟、HTTP status 和比较结果；
`resources.jsonl` 保存资源/metrics 原文；`boundaries.json` 保存前后计数；`warmup.json` 单列预热。
完整目录：[1线程/1槽](validation/mvp/benchmark/t1-s1)、[2线程/1槽](validation/mvp/benchmark/t2-s1)、
[2线程/2槽](validation/mvp/benchmark/t2-s2)。

| intra / slots | create 到 ready 秒 | 首个业务请求 ms |
| --- | ---: | ---: |
| 1/1 | 1.990 | 698.0 |
| 2/1 | 2.196 | 385.1 |
| 2/2 | 2.610 | 379.7 |

**建议（推断/MED，只针对本机 mixed-6）**：单客户端先用 `--threads 2 --max-concurrency 1`；
多个持续客户端可用 `--threads 2 --max-concurrency 2`，inter-op 保持 1。
单槽从一线程改为两线程，单客户端吞吐从 1.415 增至 2.660 QPS；双槽在两客户端时达到约 5.235 QPS，
但单客户端仍约 2.646 QPS，且多占一个 Session 的内存。
客户端数超过执行槽后主要增加排队；不要用并发 8 当作 8 路真实推理。
两槽/两客户端在本次比较中已接近其更高客户端并发的吞吐，较低排队有利于延迟。
RSS 峰值只覆盖这段固定输入，不能据此降低生产内存上限或保证长文本/16题请求的内存。
每组合仅测一次且固定顺序，热状态和共享宿主可能影响结果；不作显著性或全局最优结论。
默认 CLI 的 threads=8、镜像默认 threads=4、inter-op>1 都未纳入此性能比较，默认值未改。

## 四阶段门槛与回归

| 阶段 | 强制门槛与证据 | 本次执行状态 |
| --- | --- | --- |
| 一：兼容依据 | 固定官方 revision、模型/许可/hash、ORT ABI/provider、冻结契约、21 个参考 fixtures | 输入与证据摘要已复核；#18 镜像摘要已重算 |
| 二：真实链路 | 精确序列/usage、logits 原容差、完整答案与数值边界、失败恢复 | [engine](validation/mvp/engine.log)、[loader](validation/mvp/loader.log)、[sequence](validation/mvp/sequence.log) 全部通过 |
| 三：服务资源 | 四路由、非法输入、body/问题/选项上限、队满/超时/断连、六指标、信号/grace | [真实 HTTP](validation/mvp/http.log)、[N=1](validation/mvp/scheduler-n1.log)/[N=2](validation/mvp/scheduler-n2.log)、[普通测试](validation/mvp/tests.log) 全部通过 |
| 四：部署性能 | 最终镜像真实请求、权限/依赖/失败/退出、12 组负载与线程/槽建议 | [Docker smoke](validation/mvp/docker-smoke.log) 与 12 组负载全部通过 |

[checks-protocol.json](validation/mvp/checks-protocol.json) 固定回归命令，
[checks.json](validation/mvp/checks.json) 记录各步退出码/耗时，全部为 0。
Linux fmt、clippy `--locked --all-targets -- -D warnings` 和完整 `cargo test --locked` 通过，
普通套件为 **76 passed / 8 ignored**；七个模型/tokenizer 专项在本票分别显式运行通过，
其中调度专项在 N=1/2 各跑一次。另一个 ignored 是父测试启动的信号子进程入口，
正常套件的父测试覆盖 12 个 SIGINT/SIGTERM 场景。没有把 ignored 计作通过。

本次真实 HTTP 专项包含 21 个固定请求、并发重复、chunked 超限、断连、原生失败及恢复，
核对实际 run 与 HTTP 计数；Docker smoke 再经最终 release 镜像比较 21 个官方完整响应，
实际检查 UID、只读模型、动态链接/进程映射、缺失/损坏/权限/ABI 失败和 Docker stop。
正常排空退出 0；HTTP 已超时仍有 CPU 工作时 grace 到期退出 1，并保留 inflight=1/tracked_tasks=1，
不是 OOM，也没有把 CPU 工作误记成已取消。三组 benchmark 容器均正常退出、无 OOM。

新工具自检先复现缺失命令行入口，再通过固定样本验证 nearest-rank、QPS、错误比例及不足样本。
评审发现的截断响应通过真实 localhost HTTP 服务复现 IncompleteRead；修复后落为 transport_error 样本。
最终 [自检日志](validation/mvp/benchmark-test.log) 两项通过。Python AST、函数长度、本地文档引用和 diff 空白检查通过。
本票复用 #18 的已交付镜像，未重新导出模型、下载权重或重新构建镜像。
其二进制和运行库摘要与 #18 相同，生产 Rust/Cargo/Dockerfile 未修改。
不重新宣称历史检查都在本票执行；#18 的独立干净构建、镜像层检查、独立 CPU provider 探针，
#17 的独立 OpenMetrics 解析结果通过对应记录引用。

未执行：Linux amd64、16 核/32 GB 裸机、远程 CI、其他硬件、其他语言的完整模型验收，
以及生产流量分布、固定到达率、长时间稳定性、inter-op 调优、超过两个执行槽的性能比较。
不声明 Jev 全输入兼容、不设延迟/QPS SLA；固定样例通过不代表全输入空间兼容。

## 复现

已有固定 bundle 时，构建与启动不需要 Python。先按 [README 部署步骤](../README.md#docker-部署linux-arm64-cpu)
准备文件并保证 UID 65532 可读，再执行：

```sh
rtk proxy docker build --platform linux/arm64 -t laya-rs:local .
rtk proxy docker run -d --name laya-server --platform linux/arm64 \
  --cpus 8 --memory 12g --memory-swap 12g --read-only --cap-drop ALL \
  --security-opt no-new-privileges -p 127.0.0.1:8080:8080 \
  --mount "type=bind,source=$PWD/models/multilingual,target=/models/multilingual,readonly" \
  laya-rs:local --model /models/multilingual --threads 1 --inter-op-threads 1 \
  --max-concurrency 1 --queue-capacity 32 --queue-timeout 30 \
  --inference-timeout 120 --shutdown-grace 120
rtk proxy docker logs laya-server
rtk proxy curl -fsS http://127.0.0.1:8080/readyz
rtk proxy curl -fsS http://127.0.0.1:8080/v1/system-one \
  -H 'Content-Type: application/json' \
  --data '{"state":"客户要求退款。","questions":{"urgent":{"type":"noul","instructions":"是否紧急？"}}}'
rtk proxy docker stop --timeout 130 laya-server
rtk proxy docker rm laya-server
```

开发验收客户端使用 Python 3.11+ 标准库；输出目录必须不存在，避免覆盖证据。
运行时停止其他负载。每组至少 60 秒，完整矩阵需要十几分钟，慢机器可能更久。
本工具不修改 Docker Desktop 配额，不安装依赖或进入服务镜像。

```sh
rtk proxy python3 docs/validation/test-benchmark.py
rtk proxy python3 docs/validation/benchmark.py run laya-rs:local models/multilingual /tmp/laya-benchmark-new
rtk proxy python3 docs/validation/benchmark.py summarize /tmp/laya-benchmark-new/t1-s1/c1/requests.jsonl
rtk proxy python3 docs/validation/docker-smoke.py laya-rs:local models/multilingual
```

真实 Rust 专项在 Linux 开发容器执行，准备容器和原生库见
[engine 从零命令](validation/engine.md#从零准备并运行)。已有 `laya-loader-8` 时：

```sh
rtk proxy docker exec -w /work -e LAYA_TEST_MODEL=/work/models/multilingual \
  -e LAYA_TEST_ORT=/ort/lib/libonnxruntime.so laya-loader-8 \
  cargo test --locked --bin laya-server linux_system_one_model_parity -- --ignored --nocapture
rtk proxy docker exec -w /work -e LAYA_TEST_MODEL=/work/models/multilingual \
  -e LAYA_TEST_ORT=/ort/lib/libonnxruntime.so laya-loader-8 \
  cargo test --locked --bin laya-server linux_http_model_parity_and_disconnect -- --ignored --nocapture
rtk proxy docker exec -w /work -e LAYA_TEST_MODEL=/work/models/multilingual laya-loader-8 \
  cargo test --locked --test sequence -- --ignored --nocapture
rtk proxy docker exec -w /work laya-loader-8 cargo fmt --check
rtk proxy docker exec -w /work laya-loader-8 cargo clippy --locked --all-targets -- -D warnings
rtk proxy docker exec -w /work laya-loader-8 cargo test --locked
```

## Standards

以任务开始提交 `2846a8e` 到提交前暂存区为范围，按 implement 的要求使用 code-review 双轴独立评审。
标准轴最终 **0 项违规、0 项判断性异味**。初审发现截断 HTTP 响应漏记失败样本，
已通过真实 localhost 截断响应先复现再修复。复核实际运行 CLI 自检、截断响应检查，
独立重算 12 组成功数/nearest-rank/QPS/门槛，并在临时目录检查失败报告保留已有资源统计。
本轴未重跑 Docker/模型或构建，只核对 Linux 检查记录。

## Spec

规格轴最终 **0 项未解决发现**。独立重算 12 组、2,180 请求的百分位、QPS、CPU、RSS、
queue/inflight，并核对预热、时长、完成数和三个容器退出状态，均与报告一致。
初审发现异常配置的 OOM/退出证据只打印终端，已改为清理前持久化 State/日志和部分样本统计；
复核确认不会覆盖已完成组的资源统计。两项修补只影响失败路径，不改变本次成功测量。
上游证据、Linux 回归记录、README/方案状态、复现命令与限制覆盖 #19；没有未解决的遗漏或范围扩张。
本轴未重新运行模型或构建。提交后发表 resolution；确认所有范围内子票关闭后再关闭地图。

最终发现数：Standards 0，Spec 0。
