# macOS ARM64 验收（#21）

日期：2026-10-12。来源为下列可复现命令、原始日志和本机二进制检查；置信度 HIGH。
用户在实施会话中明确要求“只用通过本机 MAC 版本的验证”，并同意浏览器分发验收记为未完成。
因此本次验收范围是 **macOS 27.0.1 / ARM64**；最低构建目标仍为 14.0，macOS 14 未实测。
未发布远端资产，也不把 CI 配置或 ad-hoc 签名当作 Apple 公证。

## 环境与固定产物

- Apple M2 Max，64 GiB，宿主机原生运行，无 Rosetta、无 macOS VM。
- macOS 27.0.1 build 26A434；Xcode 27.0 build 27A266a；SDK 27.0。
- Rust 1.98.1，`ort 2.0.0-rc.13` / API 28，tokenizers 0.23.2，Cargo.lock 未改动。
- 基线源码 `424978ade2ad3300e76f1ea7efb396e49782b389` 加本次工作树；包内
  [build.json](macos/build.json) 记录构建时每个 Rust 源文件及 Cargo 文件 SHA-256，避免将未提交修改误记为基线版本。
- 候选 `v0.1.1-macos21`，程序包 SHA-256
  `834b52aeb8bb82564940be32e83e21e406c888638997c130bffbc1ba110d6ecb`。
- 固定模型 ID `3bff67d421ae27d4`；程序、dylib、manifest、模型归档的大小和完整哈希见
  [artifacts.json](macos/artifacts.json)。环境原始输出见 [environment.log](macos/environment.log)。

## ORT、Mach-O 与签名

官方归档 `onnxruntime-osx-arm64-1.28.0.tgz` 的 SHA-256 为
`1268b359718099bde2cedb55787f182a130067bc4f31e8c88478c445b850d3d8`。
实际 provider 为 CoreML、WebGPU、CPU，device 为 CPU、WebGPU；现有启动校验拒绝它。
见[官方枚举](macos/official-providers.log)和[失败的真实启动测试](macos/official-ort-rejected.log)。
没有删 provider 检查，也没有改 ORT 版本。

使用 `tools/release/prepare-ort-macos.sh` 从 ORT
`da9b5e364c465de65c49d91e696cd6485270757f`（v1.28.0）构建 CPU-only dylib。
固定子模块、`cmake/deps.txt` 哈希和完整 CMake 配置见 [ort-build.txt](macos/ort-build.txt)。
本机首次构建误选 Android SDK 的 FlatBuffers 25；设置
`FETCHCONTENT_TRY_FIND_PACKAGE_MODE=NEVER`、禁用用户包注册表后，使用 ORT 固定依赖完成构建。
应用正常构建和运行不调用这个依赖准备器。

[CPU 枚举](macos/cpu-providers.log)中 available provider 和 device 均只有 CPUExecutionProvider。
Session 仍明确注册 CPU、关闭环境 provider 注入，保持原图优化设置。
[Mach-O 与签名记录](macos/macho-signatures.txt)显示应用和库均为 ARM64、minos 14.0，
依赖仅系统 Framework、libiconv、libc++、libSystem 等；没有 Homebrew/构建目录依赖。
库的 install name 为 `@rpath/libonnxruntime.1.dylib`，按绝对路径实际加载成功，无需修改该标识。
两者的 ad-hoc 签名均通过 `codesign --verify --strict`，不代表开发者身份认证或公证。

## 已执行的推理与退出检查

| 检查 | 结果及原始输出 |
| --- | --- |
| fmt、clippy、locked 全套测试 | 85 passed、8 ignored；[完整输出](macos/rust-checks.log) |
| CLI 路径、帮助及原有输入边界 | 在全套测试中通过；macOS 默认 executable directory/libonnxruntime.dylib |
| 固定张量、两个 Session、intra=2/inter=2 | [tensor-smoke.log](macos/tensor-smoke.log) |
| 21 个固定请求 | 序列/marker/mask/usage 精确、原 logits/概率容差、官方舍入和 Choice 精确；[model-parity.log](macos/model-parity.log) |
| 真实 HTTP、并发重复、TCP 断连、失败后恢复 | 四路由、鉴权、六指标及任务回收；[http-parity.log](macos/http-parity.log) |
| N=1/N=2 真实 Session 并行 | 分别观测最大并行 1/2，结束后队列、槽和任务归零；[N=1](macos/concurrency1.log)、[N=2](macos/concurrency2.log) |
| SIGINT / SIGTERM | 各自空闲、在途排空、HTTP 超时后 CPU 仍运行三场景；[lifecycle.log](macos/lifecycle.log) |
| 排队与 grace | 队列满 429、队列超时 503；退出时排队为 unavailable；HTTP 504 后 inflight 仍为 1；正常 drain exit=0、grace 到期 exit=1；退出后可重绑端口 |
| Shell/归档/元数据 | 非法配置、未启动的 install、私有密钥、归档穿越/绝对路径/软硬链接/FIFO、损坏下载、陈旧锁、配置不执行、quarantine 属性保留；[script-checks.log](macos/script-checks.log) |
| Linux 回归 | Docker Linux x86_64（本机 ARM64 上仿真）、locked 无模型测试通过；[linux-tests.log](macos/linux-tests.log)。不是 Linux 原生推理/性能重验 |

