# macOS Apple Silicon 便携版

目标为 macOS 14+、Apple Silicon ARM64、CPU 推理。当前实测机器为 macOS 27.0.1；
按 #21 实施会话中的用户指令，本次只要求本机版本验收，**macOS 14 未实测**。
结果及未执行项见[验收记录](validation/macos.md)。不支持 Intel、Rosetta、GPU/ANE，
不安装 launchd、登录项或系统服务。

终端用户只需 macOS 自带 Bash、curl、tar、shasum、plutil、osascript、lsof 等工具。
osascript 只通过系统 JavaScript 读取并校验 JSON，不控制其他应用；不需要 jq、Homebrew、
Xcode、Rust、Python、Node.js 或 Docker。开发期构建和验收使用的 Python 不进入运行链路。

## 准备与启动

在[项目 Releases](https://github.com/redwolf2019/laya-rs/releases)选择一个实际包含 macOS
资产的版本，下载 `laya-macos.sh`、`release.env` 和 `SHA256SUMS`。
先用 `shasum -a 256 laya-macos.sh release.env` 对照同次发布的 `SHA256SUMS`。
清单与资产同源，这只是完整性校验，不是独立签名；不要执行下载了一半的脚本。
[v0.1.4](https://github.com/redwolf2019/laya-rs/releases/tag/v0.1.4) 已包含 macOS 程序包。
最终包在本机 macOS 27.0.1 验收；macOS 15 CI 的回退检查未通过，详情见[发布验收](validation/release-v0.1.4.md)。
未包含 macOS 包的旧版本会明确失败。

```sh
bash ./laya-macos.sh install
bash ./laya-macos.sh start
```

`install` 解析 GitHub 最新稳定版并固定本次 tag、大小和 SHA-256，然后准备程序、模型、
配置和密钥；不启动服务。也可用 `--version <实际发布的 tag>` 固定版本。
所有下载只走项目 GitHub HTTPS Releases，重定向保持 HTTPS，连接和传输均有超时。
在私有临时目录中先检查大小、哈希、归档路径和类型，再解包；拒绝链接、特殊文件和目录穿越。
预检查暂存卷至少可容纳下载文件加 8 GiB、程序卷至少有 4 GiB 空间，保留升级回退余量。

`start` 验证文件后启动子进程，确认 PID 归属、监听端口、readiness、鉴权、四个路由、
六个指标和中文 Choice/Score/Noul 推理。它留在当前终端等待进程退出，日志在 `run/<实例>/server.log`。
不要把它当作后台服务。在另一个终端停止：

```sh
bash ./laya-macos.sh stop
```

`stop` 只向本部署记录的进程发送 SIGTERM，等待 grace 加 15 秒；不按名称或端口杀进程，
不自动 SIGKILL。归属不符、PID 复用或无法确认退出会中止操作。
程序路径、用户、启动时间和部署实例工作目录须同时匹配。
Ctrl+C、SIGTERM 和 Shell 收到的 HUP 都会转发终止信号并回收自己创建的子进程。
应用的 Unix SIGINT/SIGTERM 共用 drain：关闭准入、排队结束为 unavailable，等待真实 CPU 工作；
正常排空 exit=0，grace 到期非零。关闭 Terminal、注销或关机可能没有足够时间排空；
休眠期间服务可能不可达，也不保证 HTTP deadline。SIGKILL、断电不能保证优雅退出。

## 目录、配置与密钥

默认根目录为 `$HOME/Library/Application Support/laya-server`：

```text
programs/<版本>-<包哈希前缀>/  程序、dylib、许可证、构建与逐文件校验记录
models/<model-id>/            固定模型缓存，损坏时另建目录，不覆盖旧目录
current.env / previous.env    当前和前一版配置（键值数据，不作为 Shell 执行）
token                        用户私有 Bearer 密钥
run/<实例>/server.log         本次进程日志
process.env                  PID、用户、启动时间、程序路径、实例标识
transaction/                 仅在切换未完成时存在；含旧/新配置及原运行状态
operation.lock/owner          部署操作锁及持有者信息
```

首次准备从系统随机源生成 32 字节密钥，保存为 64 位十六进制。目录为 0700，密钥为 0600，
移除这些私有对象的继承 ACL；仅目标服务子进程得到 `LAYA_API_TOKEN` 环境变量。
启动、升级、回退复用原密钥；已有部署密钥丢失时拒绝启动，不悄悄重建。
日常操作不会显示密钥，需要给客户端配置时，本机自行读取：

```sh
cat "$HOME/Library/Application Support/laya-server/token"
```

`install` 可覆盖 `--root`、`--program-directory`、`--model-directory`、`--listen`、
`--threads`、`--inter-op-threads`、`--max-concurrency`、`--shutdown-grace`。
目录使用绝对路径并加引号；支持中文、空格、单引号，不接受换行等控制字符。
程序目录必须为本部署独占的空目录或已有管理目录；显式模型目录必须已经存在并通过固定校验，
只读取，不修改它。其余命令传同一 `--root` 即可；这些配置会随升级和回退保留。
默认监听 `0.0.0.0:8080`，参数为 threads=2、inter-op=1、slots=1、grace=120 秒；
不修改全局 PATH、系统目录、防火墙，也不需要 sudo。服务使用明文 HTTP，部署在受控网络内。

需要改配置时先 `stop`，备份并编辑 `current.env` 的数据项，再 `start`。
不修改程序、模型标识或路径来绕过校验；不要把密钥放进配置或命令行。
直接执行二进制继续使用现有 CLI 和 `LAYA_API_TOKEN`；通用 CLI 默认线程/并发不变。
macOS 默认从可执行文件旁加载 `libonnxruntime.dylib`，可用 `--ort-library` 覆盖，
不依赖当前工作目录或 `DYLD_LIBRARY_PATH`。

## 升级、回退和中断恢复

```sh
bash ./laya-macos.sh update
# 或 update --version <实际发布的 tag>
```

更新锁内先准备候选包并验证模型缓存；下载或校验失败不停止旧版。
相同 model-id 的有效模型直接复用，不重复下载。旧配置和运行状态写入事务目录后，
才停止旧实例；新程序绝不原地覆盖旧程序。候选必须通过真实三类型 API 检查才原子替换
`current.env`，并保留 `previous.env` 与旧程序。

候选失败时重新启动旧版，并再次通过真实 API 检查才打印恢复成功。
旧版原来停止，则验证后停止；原来运行，则恢复后的进程由当前更新终端承载。
运行中的升级（包括回退）会停留在终端；回退后最终命令退出码为 1。
恢复失败会单独报告并保留事务和日志，不报成功。

中断或断电留下 `transaction/` 时，后续普通操作会拒绝继续。查看其中的旧/新路径及
`run/*/server.log`，确认程序和密钥保留后执行：

```sh
bash ./laya-macos.sh update --recover
```

该命令只恢复事务中的旧配置及原运行状态，不下载新版本。
正常捕获信号会回收本次子进程并释放操作锁。SIGKILL/断电可能留下陈旧锁：
读取 `operation.lock/owner`，用 `ps -p <PID> -o uid=,lstart=,command=` 检查持有者。
只有确认原操作已结束，才手动将该锁目录改名保留为备份，再执行恢复。
脚本从不自动删除未知锁或接管可疑 PID。不要在仍运行的部署操作中移走锁。

本地发布候选可用 `--release-directory /绝对路径` 测试；目录须包含 `release.env`、程序包，
无可用缓存时还须有对应 `model-<id>.tar.gz`。验证规则与 HTTPS 下载相同。

## 未公证分发

发布包的可执行文件和 dylib 使用有效 **ad-hoc** 签名，没有 Developer ID 身份认证，
也没有 Apple 公证。程序包内 `macho-signatures.txt` 记录每个文件的签名和依赖。
浏览器下载可能附带 quarantine，首次运行及后续换版本都可能再次接受系统检查。

Apple 提供的手动流程是：先尝试打开具体文件，再到“系统设置 → 隐私与安全性”对该文件
选择“仍要打开”，核验提示后确认。来源见 [Apple 操作说明](https://support.apple.com/en-gb/102445)。
程序和 dylib 的本机实际检查结果见[验收记录](validation/macos.md)；这里不将官方通用流程
当作所有 macOS 版本上已经通过的部署测试。遇到损坏或签名无效提示应重新核验候选，不能绕过。
脚本不关闭 Gatekeeper、不修改安全策略、不自动移除 quarantine。

## 开发与发布

官方 macOS ORT 1.28.0 包枚举到 WebGPU device，无法通过现有 CPU-only 启动检查。
因此发布依赖单独使用固定 `v1.28.0` / `da9b5e364c465de65c49d91e696cd6485270757f`
源码及固定子模块构建；不用 CoreML、WebGPU、XNNPACK，保持 CPU Session 配置和数值阈值。

```sh
bash tools/release/prepare-ort-macos.sh /绝对路径/ort-source /绝对路径/cpu-ort
python3 tools/release/build-macos.py --version <候选 tag> --ort /绝对路径/cpu-ort
```

开发机需要 Rust 1.98.1、Xcode/CLT、SDK、CMake、Python 3、Git；普通运行用户不需要。
构建固定 `MACOSX_DEPLOYMENT_TARGET=14.0`，原生 ARM64；记录 Rust/Xcode/SDK/ORT、
源码 hash 和依赖配置。依赖取自 ORT 固定清单，禁用 CMake 对用户安装包的自动替代。
应用构建与启动不调用 ORT 构建器。打包检查 Mach-O 架构、最低 OS、动态依赖闭包并验证签名；
系统 framework/libSystem/libc++ 不复制入包。不为库标识添加无用别名，直接路径加载已通过验证。

发布新增 `laya-server-macos-aarch64.tar.gz`、`laya-macos.sh` 和 `MACOS_aarch64_*` 元数据；
复用现有独立模型包，不改变 Linux/Windows 资产。CI 核验 runner 的实际架构/OS/SDK，
普通 CI 不带权重；release job 运行真实模型、HTTP、生命周期及升级回退后只创建 draft。
v0.1.4 的 release workflow 因 Windows/macOS CI 失败未自动创建 draft；本次按用户已确认的平台范围，
使用原生 Linux CI 产物和两台实测主机的 Windows/macOS 产物手动发布，未将失败步骤标记为通过。
