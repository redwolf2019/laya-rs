# Windows 11 x64 验收

本次按用户指令将 #20 最低目标改为仅 Windows 11 x64。结果置信度 HIGH，范围限
2026-10-12 授权的 7900XTX 主机上的原生 CPU 进程；显卡未参与推理。
2026-10-12 用户同意暂缓普通用户、干净环境/VC-UAC 及公开候选验收，
#20 按 Windows 11 实现和本记录的实测范围收口。暂缓项见文末；
本记录不代表所有 Windows 11 版本或完整部署验收。

## 环境与候选

| 项目 | 实际值 |
| --- | --- |
| 系统 | Windows 11 专业工作站版，10.0.26300，build 26300，x64 |
| CPU / 内存 | Intel Core i5-13600KF；系统报告 33304628 KiB 可见内存 |
| 系统硬件报告 | ASUS / System Product Name；HypervisorPresent=true，未据此推断具体虚拟化用途；原生 MSVC x64，未使用 Wine 或架构模拟 |
| 源码 | 基线 `e9bbb22c81579845c9c8ea25951d6bc9aaf3e101` 加本提交的运行时变更；开发候选在提交前构建，非发布 tag |
| Rust / MSVC / SDK | 1.98.1 (`48a229cea`)，14.44.35207，10.0.26100.0 |
| ort / ORT / tokenizer | 2.0.0-rc.13 / 官方 CPU 1.28.0 / 0.23.2，Cargo.lock 不变 |
| VC++ x64 | 已安装 14.44.35211.0，包内 VC_MIN 同值 |
| 参数 | 脚本 intra=2、inter=1、slots=1、grace=120；对照入口另测 slots=2；生命周期专项 grace/HTTP timeout=1 |

候选 `v0.1.2-candidate`：

- 程序 ZIP：9,290,375 bytes，SHA-256
  `d28a5f51d68276fc6c603eed8418cf179809a57910c0f6cc18077ea72fe924fe`。
- 固定模型：`3bff67d421ae27d4`；开发期重打包 tar.gz 752,018,537 bytes，SHA-256
  `699aae05cab2f36792859f59c533280daf8bcb96c57fae21d30850fc7ad8dfe5`。
  模型每个文件继续按 [固定 manifest](../model-manifest.json) 校验，未重新导出或修改权重。
- 官方 ORT ZIP 核验 SHA-256
  `abef733dacbe2f571547a7150b479b5cb9cc0df22f96c24983a42cadb1b4f8bc`。

ZIP 内有 exe、两份官方 DLL、文件大小/hash 清单、模型标识、版本、构建信息、
`dumpbin /DEPENDENTS` 记录及项目/Rust/crate/ORT 许可证。依赖闭包为 Windows 系统 DLL
和 VC++ 系统运行库；不分发 Linux loader 或 glibc。完整 [打包日志](windows/package.log)。
该开发主机使用源码快照而非 Git checkout，`build.json.Source` 明确记录
`uncommitted-source-snapshot`；包内 `source-files.json` 的 24 个源码/工具链文件 hash
与本地一致，436 个程序文件大小/hash 已逐项校验。正式 Git 构建记录实际 checkout HEAD。

## 已执行

日志仅统一为 LF 并移除行尾空白，保留 stdout/stderr 和 CLIXML 内容。

