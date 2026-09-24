# HTTP 路由与真实模型验收（#16）

日期：2026-09-24。来源：[任务 #16](https://github.com/redwolf2019/laya-rs/issues/16)、
[冻结契约 §2.1、§7](../compatibility.md)、[engine 验收](engine.md)、[调度验收](scheduler.md)。

`[已验证/HIGH，以下测试及 Linux ARM64 固定请求范围]`
[`api`](../../src/api.rs) 接通四个路由，CLI 在全部模型资源校验成功后绑定监听地址。
所有请求共享加载时建立的 Session；HTTP handler 不加载模型，也不伪造答案。
ready 读取调度器的真实准入状态，忙碌不使 ready 失败；关闭准入后返回 503。
`/metrics` 编码 prometheus-client 空 registry，响应为 OpenMetrics 1.0 文本 `# EOF\n`，
不注册虚构的计数。完整指标、结构化请求日志及信号/grace 退出属于 #17。
当前进程收到操作系统终止信号会直接退出，不是优雅退出验收。

## 路由与取消

输入校验复用 `Request::from_slice`；读取前使用
[Axum 0.8.9 的 `to_bytes(body, limit)`](https://docs.rs/axum/0.8.9/axum/body/fn.to_bytes.html)
限制流，不根据 Content-Length 放行。就绪、媒体类型、流字节、JSON/字段和准入按契约顺序检查。
仅接受 `application/json`，所有 charset 参数均须为 UTF-8；`+json` 和其他编码返回 415。
未知路径返回空 404，错误方法返回空 405/Allow，GET 路由支持 HEAD。

[`api::tests`](../../src/api/tests.rs) 的 14 项测试覆盖八错误状态/code/message、完整 DTO 转发、
重复请求、body 恰好上限与多一字节、流错误、字段/数量/深度/Unicode、媒体类型、未知路由、
忙碌和关闭时的 ready，以及读取期间关闭准入。闭锁和暂停时钟验证队满、两个超时、
排队 future 取消、运行 future 取消、晚到完成及所有者回收；被取消的排队项不执行。

[`TCP 测试`](../../src/api/tests/network.rs) 使用实际 Axum HTTP/1 连接。
没有终止 chunk 的超限请求仍收到 413；完整请求发出后关闭 TCP，已排队的请求被移除。
运行中的阻塞任务仍持槽，只有闭锁释放并收到结果后才归零。
这些结论限于本次 HTTP/1 配置与测试时序，不声称任意 TCP 断开都会取消 handler，更不声称停止 CPU。

## 真实 Linux 模型与 CLI

环境复用 `laya-loader-8`：Docker Desktop Linux ARM64，8 vCPU、12 GiB，无额外 swap；
Rust 1.98.1、ORT CPU 1.28.0、固定 #13 图及权重，每 Session intra-op=2/inter-op=1，实际槽数 1。
[environment.log](http/environment.log) 保存容器、依赖树、图/权重/ORT/Cargo.lock 摘要。

[`linux_http_model_parity_and_disconnect`](../../src/model/tests/http.rs) 先由同一加载资源计算
21 个固定请求的 engine 响应，再通过真实 TCP 请求路由。HTTP 与同输入 engine 的所有字段
精确一致；官方四位字段、Choice、usage 精确一致，action 使用原容差
`abs <= 1e-5 + 1e-4*abs(reference)`。随后四客户端并发重复 mixed-6，全部通过；
执行期间未重载模型。请求涵盖中英三类型、混合 batch、长输入及 JSON 数字/顺序。

同一专项还通过 1 MiB chunked 超限（未发送终止 chunk），在实际工作期间关闭连接，
观察断开时 inflight=1，随后 owner 收到完成并回收，后续请求恢复正常。
测试专用 tokenizer 添加一个未训练 token 触发真实 ORT Gather 失败，HTTP 返回静态 500；
正常请求随后仍使用同一 Session 完成。该注入仅在测试中，生产 tokenizer 保持 manifest 原样。
最后等待/inflight/已跟踪任务均为 0。原始输出见 [model-http.log](http/model-http.log)。

```sh
rtk proxy docker exec -w /work \
  -e LAYA_TEST_MODEL=/work/models/multilingual \
  -e LAYA_TEST_ORT=/ort/lib/libonnxruntime.so laya-loader-8 \
  cargo test --locked --bin laya-server linux_http_model_parity_and_disconnect -- --ignored --nocapture
```

独立 CLI 进程也已启动并由 curl 检查三个 GET 路由；两次 mixed-6 请求通过完整官方响应比较。
结果见 [cli-curl.log](http/cli-curl.log)。开发容器额外安装 curl，仅作验证工具；服务不调用它。
从零准备模型/ORT 沿用 [engine 命令](engine.md#从零准备并运行)。现有环境中，分别在两个终端运行：

```sh
rtk proxy docker exec -w /work laya-loader-8 cargo build --locked
rtk proxy docker exec -w /work laya-loader-8 /target/debug/laya-server \
  --model /work/models/multilingual --ort-library /ort/lib/libonnxruntime.so \
  --listen 127.0.0.1:18080 --threads 2 --max-concurrency 1
```

```sh
rtk proxy docker exec laya-loader-8 curl -fsS http://127.0.0.1:18080/readyz
rtk proxy python3 -c 'import json; print(json.load(open("tests/fixtures/system-one/mixed-6.json"))["request_json"])' \
  | rtk proxy docker exec -i laya-loader-8 curl -fsS http://127.0.0.1:18080/v1/system-one \
      -H 'Content-Type: application/json' --data-binary @-
```

## 脱敏与依赖

HTTP 不记录正文、问题名、路径或原异常；请求/调度错误仅用原静态 envelope。
验收曾复现 ORT 默认 logger 把 Gather 底层异常直接写入 stderr，新增专项断言因此失败。
加载器改用 ort 固定版本的 `EnvironmentBuilder::with_logger`，仅记录静态事件名和 typed severity；
原始 category/id/location/message 不进入日志。回归已捕获原生错误事件并检查全部字段，
最终日志没有 `Non-zero status code` 原始异常。启动错误仍保留 typed source，Display 使用静态原因。

本次引入 Axum 0.8.9（MIT，声明 Rust 1.80），仅启用 http1/json/tokio；标准库不能提供项目所选
Axum 路由与 HTTP 协议栈。http-body-util 复用同一传递依赖来区分超限与读取失败。
prometheus-client 0.25.0（MIT OR Apache-2.0，默认无 feature）提供真实 registry 与编码器，
避免手写指标协议；它是 [Prometheus 官方 Rust 客户端](https://github.com/prometheus/client_rust)。

媒体类型使用 [mediatype 0.23.0](https://docs.rs/mediatype/0.23.0/mediatype/)（MIT，关闭默认 feature，
无运行时传递依赖）。原尝试复用的 mime 0.3.17 在 `charset="UTF-8"; profile=test` 的参数索引
上返回错误值，导致合法请求 415；同一回归用例现已通过。没有手写 MIME 解析器。
prometheus-client/mediatype 未声明 MSRV，以本次 Rust 1.98.1 的宿主和 Linux 实际构建为依据。

Tower util/futures-util 仅作测试工具，Tokio 测试增加 io-util；这些包原已属于 Axum 传递依赖。
相对任务基点，Cargo.lock 增加 37 个包，所有新增包的许可证/声明工具链和包归档大小见
[dependencies.log](http/dependencies.log)。归档总量 4,291,000 字节，含未在 Linux 使用的 Windows 包；
Linux 未剥离 debug 二进制 91,799,944 字节，不是 release 镜像体积。没有新增原生模型 ABI 或 GPU 依赖。

本票没有验收 Linux amd64、裸机吞吐、稳态性能或优雅退出。普通 CI 的模型专项仍显式忽略；
绿色 CI 不能替代以上单独执行的真实模型记录。

## 最终检查与双轴评审

宿主 `cargo fmt --check`、`cargo clippy --locked --all-targets -- -D warnings` 和
`cargo test --locked` 通过；完整测试为 71 passed / 7 ignored，原始输出见
[clippy.log](http/clippy.log)、[test.log](http/test.log)。新增 Linux HTTP 专项单独执行通过；
其他六个依赖模型/tokenizer 的既有专项本次未逐个重跑。
本地 Markdown 引用、函数长度与 `git diff --check` 通过；未运行远程 CI。

完整测试曾在默认并行运行下暴露既有日志观察测试失败（期望 1 次事件，实际 0 次），
单独与串行运行通过。临时探针观察到发出事件时 scoped subscriber 仍存在，但日志已被禁用。
核对 Cargo.lock 固定的 tracing-core 0.1.36 `callsite.rs` 中 `Rebuilder::JustOne`：
仅登记一个 scoped dispatch 时，另一线程首次使用共享调用点会按其 NoSubscriber 缓存 Never。
测试现额外登记并持有一个 NoSubscriber dispatch，使缓存计算覆盖这两种实际观察环境；
原断言及生产日志逻辑未改。清除探针后，20 次默认并行库测试各 24 项通过，见
[parallel-regression.log](http/parallel-regression.log)，随后完整套件通过。

评审基点为任务开始时的 `35f37535cd44d72a102351b4183f16986e3a4713`，
按 implement 要求，由两个子代理分别审阅提交前工作树的标准与规格。

### Standards

首次发现服务启动路径丢失 I/O/JoinError 来源，已由 `ServiceError` 保留 typed source，
Display 仍为静态原因。复核实现、测试修复、依赖和最终记录：0 项未解决违反，0 项 baseline smell。
该轴另行运行过 14 项 HTTP 测试；最终复核没有重复执行完整套件或模型。

### Spec

首次发现媒体类型沿用 Axum 宽规则而接受 `+json`/非 UTF-8 charset，违反原冻结契约。
新增拒绝测试先复现失败后修正实现，同时保留大小写、引号及其他合法参数的接受测试。
复核确认契约、代码和验收记录一致，0 项未解决发现；该轴没有另行重跑模型。
