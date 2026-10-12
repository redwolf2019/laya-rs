# v0.1.4 发布验收

日期：2026-10-12。程序 tag 为 `f8a081be15d195f741fcc06859c35e555fa6f3b9`，
固定模型为 `3bff67d421ae27d4`。本次由用户明确要求推送、关闭 #22 并发布各平台 Release；
四个平台共用 [v0.1.4](https://github.com/redwolf2019/laya-rs/releases/tag/v0.1.4)，模型继续独立发布。
已执行的文件校验和主机检查置信度 HIGH；未定位的 CI 原因保持未知，不归因为已证实的浮点或调度问题。

## 产物来源

| 产物 | 来源与边界 |
| --- | --- |
| Linux x86_64 / ARM64 | [release run 38235221676](https://github.com/redwolf2019/laya-rs/actions/runs/38235221676) 的两个成功 job；保留原始程序包及对应运行库源码包 |
| Windows x64 | Windows 11 专业工作站版 build 26300 / i5-13600KF；Rust 1.98.1、MSVC 14.44.35207、SDK 10.0.26100.0、固定官方 CPU ORT 1.28.0。VC 最低版本 14.44.35211.0，本机已满足，未执行运行库升级或重启 |
| macOS ARM64 | 本机 macOS 27.0.1 / M2 Max；tag 的独立干净工作树，包内 `build.json` 的 Source 为上述提交、Dirty=false；Rust 1.98.1、Xcode/SDK 27.0、最低目标 14.0；固定 ORT 1.28.0 CPU 源码构建 |
| 管理脚本与元数据 | 从 tag 工作树执行 `tools/release/package.py release --version v0.1.4`，保留 Linux 元数据并加入 Windows/macOS 字段 |

Windows 主机没有 Git，构建记录如实保留 `Source=uncommitted-source-snapshot`。
其 `source-files.json` 的 24 个 Rust、Cargo 和 toolchain 文件逐个与 tag 的 Git 内容校验一致。
Windows 原始 ZIP 的 SHA-256 为 `f99b251c3590194fbf23b3a8ccffee257298684857baf2e8b3f718fe2e1537d7`。
发布前发现 ORT 主许可证曾被同名项目 LICENSE 覆盖，因此补入 `licenses/ORT-LICENSE` 并重建
`program-files.json`；与原 ZIP 逐文件比较，除了该清单之外的既有文件字节均相同，exe/DLL 未改变。
ORT 许可证来自固定官方归档，SHA-256 为 `c250d6278f0b47a6439fb7592b08b58a55eb9f535aa49a1db63211c3f982b674`。
构建器修复提交为 `4fca627`，在 tag 之后；没有移动 tag，也没有将手动修复包装描述为原始 CI ZIP。

macOS 的 24 个源文件哈希同样与 tag 一致；打包器检查原生 ARM64、最低 OS、动态依赖和 ad-hoc 签名。
该包未使用 Developer ID，也未公证。CPU ORT 来源仍为固定提交
`da9b5e364c465de65c49d91e696cd6485270757f`，禁用 WebGPU/CoreML/XNNPACK。

## 最终附件 SHA-256

以下为发布前计算的最终清单；公开下载复核结果见下节。模型不重复附在程序 Release，
模型归档为 752018537 bytes，SHA-256 为
`699aae05cab2f36792859f59c533280daf8bcb96c57fae21d30850fc7ad8dfe5`。

```text
8e8f448e6654872123500dacf708837c4fa7ca98a3ce688e07143e8670e61a82  release.env
8f980b000eac3eec1e525eac3dbf5605c985939eb896e4031a6b182cd4eb1ee3  install.sh
d380376bf5c15d3d3c241f46f44bf74384e0c7fe06e0b3e842adeb2f0a002cd8  laya-server-windows-x86_64.zip
5d505c5f07b4b988dec409845e86d8043ed69291e6e6df448eef4b425a302c33  laya.ps1
47b5ae6522a6508c88a2630f839b9d2f5dadb48aaa007c1aca3dde9b9c94d4ec  laya-server-macos-aarch64.tar.gz
c12791e7558e28877b03af31ff0145eca0d44ddc4bbf52bfb49609223c6dc18a  laya-macos.sh
b442a1931cfda907e33aa6c3e66dc0cd80868454b60c912cb7d0b36fcd122d93  laya-server-linux-aarch64.tar.gz
1cdc3de282eafba5b2d00e4cffbda8a1cab8b1ee17af10c6a2a99c62150227d8  laya-server-linux-x86_64.tar.gz
8d1284e8370e8fa412bf7d24b9fb8427af86b904245c53e3edbbec7509dcaa78  runtime-sources-aarch64.tar.gz
b91423c1eb1408c205e1a27c4bc0610946ef9a551de8402f9c57c5d301f1dd37  runtime-sources-x86_64.tar.gz
```

## 已执行检查

- tag 对应的[普通 CI 38234498749](https://github.com/redwolf2019/laya-rs/actions/runs/38234498749)：Linux、Windows、macOS 全部通过格式、静态检查、单元与脚本边界检查。许可证修复提交 `4fca627` 的[普通 CI 38236210264](https://github.com/redwolf2019/laya-rs/actions/runs/38236210264) 也通过。
- Linux 最终两份程序包在各自原生 runner 使用固定模型通过中文 Choice/Score/Noul 真实 HTTP 检查；本次未重跑 systemd、OpenRC、runit、dinit 完整安装矩阵，历史范围见[安装验收](installer.md)。
- Windows 11：应用完整 21 请求模型对照及错误后恢复通过；原始包与补证后包的应用和 DLL 字节相同。最终 ZIP 在实际 8.3 TEMP 别名下通过安装/启停、更新、启动/API 故障回退、事务恢复、模型/密钥复用。21 请求 HTTP 对照及并发重复、断连恢复通过；原生包的正常 drain 与文件/Ctrl+C/Ctrl+Break grace 到期检查通过，端口可重新绑定。
- macOS 最终包：21 请求模型对照、21 请求 HTTP/engine/reference 对照、并发重复、chunked 413、断连及恢复、N=1/2 调度均通过。原生包的 SIGINT/SIGTERM 各 idle/drain/grace-expired 六项检查通过，退出码及端口回收符合契约。最终包完整部署检查通过：中文/空格/单引号路径、端口/PID 归属保护、A-to-B 更新、启动/API/架构故障回退、并发操作拒绝、中断恢复、停止状态和模型/密钥复用。
- 组装脚本的 Windows/macOS 元数据兼容性检查与 cargo metadata stderr 回归通过；全部 10 个被列入 `SHA256SUMS` 的附件哈希核对一致。
- 文档复核：147 个本地链接/锚点和 README Shell 语法通过，`git diff --check` 通过。Standards / Spec 两轴评审各指出同一处 dinit 名称笔误，已修正，无其他遗留发现。
- Release 已于 2026-10-12 10:58:21 UTC 公开，非 draft、非 prerelease，latest 指向 v0.1.4。不带 GitHub 凭证完整下载全部 11 个附件，逐一核对本地 SHA-256、大小与 GitHub asset digest，全部一致。
- macOS 已通过 GitHub latest 查询、脚本哈希、程序/元数据在线下载、安装、三类型 API、readiness 和停止；复用现有模型，未复测浏览器 quarantine。
- Windows 在线入口首轮在教程的脚本校验处失败：PowerShell 5.1 对 `application/octet-stream` 的 `Invoke-WebRequest.Content` 返回 `System.Byte[]`，不能直接按行匹配 SHA256SUMS。下载的脚本哈希正确，未运行错误脚本；教程已改为 `-OutFile` 下载后由 `Get-Content -Raw` 读取，修正后在 Windows 11 复跑，latest 查询、脚本哈希、程序/元数据在线下载、安装、三类型 API、readiness 和停止全部通过，复用原模型。

本轮实跑的结论行摘录见[检查输出](release-v0.1.4/checks.log)。完整测试入口为 tag 中的
`tools/release/smoke-{windows.ps1,macos.py}`、真实模型 ignored tests 和
`docs/validation/{windows/lifecycle.ps1,macos/lifecycle.py}`。

## 未通过的 release CI

[release run 38235221676](https://github.com/redwolf2019/laya-rs/actions/runs/38235221676)
整体结论为 **failure**，自动创建 draft 的步骤为 skipped，本次手动组装并发布。
普通 CI 通过不能覆盖以下失败，也不代表全部发布门禁通过。

1. Windows Server 2025：真实安装、更新与故障回退通过，但完整 21 请求对照在
   `long-padding.json` 的 Score 上失败，参考 `4.5307`、实际 `4.5306`。
   相同应用源码在原 Windows 11 主机通过全部 21 请求。原因未定位，未调整样例或数值阈值。
   用户明确选择“仅按已验证 Windows 11 范围发布，公开记录 Server CI 失败”；不支持 Windows Server。
2. macOS 15 runner：构建及打包通过，安装、启动、版本切换、启动故障回退通过；
   模拟 API 故障后的回退检查提前退出，`Command exited early: 1`。
   作业没有上传该命令的详细日志，不能据此确定根因。其后完整数值对照、HTTP、并发及生命周期步骤未运行。
   按 #21 已确认的本机 macOS 27.0.1 验收范围发布本机构建，不将 runner 失败改记为通过。

此前 v0.1.2 / v0.1.3 也是未发布的 tag，保留不移动。发布准备中修复了 Windows 短路径比较、
fixture 换行、macOS cargo metadata 的 stderr 混入 JSON、验收脚本的 bytecode 副作用与 ORT 许可证覆盖；
没有改模型、推理契约或误差阈值。README 的 #22 已由提交 `30ac478` 关闭。

## 仍未验收

Windows 普通用户与跨用户 ACL、干净系统/VC-UAC 完整矩阵；macOS 14、无开发工具系统、
浏览器下载后 quarantine/Gatekeeper 手动许可；Windows Server 与 macOS 15 CI 失败原因。
本次使用开发主机和现有模型缓存，不能替代上述环境验收。签名、同源哈希清单不构成独立的发布者认证。

历史范围见 [Windows 验收](windows.md)、[macOS 验收](macos.md) 和 [README 命令验收](readme.md)。