| 检查 | 证据与范围 |
| --- | --- |
| 编译、fmt、clippy、全量无模型测试 | Windows 81 passed / 8 ignored；21 对照、HTTP、并发和 Tokenizer 另行执行，见下列专项。[原始日志](windows/rust-checks.log) |
| 固定模型与 HTTP | 21 请求序列、marker、mask、usage 精确一致；logits、未舍入概率和 action 使用原容差；舍入字段和 Choice 精确一致。[模型/HTTP/资源日志](windows/model-parity.log) |
| Tokenizer 与并发 | 三项固定 tokenizer/budget/最大请求检查通过；N=1/2 各 8 次真实 native run，实际最大重叠恰为 1/2，资源和任务回收为零。证据同上 |
| 独立 inter-op=2 探针 | `linux_real_model_smoke` 单独执行通过：两个可复用 CPU Session，intra=2/inter-op=2；特殊 token IDs、tensor-L2 logits 和 act_probs 使用原参考断言与容差；1 passed / 0 failed，2.36 秒。[原始日志](windows/interop2.log) |
| 进程级控制事件 | Ctrl+C / Ctrl+Break / 用户停止文件覆盖 idle、queue、inflight、HTTP timeout 后的 blocking work，exit=0/1 与日志计数检查通过。无模型部分明确使用受控 worker，证据在 Rust 日志 |
| 真实 CPU drain/grace | 16 个长 Noul 的实际 CPU 工作：排队返回 503；正常排空 exit=0；HTTP 504 后仍 inflight=1，文件/Ctrl+C/Ctrl+Break 后 grace 到期 exit=1；端口可重新绑定。[入口](windows/lifecycle.ps1)、[日志](windows/lifecycle.log) |
| 管理脚本 | 中文、带空格路径，默认 DLL 位于可执行程序目录、模型使用显式路径，不同工作目录；首次/重复 install/start/stop/update、重复 Start 保持旧 PID、A→B current/previous 指针、无模型归档候选复用外部模型、停止状态保持、模型/密钥不变、假 PID 拒绝、下载校验失败不停止旧进程。[完整日志](windows/deployment.log) |
| 真实回退 | 故障候选重新生成可信本地 hash 清单：用 1-byte DLL 注入启动失败，用 `{}` smoke 注入真实 API 验收失败。两次均启动旧版再真实推理验证；显式事务恢复通过。候选 hash 同日志 |
| 边界 | ZIP/tar 穿越、绝对路径、ADS、链接/PAX、重复元数据、损坏资产、操作锁、未完成事务、PS 5.1 空 opset key、VC++ 不足且用户拒绝。[入口](../../scripts/test-windows.ps1)、[日志](windows/script-boundaries.log) |
| Linux / macOS 无模型回归 | Linux x86_64 Docker（ARM64 主机上的架构模拟）fmt/clippy/83 tests 通过；macOS fmt/clippy/83 tests 通过；Linux 安装器边界、Windows 发布元数据保持 Linux 字段检查通过。未借此声明 Linux 性能或新模型对照 |

2026-10-12 补验独立 `linux_real_model_smoke` 通过，本次 SSH 连接成功。
运行前确认远程 24 个源码/工具链文件 hash 与候选的 `source-files.json` 一致，
使用候选包的官方 ORT DLL 和同一固定模型，未修改运行时代码、权重或参考容差。
复现时按部署的 `current.json` 设置 `LAYA_TEST_MODEL` 为 `Model`、
`LAYA_TEST_ORT` 为 `Program` 目录内的 `onnxruntime.dll`，运行：

```powershell
cargo test --release --locked --target x86_64-pc-windows-msvc --bin laya-server model::tests::linux_real_model_smoke -- --ignored --exact --nocapture --test-threads=1
```

Windows 默认 ORT 图优化曾使 `long-padding` Score 为 `4.5306`，参考要求 `4.5307`。
关闭 Windows Session 图优化后，完整 21 请求及 HTTP 对照通过；保留原权重、容差和
Linux 设置。只验证了整体图优化开关的影响，未确定其中哪一个 optimizer 引起差异。
不将优化关闭后的 Linux benchmark 作为 Windows 承诺。

最近一次缓存安装验证约 17.9 秒，冷启动加三类型验收约 6.3 秒，见部署日志。
一槽代表进程 working set 约 617 MB、峰值约 626 MB；N=2 mixed-6 峰值约 1.18 GB。
这是该主机、请求和参数的观测，不是内存上限或延迟/QPS 保证。

## 暂缓验收与任务收口

按用户 2026-10-12 指令，下列检查暂缓，不作为本次 #20 的关闭条件：

- 普通用户执行四个动作，以及另一普通用户读取密钥、篡改部署状态的实际访问检查。
- 干净 Windows 11 无开发工具部署，以及 VC++ 实际缺失安装、UAC 取消、安装失败和重启要求。
- 从公开候选入口下载最终资产并按同一哈希复验。

这些项目仍标为未执行；已有验收入口和失败记录保留，供后续补验。

普通账户及另一用户 ACL 实际访问尚未验收。远程会话中的临时普通账户任务
未产生验收输出，等待后仍为 `Ready`、result `267011`，检查非零退出；
[原始失败日志](windows/permissions.log) 保留该限制，临时账户和任务已清理。
[权限入口](windows/permissions.ps1) 改为在另一普通用户的交互登录会话中执行，
该入口本次未运行；不以管理员会话的 ACL 静态检查替代实际访问验证。

当前主机原有 VC++ 运行库，Windows Sandbox 未启用；本次没有重启或更改该系统功能。
拒绝安装已通过公开脚本检查，不能用该分支检查声称 UAC 矩阵已验证。
Windows Server CI 只证明相应 runner；正式 workflow 保持 draft，不自动声明 Windows 11 支持。
#20 按调整后的实现范围收口，关闭不代表上述暂缓验收已通过或候选已正式发布。

复现步骤与执行策略见 [Windows 操作说明](../windows.md)。模型环境变量入口保留历史
`linux_*` 测试名，以免破坏 Linux 验收脚本；这些通用测试已移除 Linux-only 假设。
独立评审的两个轴、发现的问题及修复见 [评审记录](windows-review.md)。
