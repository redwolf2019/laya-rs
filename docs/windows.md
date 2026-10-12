# Windows 11 x64 便携部署

#20 的原始 Windows 10 22H2 范围已按用户本次指令改为仅 Windows 11 x64。
应用使用 CPU ONNX Runtime，不使用显卡。实现与实测范围见
[Windows 历史验收记录](validation/windows.md)。v0.1.4 已公开发布，最终 ZIP、哈希与本机复验见
[发布验收](validation/release-v0.1.4.md)。Windows Server CI 的数值对照未通过，本次仅按已验 Windows 11 范围发布。
原 #20 按已完成实现和现有主机实测收口；普通用户、干净环境/VC-UAC 验收
按用户要求暂缓，未执行状态保留在验收记录中。

## 部署契约

`scripts/laya.ps1` 兼容系统 Windows PowerShell 5.1，提供 `Install`、`Start`、
`Stop`、`Update`。默认 `%LOCALAPPDATA%\laya-server`，程序按版本独立存放，
模型按固定 manifest 标识存放，密钥和配置独立于版本。普通用户运行，无系统服务、
开机任务或防火墙修改。默认监听 `0.0.0.0:8080`，局域网访问须由用户配置防火墙；
验收通过 loopback，IPv6 监听也使用 loopback。

首次安装生成 32 字节密码学随机密钥，目录与文件仅当前用户及 SYSTEM 可访问。
密钥经子进程环境注入，不写命令行；升级和回退不改变密钥。模型始终复用固定
manifest，不重新导出。下载只接受 HTTPS，固定本次 release、大小及 SHA-256；
同源清单校验不是独立数字签名。ZIP/tar.gz 解包只接受目标目录内的普通文件和目录。

升级在旧进程继续运行时完成下载和预检查，使用排他文件锁拒绝并行管理操作。
停止前核对 PID、创建时间、exe 路径、当前用户及部署实例。端口归属须匹配本次进程，
然后检查 readiness、鉴权和中文 Choice/Score/Noul 真实响应。失败候选先停止，
再恢复旧配置并重新执行真实推理；回退失败单独报错。原来停止的部署验证后仍停止。
事务记录先于切换写入，异常中断保留旧配置及候选信息，下一次管理操作明确要求
执行恢复，不猜测进程归属或自动删除备份。

VC++ x64 运行库最低版本随构建产物记录；缺失或过旧时须先征得用户同意，
验证微软安装程序签名后仅该步骤请求 UAC。拒绝、取消、安装失败或重启要求均非成功。
不改机器级执行策略，不绕过组织策略。脚本使用 ASCII 源码，HTTP JSON 显式 UTF-8。

## 操作与恢复

[v0.1.4 Release](https://github.com/redwolf2019/laya-rs/releases/tag/v0.1.4) 提供 Windows ZIP 和管理脚本；
普通运行可直接使用下方下载入口。需要自行构建时，在 MSVC x64 Developer PowerShell 中生成候选：

```powershell
rustup show active-toolchain
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\tools\release\build-windows.ps1 -Version v0.1.4-candidate
```

程序 ZIP、固定模型 `model-3bff67d421ae27d4.tar.gz` 与 `release.env` 放在同一候选目录。
完整组装由 `tools/release/package.py release` 完成，保留 Linux 字段。
单平台开发验收按 [release workflow](../.github/workflows/release.yml) 的
`Fixed model and real install/update/rollback gate` 步骤，用 `Get-FileHash` 和文件长度
生成同样字段。ZIP 包含 `build.json`、`pe-dependencies.txt`、`VC_MIN`、
`program-files.json`、`source-files.json` 和许可证，不复制或替换系统 DLL。

下载完整脚本、核对同次发布清单，再解除下载标记：

```powershell
$release = Invoke-RestMethod 'https://api.github.com/repos/redwolf2019/laya-rs/releases/latest'
if ($release.assets.name -notcontains 'laya-server-windows-x86_64.zip') { throw 'This release has no Windows asset.' }
$base = "https://github.com/redwolf2019/laya-rs/releases/download/$($release.tag_name)"
Invoke-WebRequest -UseBasicParsing -TimeoutSec 600 "$base/laya.ps1" -OutFile .\laya.ps1
Invoke-WebRequest -UseBasicParsing -TimeoutSec 60 "$base/SHA256SUMS" -OutFile .\SHA256SUMS
$sums = Get-Content -LiteralPath .\SHA256SUMS -Raw
$expected = @($sums -split "`n" | Where-Object { $_ -match '^([0-9a-f]{64})  laya\.ps1\r?$' })
if ($expected.Count -ne 1 -or (Get-FileHash .\laya.ps1).Hash.ToLowerInvariant() -ne $expected[0].Substring(0,64)) { throw 'Script hash mismatch.' }
Unblock-File -LiteralPath .\laya.ps1
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\laya.ps1 Install -Version $release.tag_name
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\laya.ps1 Start
```

`RemoteSigned` 仅作用于该子进程，组织 Group Policy 仍优先。组织禁止脚本时，
请使用组织管理员允许的部署流程，不使用 `Bypass` 或修改机器级执行策略。
Install 验证后保持首次部署停止；Start 创建受控控制台进程，日常启动不打印密钥。
Stop 可从另一个 PowerShell 窗口运行：

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\laya.ps1 Stop
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\laya.ps1 Update
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\laya.ps1 Update -Version v0.1.4
```

程序位置用 `-Root 'D:\Laya 程序'`，外部固定模型用 Install 的
`-ModelDirectory 'D:\Laya 模型'`，只读取和校验外部模型。
Install 支持 `-Listen`、`-Threads`、`-InterOpThreads`、`-MaxConcurrency`、
`-ShutdownGrace`；重复 Install 保留未显式覆盖的参数，Update 保留原配置。
本地候选使用 `-ReleaseDirectory 'D:\candidate'`，仍执行全部大小、hash、归属和推理校验。
`-ReleaseBase` 支持 HTTPS 候选入口；自定义入口应显式指定 `-Version`，
`latest` 通过本项目 GitHub 稳定 release API 解析。

当前用户可在本地读取密钥并调用 API（正文显式 UTF-8）：

```powershell
$root = Join-Path $env:LOCALAPPDATA 'laya-server'
$headers = @{ Authorization = 'Bearer '+[IO.File]::ReadAllText((Join-Path $root 'token')) }
$json = '{"state":"urgent refund request","questions":{"q":{"type":"noul","instructions":"Is this urgent?"}}}'
Invoke-RestMethod 'http://127.0.0.1:8080/v1/system-one' -Method Post -Headers $headers -ContentType 'application/json; charset=utf-8' -Body ([Text.Encoding]::UTF8.GetBytes($json))
Remove-Variable headers
```

未完成事务拒绝常规操作，保留 `transaction.json`、`current.json` 和旧版本目录。
先检查记录中的路径、PID/创建时间、控制台日志和文件占用，再显式恢复：

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\laya.ps1 Update -Recover
```

恢复会停止已确认归属的实例，加载旧配置并重新真实推理验证，恢复原来的运行状态。
归属不明或“Rollback failed”时，不按名称强杀、不删除旧版本或事务来掩盖失败。
由当前用户处理已退出或无法记录的实例后再次恢复。
Ctrl+C/Ctrl+Break 进入共同 drain；关闭窗口、注销、关机、任务管理器结束进程和外部
强杀可能中断推理或升级事务。程序包和模型的归属、校验及冷启动证据见验收记录。
