Linux x86_64 / ARM64 原生安装包，包含服务程序、ONNX Runtime CPU 和私有动态运行库。
安装器提供交互配置、服务托管、开机自启、真实推理检查、升级回退和卸载。
默认监听所有网卡；API 使用密钥；安装器不修改防火墙。

Windows 11 x64 便携候选包含 exe、官方 CPU ORT DLL、许可证和文件校验清单；
使用 PowerShell 5.1 的 laya.ps1 Install/Start/Stop/Update，不注册系统服务。
首次生成私有密钥，升级失败恢复旧版并重新进行中文三类型真实推理。
VC++ 运行库缺失时仅在用户同意后安装。具体操作与限制见 docs/windows.md。

macOS Apple Silicon ARM64 候选使用 Shell 的 install/start/stop/update，运行在用户目录，
包含独立构建的 ORT 1.28.0 CPU dylib。应用与 dylib 使用 ad-hoc 签名，**没有 Developer ID
身份认证，也未经过 Apple 公证**；不关闭 Gatekeeper 或自动移除 quarantine。
手动允许运行、前台终端生命周期及恢复步骤见 docs/macos.md。
最低构建目标 14.0；按本次用户要求只在本机 macOS 27.0.1 验收，不代表 14 已通过。
发布前核对 docs/validation/macos.md 中同一包哈希、浏览器下载和未执行项。

模型独立发布，版本与校验值见 release.env。运行库对应源码见 runtime-sources 附件；
许可证与归属包含在程序及模型包内。SHA256SUMS 校验同次发布的附件。

发布前须更新 docs/validation/installer.md，记录实际运行的宿主机/虚拟机安装、
重启自启、升级回退、卸载验收。构建工作流通过只证明双架构程序真实推理及静态检查，
不能代替服务管理器和所有 Linux 发行版的部署验收。
Windows 发布仍为 draft；转为正式发布前必须在 Windows 11 x64 上复验同一 ZIP 哈希，
更新 docs/validation/windows.md。Windows Server CI 的模型门禁不替代该验收。
