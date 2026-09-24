# 观测与退出验收（#17）

日期：2026-09-24。来源：[任务 #17](https://github.com/redwolf2019/laya-rs/issues/17)、
[冻结契约 §7.3、§8](../compatibility.md)。以下结论为 `[已验证/HIGH]`，范围限于所列测试、
macOS 宿主及 Docker Desktop Linux ARM64；不外推 amd64、裸机性能或任意 TCP 时序。

## 计数与日志

[`metrics`](../../src/metrics.rs) 复用 prometheus-client 0.25.0；每个 scheduler 共享一份计数，
没有进程全局 registry。queue/inflight 从调度器的一次加锁快照读取，不另维护易失配的计数。
仅 POST System One 的路由进入增加 requests；终态 guard 在响应确定或 future 丢弃时观测一次耗时。
错误标签只含契约八种 code 及 client_cancelled。GET/HEAD、404/405 不计入业务请求。

[`engine`](../../src/engine.rs) 在五个输入张量准备好后，仅计时实际 Session run；
序列构造、分词、后处理均不在该直方图内。直接调用未绑定服务指标的 engine 不增加服务计数。
CPU 工作即使失去 HTTP 接收者也继续观测真实 run，晚到失败只记日志，不重复增加 HTTP errors。

[`HTTP 测试`](../../src/api/tests.rs) 验证成功、校验、队满、两个超时、取消和晚到失败。
暂停时钟下，排队 3 秒和执行等待 5 秒的三请求总 duration sum=8 秒；后台完成不增加 count。
受控 worker 没有实际 Session run，因此其 inference duration count 保持 0。
[`TCP 测试`](../../src/api/tests/network.rs) 保留实际断连与 chunked 超限检查。

[真实模型日志](observability/model-http.log) 记录 21 个固定请求、4 个并发重复请求、
1 个 chunked 拒绝、1 个执行中断连、1 个原生错误与 1 个错误后恢复请求：共 29 个 HTTP 请求，
28 次 Session run；原生错误只计一次，最终 queue/inflight/tracked tasks 均为 0。
从真实 `/metrics` 响应提取的六个 family 已通过 Python prometheus-client 的独立 OpenMetrics
解析器，版本与类型见[解析记录](observability/metrics-parser.log)。Python 仅用于开发验收。

CLI 安装 Tracing JSON subscriber，记录静态 outcome、秒耗时和资源数。
子进程输入使用敏感 state/instructions/问题名，并注入含敏感文本及绝对路径的 tokenizer 错误；
捕获 stderr 和错误响应后检查无泄露。实际 ORT Gather 错误继续通过原生脱敏 logger 检查。
原生 CPU vendor 启动提示及 CLI 启动诊断仍可能为普通文本；业务观测事件是 JSON 行。

## 信号与真实进程

[`server`](../../src/server.rs) 被 CLI 和子进程测试共用。收到 SIGINT/SIGTERM 关闭准入，
ready 返回 503，排队请求返回 unavailable；排空期间监听仍可提供 health/ready/metrics。
完成全部 JoinSet 回收后关闭 HTTP 并退出 0。grace 覆盖 HTTP 与真实任务排空；
到期记录剩余 queue/inflight/tracked_tasks 并 `process::exit(1)`，避免 Runtime drop 无限等待
spawn_blocking，也不把尚未结束的 CPU 工作记成 0。

[子进程测试](../../src/server/tests.rs) 每次启动独立测试进程，以 stdin 闭锁控制真实阻塞线程。
两种信号各验证空闲、执行中、带队列正常排空、带队列强制退出、HTTP 已超时后的正常排空和强制退出，
共 12 个进程。检查真实 HTTP 状态、指标、退出码、日志和退出后端口不可连接，并重复启动。
退出 0 时 inflight/tracked_tasks=0；到期退出 1 时二者=1，queue=0。
[Linux 原始记录](observability/linux-signals.log) 保存每次状态转换；这是受控 CPU worker，
并非模型数值验收。测试帮助入口仅 cfg(test) 存在，不给生产二进制增加后门。

[CLI 验证脚本](observability/cli-smoke.sh) 另启动真实 laya-server 和固定 bundle：
long-padding 请求在 TERM 后 grace 内完成并退出 0；执行等待 1 秒的配置返回 504 后仍 inflight=1，
TERM 后 grace=1 秒到期退出 1；随后同端口重启并在空闲时 INT 退出 0。
[CLI 原始记录](observability/cli-smoke.log) 包含脱敏业务日志和退出码。

环境复用 laya-loader-8：Linux ARM64、8 vCPU、12 GiB Docker Desktop，Rust 1.98.1、ORT CPU 1.28.0。
模型沿用 [#13 固定图和权重](postprocess.md)，真实 HTTP 专项 intra-op=2，CLI 专项 intra-op=1；
全部 max-concurrency=1。本票没有改变模型、序列或数值协议。

```sh
rtk cargo test --locked --lib api::tests
rtk cargo test --locked --lib server::tests::signals -- --nocapture
rtk proxy docker exec -w /work laya-loader-8 cargo test --locked --lib server::tests::signals -- --nocapture
rtk proxy docker exec -w /work \
  -e LAYA_TEST_MODEL=/work/models/multilingual -e LAYA_TEST_ORT=/ort/lib/libonnxruntime.so \
  laya-loader-8 cargo test --locked --bin laya-server linux_http_model_parity_and_disconnect -- --ignored --nocapture
```

CLI 脚本要求 `/work/target/observability-request.json` 是 long-padding fixture 的 request_json，
可在宿主用 Python json 模块从 `tests/fixtures/system-one/long-padding.json` 提取。
先 `rtk proxy docker exec -w /work laya-loader-8 cargo build --locked`，再运行：

```sh
rtk proxy docker exec -w /work laya-loader-8 timeout 180 sh docs/validation/observability/cli-smoke.sh
```

## 依赖与检查

新增 tracing-subscriber 0.3.23，仅 fmt/json，关闭默认 ANSI 和 log 桥接；标准库与 tracing facade
不能提供 subscriber，复用 Tokio 官方组件避免手写日志协议。该包 MIT、声明 Rust 1.65，
Linux CPU 实际构建通过。Tokio 增开 signal，复用其 Unix 信号驱动；没有新增模型原生 ABI。
新增锁定包的许可证、MSRV 与归档体积见[依赖记录](observability/dependencies.log)。
valuable 是锁文件中的可选依赖，未进入本次普通构建，未宣称运行时使用。

宿主 `cargo fmt --check`、`cargo clippy --locked --all-targets -- -D warnings` 与
`cargo test --locked` 全部通过：76 passed / 8 ignored，见
[Clippy](observability/clippy.log) 和[完整测试](observability/test.log)。
ignored 中一个是由普通父测试启动的子进程入口；另七个为模型/tokenizer 专项，
本次单独运行了上述真实 HTTP 模型专项，其余六个未逐项重跑。未运行远程 CI。
更改文档的本地链接与 `git diff --check` 通过；依赖摘要、图/权重/ORT 哈希、CPU/内存配额及
Linux 未剥离 debug 二进制大小见[环境记录](observability/environment.log)。

### Standards

评审以任务开始提交 `cc5ff26fd5a7f5eb101499df4cfa93b00490063b` 到提交前工作树为范围。
首次发现测试启动握手先于子进程清理 guard，且没有期限；现已在 spawn 后立即建立 guard，
启动握手限时 5 秒，失败/超时回收进程并清理两个临时文件。
复核后 0 项未解决标准问题，0 项 baseline smell。该轴未重复运行完整测试。

### Spec

首次指出契约开头仍有空 registry 的旧状态描述，已更新。
独立复跑 12 个子进程信号场景通过；源码、模型和 CLI 记录与冻结边界一致。
复核后 0 项未解决规格问题，该轴未重复运行真实模型。

两个评审子代理独立检查后分别复核，最终 Standards 0 项，Spec 0 项。

