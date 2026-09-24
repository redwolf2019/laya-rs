# 有界排队与真实执行生命周期验收（#15）

日期：2026-09-24。来源：[任务 #15](https://github.com/redwolf2019/laya-rs/issues/15)、
[兼容契约 §7–8](../compatibility.md)、[同步 engine 验收](engine.md)。

`[已验证/HIGH，闭锁测试及固定 Linux ARM64 请求范围]`
[`scheduler`](../../src/scheduler.rs) 使用公平 Semaphore 排队，等待容量与执行槽分别限制。
加载器创建的 N 个独立 Session 全部交给调度器；短锁仅负责取还 Session 和状态登记，
锁不跨 await，也不覆盖分词、原生推理或输出处理。

调用方取得槽后立即提交 `spawn_blocking`；阻塞任务持有 permit 到 engine 返回，
先还 Session，再回落 inflight/释放 permit。排队 future 丢弃时移除等待项；
在途 future 丢弃或超时仅关闭结果接收端，真实任务继续占槽。
队满、排队超时、执行等待超时分别映射固定 429/503/504 与静态 envelope。
已观察到退出优先拒绝等待项；期限已到不返回晚到成功。

`Scheduler::run(&mut self)` 是必须持续驱动的所有者入口：JoinSet 保留所有任务，
每次 join 收到完成或任务错误；所有实际工作失败统一记录静态 Tracing 事件，
包括接收端已关闭、错误已缓冲后调用方再超时或取消的情况，不增加请求错误计数。
驱动 future 可以取消后恢复，句柄仍在原所有者中。服务必须保留所有者到 `close` 后排空，
不能把 Drop/abort 当作回收路径；grace 到期终止整个进程的集成属于后续生命周期任务。
正常引擎错误归还 Session；panic 关闭准入并回收剩余任务，不重新使用失败资源或自动重试。
`snapshot` 提供 accepting、waiting、inflight 和尚未 join 的任务数。

## 无模型时序验证

[`src/scheduler/tests.rs`](../../src/scheduler/tests.rs) 以 std channel 闭锁控制真实阻塞任务，
以 Tokio 暂停时钟推动期限；不依赖 sleep 猜测顺序。当前 10 项验证覆盖：

- N=2 满槽、取消运行等待者、N+1 不执行、只有 CPU 闭锁释放后才出现名额。
- FIFO、排队取消立即释放容量、新请求不能抢已分配给旧等待者的槽。
- queue-capacity=0 与满队列拒绝；队列期限和可用槽同时出现时拒绝执行。
- 执行等待超时仍占槽、晚到失败被接收并 join；已就绪结果遇期限仍返回超时。
- 引擎失败后复用、panic 后 unavailable、退出与排队期限同时出现、驱动取消后恢复回收。
- Tokio 协作预算耗尽仍可取得空槽；只在真实 semaphore 等待登记后计入队列。
- 接收端尚存活但不再 poll，后台错误已缓冲后再超时，仍记录一次失败事件。
- 静态错误表与配置边界。CLI 另验证 semaphore 可表示的并发上限。

```sh
rtk cargo test --locked --lib scheduler::tests
rtk cargo fmt --check
rtk cargo clippy --locked --all-targets -- -D warnings
rtk cargo test --locked
```

## 真实 Linux CPU 验证

复用 `laya-loader-8`：Docker Desktop Linux aarch64，8 vCPU、12 GiB、无额外 swap。
固定图、external data、ORT 与 Cargo.lock 摘要，以及依赖树见
[environment.log](scheduler/environment.log)。Rust 1.98.1、ort 2.0.0-rc.13、ORT CPU 1.28.0。
每 Session intra-op=2、inter-op=1，两个配置在不同进程运行，避免累计峰值互相污染。

每组同时提交 8 个相同 `mixed-6` 固定请求（每请求 6 个问题）；不合并也不去重。
所有响应按 #14 的原始完整答案/usage 比较和 action 容差校验。
Tracing 的 `laya_session_run` span 仅包住实际 `Session::run`，输入张量构造在 span 外，
输出处理也在 span 外；开发专项记录进入/退出的单调时刻与 span ID，逐事件检查并行上限。
实际观察到 8 次原生调用与 N=1/2 的重叠，不由 permit 数推断并行。

| 配置 | 原生最大重叠 | 加载后 RSS (kB) | 压力后 RSS (kB) | 进程峰值 VmHWM (kB) | 8 请求耗时 (ms) |
| --- | ---: | ---: | ---: | ---: | ---: |
| N=1 | 1 | 617620 | 681796 | 681796 | 2926 |
| N=2 | 2 | 1108532 | 1188524 | 1235044 | 1534 |

原始结果：[n1.log](scheduler/n1.log)、[n2.log](scheduler/n2.log)。
RSS/VmHWM 来自 `/proc/self/status`；包含运行库、Session、分词器及推理缓冲区，
不等于模型权重大小。只有每组一次、小型固定请求；耗时包含调度及分词/输出处理，
没有稳态 P50/P95/QPS 结论，不外推默认 threads=8、amd64 或 16 核裸机。

已有环境复现；权重与原生库从零准备沿用 [engine 验收](engine.md#从零准备并运行)：

```sh
rtk proxy docker start laya-loader-8
rtk proxy docker exec -w /work -e LAYA_TEST_MODEL=/work/models/multilingual \
  -e LAYA_TEST_ORT=/ort/lib/libonnxruntime.so -e LAYA_TEST_CONCURRENCY=1 \
  laya-loader-8 cargo test --locked --bin laya-server linux_scheduler_concurrency -- --ignored --nocapture
rtk proxy docker exec -w /work -e LAYA_TEST_MODEL=/work/models/multilingual \
  -e LAYA_TEST_ORT=/ort/lib/libonnxruntime.so -e LAYA_TEST_CONCURRENCY=2 \
  laya-loader-8 cargo test --locked --bin laya-server linux_scheduler_concurrency -- --ignored --nocapture
```

## 依赖与未覆盖范围

采用服务方案已选择的 Tokio 1.53.1，仅启用 rt-multi-thread/sync/time/macros，
开发期另启用 test-util；新增锁定包只有 tokio 和 tokio-macros，复用已有 pin-project-lite。
两包压缩下载共 936182 字节；这不是部署二进制的增量体积。
标准库没有 Tokio 的异步取消、定时器和 JoinSet；没有引入通用 engine trait 或额外线程池。
Tracing 0.1.44 已在原依赖树中，本次作为直接依赖提供静态事件及真实 run span，只启用 std。
两者为 MIT、MSRV 分别 1.71/1.65，源码 Cargo.toml 与本次 Linux 编译均已核验；
无新增原生 ABI/GPU/网络依赖，完整传递依赖由环境日志与 Cargo.lock 固定。
官方依据：[Tokio 阻塞边界](https://docs.rs/tokio/1.53.1/tokio/task/fn.spawn_blocking.html)、
[JoinSet 句柄及取消语义](https://docs.rs/tokio/1.53.1/tokio/task/struct.JoinSet.html)。

本票未实现 HTTP、Prometheus 注册、信号与进程 grace；调度器提供它们所需的关闭与状态接口。
本次真实模型只重跑 mixed-6 并发场景，未重新执行全部 21 个顺序 fixtures。
模拟闭锁与静态检查均不被用作模型兼容或性能证据。

## 最终检查与 code-review

宿主 `cargo fmt --check`、`cargo clippy --locked --all-targets -- -D warnings` 通过；
完整 `cargo test --locked` 为 57 passed / 6 ignored。新增 Linux 真实专项已在 N=1、N=2
独立进程各执行一次最终复验；其他 5 个既有模型/tokenizer 专项本次未独立重跑。
Markdown 本地引用与 `git diff --check` 通过。

评审基点是任务开始时的 `493bca6e067d6b7ad2af67b0a3753178c3623a41`，
按 implement 要求评审提交前工作树；两个子代理分别审阅标准与规格。

### Standards

代码标准问题 0 项；一处文档测试计数已从 8 更新到 10，并补齐失败事件边界。
短锁不跨 await 或覆盖 engine；阻塞任务持槽，所有者恢复驱动即可继续排空。
本轴审阅源码与记录，未独立重跑模型。

### Spec

首次发现 3 项：协作预算耗尽误报队满、缓冲错误后超时遗漏失败日志、期限起点偏移。
前两项新增确定性测试，在修复前分别复现 QueueFull 和日志计数 0；修复后通过。
队列期限改在入队登记时建立，执行期限在获槽转换时记录。
复核确认 3 项已解决、新问题 0 项，独立运行 10 项调度测试通过。

最终未解决发现：Standards 0，Spec 0。
