# 原生安装验收

日期：2026-09-24。发布验证进行中，原生 GitHub runner 与公开下载仍待执行。
以下为实际执行范围，置信度 HIGH；不推断所有发行版、内核或 CPU 均已通过。

已执行：

- ShellCheck：安装器与发布脚本通过。
- actionlint：两个 GitHub Actions 工作流通过。
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

| 检查 | ARM64 | x86_64 |
| --- | --- | --- |
| 原样安装脚本、交互确认、下载/校验/解包 | 通过 | 通过 |
| systemd 后台服务、非 root 用户、鉴权三类型推理 | 通过 | 通过 |
| 重启后自启并恢复 ready | 通过 | 通过 |
| 新版故意令 API 验收失败，旧版恢复并再次推理 | 通过 | 通过 |
| 回退后旧程序链接和密钥哈希不变 | 通过 | 通过 |
| 默认卸载保留数据，重装复用模型与密钥 | 通过 | 待执行 |
| 彻底卸载移除目录、服务及专用用户 | 通过 | 待执行 |

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
