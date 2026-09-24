# 原生安装验收

日期：2026-09-24。原生 GitHub runner 门禁和公开安装复验已通过。
以下为实际执行范围，置信度 HIGH；不推断所有发行版、内核或 CPU 均已通过。

已执行：

- ShellCheck：安装器与发布脚本通过。
- actionlint：两个 GitHub Actions 工作流通过。
- ShellCheck 0.9.0 和本机版本均通过；初次 CI 的复合条件提示已改为明确分支。
- 发布源码 `v0.1.1` 的 [Rust/脚本 CI](https://github.com/redwolf2019/laya-rs/actions/runs/35963650724)
  通过，含 fmt、clippy、普通测试及脚本边界检查。普通测试中依赖模型的 ignored 项未运行。
- `python3 scripts/test-install.py`：参数、无终端拒绝、压缩包路径/链接拒绝、清单重复字段、
  服务管理命令失败传播通过。仅隔离逻辑检查，不证明实际安装可用。
  另检查了恢复失败时保留配置备份，以及失败解包目录的清理。
- Linux x86_64 Debian 12 容器：预编译运行包真实中文 Choice/Score/Noul API 检查通过；
  宿主为 Apple Silicon，属于模拟架构执行，不作为原生 x86_64 硬件/性能验收。
- Linux ARM64 Alpine/musl 容器：同一 ARM64 运行包真实中文三类型 API 检查通过，
  使用私有 glibc/动态加载器，没有替换 Alpine 系统库；不证明 OpenRC 开机自启。
- Linux x86_64 Alpine/musl 容器：真实中文三类型 API 检查通过；模拟架构执行。

## 虚拟机生命周期

宿主 Apple Silicon/macOS；QEMU 11.1.1；两台 Debian 12 genericcloud 虚拟机均为
4 vCPU、6 GiB RAM、16 GiB 虚拟磁盘。ARM64 使用 HVF，x86_64 使用 TCG 模拟。
镜像在启动前核对 Debian 官方 SHA512SUMS；两种架构分别安装对应运行包。
本轮生命周期候选为 `v0.1.0`，环境与包校验值见 [environment.json](installer/environment.json)。
后续 `v0.1.1` 只调整安装器条件表达式并更新版本；最终发布包的原生 CI 和公开下载另行记录。

| 检查 | ARM64 | x86_64 |
| --- | --- | --- |
| 原样安装脚本、交互确认、下载/校验/解包 | 通过 | 通过 |
| systemd 后台服务、非 root 用户、鉴权三类型推理 | 通过 | 通过 |
| 重启后自启并恢复 ready | 通过 | 通过 |
| 新版故意令 API 验收失败，旧版恢复并再次推理 | 通过 | 通过 |
| 回退后旧程序链接和密钥哈希不变 | 通过 | 通过 |
| 默认卸载保留数据，重装复用模型与密钥 | 通过 | 通过 |
| 彻底卸载移除目录、服务及专用用户 | 通过 | 通过 |

测试下载经过隔离的本地 HTTPS 发布代理，代码见 `tools/release/test-server.py`。
仅测试 VM 信任临时证书；宿主没有修改证书或 DNS。生产安装器没有关闭 TLS 或校验，
失败候选版本只在临时目录中把 `response.jq` 改为 `false`，重算压缩包哈希并发布给测试代理。
因此回退是实际停止、切换、启动旧服务及真实推理，不是 mock；该步骤不证明公开 GitHub 可达。

固定模型 id 为 `3bff67d421ae27d4`，压缩包 752018537 bytes，SHA-256：
`699aae05cab2f36792859f59c533280daf8bcb96c57fae21d30850fc7ad8dfe5`。

## 非 systemd 服务适配

在独立 ARM64 Alpine 容器中运行真实 OpenRC、runit、dinit supervisor，分别验证启动、
鉴权三类型推理、停止、更换密钥后再次启动/推理、移除服务。全部通过。
OpenRC/runit 包含合法密钥 `~` 的回归，防止 shell 把密钥误作 HOME 展开。
OpenRC 输出容器 cgroup 只读警告；此处不把容器当宿主安装，不证明重启自启或完整 cgroup 功能。

复现适配器检查（仅允许在可丢弃 Docker 容器执行）：

```sh
docker run --rm --platform linux/arm64 -v "$PWD:/work:ro" -v "$PWD/dist:/release:ro" -w /work \
  alpine:latest sh tools/release/test-adapter.sh openrc /release/laya-server-linux-aarch64.tar.gz /work/models/multilingual
```

将 `openrc` 替换为 `runit`、`dinit` 可检查另两个适配器。发布包推理复现入口为
`sh tools/release/smoke.sh <解包目录> <固定模型目录>`。
其他发行版及非 systemd 管理器的宿主开机自启仍需补充实机/虚拟机验收。

## 正式发布包

`v0.1.1` 的 [发布工作流](https://github.com/redwolf2019/laya-rs/actions/runs/35965526227)
在原生 `ubuntu-24.04` x86_64 和 `ubuntu-24.04-arm` runner 构建、下载正式模型、核对
模型逐文件 SHA-256，并通过带鉴权的中文 Choice/Score/Noul API 推理。两个架构和草稿发布
job 均为 success。构建使用固定 Rust/Debian 镜像，运行包源码对应 tag `v0.1.1`。

模型 Release 已公开，GitHub asset digest 与本地打包值一致，匿名下载返回 HTTP 200。
程序 Release `v0.1.1` 已设为 Latest；下载的 `install.sh` 与该 tag 的脚本字节一致，
`release.env` 和脚本的 SHA-256 与附件 `SHA256SUMS` 一致。

正式程序包 SHA-256：

- x86_64：`816170026b94a4115ecc3380a87b04f0f869dee9f004190fa64dfe832542a4f6`
- ARM64：`01b0bd1ab42341fe8262af3a8fe933e776439336fc11bb4cae7d95516d6247ba`

在上述 ARM64 Debian VM 中，按 README 原样执行公开安装命令，匿名取得脚本、元数据和
正式 ARM64 程序包，安装、systemd 自启注册、非 root 运行、密钥文件 `0600` 和真实三类型
推理均通过。该次命令使用预先放入、与发布资产一致的模型缓存，安装器重新校验了全部模型文件。
本机从公开站点重复下载完整模型因网络较慢而主动中止，未把这次中止记为通过；首次下载流程
已由前述候选 VM 覆盖，正式模型下载和真实推理由发布 CI 覆盖。
随后按 README 的公开卸载命令选择彻底删除，服务、程序、模型、配置和专用用户均已移除。
两台临时验收虚拟机及本地 HTTPS 测试代理已关闭。