生命周期脚本为 [lifecycle.py](macos/lifecycle.py)，读取的是实际 HTTP 和进程退出状态。
六个原始应用日志保存在同目录的 `idle-*`、`drain-*`、`timed-*` 文件。
数值断言未放宽；沿用历史名称 `linux_*` 的真实测试当前也用于 macOS/Windows，名称不表示执行平台。

## 便携部署与故障恢复

[deployment.log](macos/deployment.log)通过 `smoke-macos.py` 在中文、空格、单引号路径中执行
真实安装、首次启动、停止、A→B 更新和回退，子命令输出保存在 `deployment-command-*.log`。
程序、模型分离；显式复用仓库模型目录，更新候选目录不附模型归档，依旧通过逐文件验证。
第一次未指定模型目录的准备也已实际解包并验证独立模型资产；最终完整门禁使用显式模型目录。

- 重复 start、其他进程占端口、伪造为测试 Python PID 的记录均被拒绝，未接管其他进程。
- 初次启动期间 SIGTERM 回收子进程，保持配置；有效升级保留监听、线程、grace、模型和 token。
- 对同一源码候选分别写入损坏 dylib、把 smoke 请求改成 `{}`、编译 x86_64 测试 dylib，
  并重新生成测试包校验清单；三种候选都失败，旧版重新通过三类型真实推理后才报告恢复成功。
  每个故障候选的版本、SHA-256 和实际恢复结果均在日志中。
- 更新正在切换时，第二个真实 update 因锁拒绝；SIGTERM 中断首个更新后回收候选、保留事务。
  普通 start 拒绝未完成事务；`update --recover` 恢复原配置、运行状态并重验旧版真实推理。
- 原来停止的实例更新后仍停止；全部流程后 token 哈希与模型文件 mtime 保持一致。
- 7 个无模型命令边界测试另覆盖权限不足、陈旧 PID/锁和下载中断。
  下载中断用本地阻塞 HTTPS CONNECT（不关闭 TLS 校验）模拟未完成网络传输：先复现父 Shell
  迟迟不退出，再改为跟踪 curl 子进程并通过可中断 wait 等待；SIGTERM 后 3 秒内退出、
  socket EOF、旧配置/token 保留、锁和暂存释放均有断言。这不是公网发布下载验收。

脚本显式保留本地候选归档的 quarantine 到暂存及解包文件；这只通过属性边界测试，
不替代用户已同意暂不完成的浏览器手动许可验收。最后增量对下载 PID 增加父进程检查，
目标下载测试再次通过；完整真实部署日志记录所执行脚本的哈希。
包重新构建后解包验证签名通过，app/dylib 字节与上面的已验收候选一致，见 [package.log](macos/package.log)。
两轴评审结果见 [macOS 评审](macos-review.md)。

资源采样不是专用 benchmark：1 槽 loaded RSS 943056 KiB、2 槽 1443920 KiB；
8 个压力请求总耗时分别约 2.975/1.524 秒，CPU 时间和 RSS 原始值在并发日志。
单槽 threads=1 的直接 CLI 启动就绪约 1.49–1.58 秒；后台可能有其他构建/测试，
这些本机数字不外推到 macOS 14、其他芯片、干净用户或生产流量。

## 复现

先按 [macOS 说明](../macos.md)构建 ORT 和程序包。普通用户脚本只使用系统工具；
下面的 Python 仅属于开发验收环境。

```sh
export MACOSX_DEPLOYMENT_TARGET=14.0
export LAYA_TEST_MODEL="$PWD/models/multilingual"
export LAYA_TEST_ORT=/绝对路径/cpu-ort/libonnxruntime.dylib
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
for name in linux_real_model_smoke linux_system_one_model_parity linux_http_model_parity_and_disconnect; do
  cargo test --release --locked --bin laya-server "$name" -- --ignored --nocapture
done
for slots in 1 2; do
  LAYA_TEST_CONCURRENCY=$slots cargo test --release --locked --bin laya-server linux_scheduler_concurrency -- --ignored --nocapture
done
python3 scripts/test-macos.py
python3 tools/release/test-package.py
python3 tools/release/smoke-macos.py --release-directory /绝对路径/候选目录 --root "/绝对路径/中文 mac's test"
python3 docs/validation/macos/lifecycle.py --program /绝对路径/解包目录 --model "$LAYA_TEST_MODEL" --out /绝对路径/日志
```

## 未执行与明确边界

- macOS 14 实机/VM：按用户最新指令不纳入本次验收；部署目标不等于实测支持。
- 浏览器分发、quarantine 后的系统手动“仍要打开”：Chrome 下载本地候选时返回
  `ERR_BLOCKED_BY_CLIENT`，未绕过拦截；用户同意本项记为未完成。属性保留边界测试不能替代该验收。
- 全新 macOS 用户、物理断电、注销/休眠/关机：未执行；当前为已有开发机的普通用户，
  脚本强制使用系统 PATH。不能将此描述为已通过无 CLT 的干净系统验收。
- 未运行远端 CI，未创建/发布 GitHub Release；没有重新执行 Windows 11 实机验收。
  Windows 代码未改，发布元数据兼容性测试覆盖三平台；原 Windows 证据见 [Windows 记录](windows.md)。
