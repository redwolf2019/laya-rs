# 原生安装、升级与卸载

本教程面向正常启动、系统目录可写的 Linux 宿主机或虚拟机，架构为 x86_64 / ARM64。
需要 root 或 sudo，以及用于下载脚本的 curl。运行服务不需要 Docker、Python、Node.js、
PyTorch、Rust 工具链或 GPU。脚本只支持交互运行，所有输入从 `/dev/tty` 读取。

## 安装和升级

完整下载脚本后执行；下载失败不会执行部分脚本，执行结束删除临时脚本：

```sh
( script=$(mktemp) || exit; trap 'rm -f "$script"' EXIT; curl -fsSL --proto '=https' --tlsv1.2 https://github.com/redwolf2019/laya-rs/releases/latest/download/install.sh -o "$script" && sh "$script" install )
```

脚本按需通过 sudo 提权，识别架构和运行中的服务管理器，展示版本、下载量、目录、端口、
线程与并发数。回车使用默认值；输入 `e` 可修改端口、线程、并发和密钥，其他输入取消。
默认 `0.0.0.0:8080`、推理线程 2、并发槽 1、inter-op 线程 1、退出 grace 120 秒。
这是安装配置，不修改 CLI 自身默认值，也不是对所有硬件的性能承诺。

密钥默认使用系统随机源生成，也可隐藏输入已有 Bearer 密钥。脚本不回显密钥，保存到
root 所有、权限 `0600` 的 `/etc/laya-server/token.env`，其父目录权限为 `0700`。
服务以专用 `laya-server` 用户运行。读取密钥：

```sh
sudo cat /etc/laya-server/token.env
```

程序包内包含私有动态加载器、glibc、C/C++ 依赖、ONNX Runtime 和用于检查 API 的 jq。
这些库仅供本程序使用，不替换系统库；支持 glibc/musl 主机的能力须以实际验证矩阵为准。
基础下载/解包工具缺失时，先征求同意再调用系统包管理器安装。发行版自己的生命周期、
内核能力及本机安全策略仍须满足部署要求；不将“发行版不限”表述为任意系统都已验证。

程序位于 `/opt/laya-server`，模型位于 `/var/lib/laya-server/models/<模型版本>`，
配置位于 `/etc/laya-server`。首次安装需为模型压缩包、解压目录与程序预留空间；脚本检查
`/var/lib` 至少有 4 GiB 空闲，分区独立时还需保证 `/var/tmp` 和 `/opt` 有下载/解包空间。
模型在开始监听前校验 SHA-256 并预热；随后检查开机自启、就绪和一次带鉴权的中文
Choice/Score/Noul 真实 API 调用，全部通过才显示成功。

重复执行同一命令可升级或修复。默认选择最新稳定 Release，解析后固定本次版本；也可将
命令末尾的 `install` 改成 `install v0.1.1` 指定版本。模型独立固定版本，校验通过后复用。
旧程序在下载和校验期间继续服务，切换时有短暂停机。失败则恢复旧链接、配置和服务定义，
再次进行真实推理检查；恢复失败会单独报错，不宣称回退成功。旧版本目录保留供诊断，
完整卸载时一并清理。进程被 SIGKILL 或断电时无法执行 shell 退出处理，须检查服务和锁后恢复。

服务默认允许其他机器连接，但安装器不修改防火墙。按部署环境放行所选 TCP 端口，
生产 HTTPS 由网关提供，不把共享密钥放入前端或源码。当前支持识别并配置
systemd、OpenRC（supervise-daemon）、runit（`/var/service`）和 dinit（`/etc/dinit.d/boot.d`）；未知管理器、
容器、只读系统及非本安装器创建的同名目录/服务会明确拒绝，不能降级为手动启动。
实际验证记录见 [安装验收](validation/installer.md)。

## 检查与调用

```sh
curl -fsS http://127.0.0.1:8080/readyz
sudo sh -c 'token=$(sed -n "s/^LAYA_API_TOKEN=//p" /etc/laya-server/token.env); printf "Authorization: Bearer %s\n" "$token" | curl -fsS --header @- -H "Content-Type: application/json" --data-binary @/opt/laya-server/current/smoke.json http://127.0.0.1:8080/v1/system-one'
```

若修改了端口，上述地址相应调整。最后一条命令通过标准输入传递鉴权头，密钥不出现在
curl 参数列表。三种问题的业务示例和完整 API 说明见 [README](../README.md#调用-api)。

systemd 查看状态和日志：`sudo systemctl status laya-server`、
`sudo journalctl -u laya-server`；OpenRC 使用 `sudo rc-service laya-server status`；
runit 使用 `sudo sv status /var/service/laya-server`；dinit 使用 `sudo dinitctl status laya-server`。
安装器只检查本地 API 可用，不证明外部网络、防火墙或网关已经配置正确。

## 卸载

```sh
( script=$(mktemp) || exit; trap 'rm -f "$script"' EXIT; curl -fsSL --proto '=https' --tlsv1.2 https://github.com/redwolf2019/laya-rs/releases/latest/download/install.sh -o "$script" && sh "$script" uninstall )
```

默认删除服务注册和程序，保留模型、配置及专用用户，方便重新安装。若选择彻底删除，
还需输入 `DELETE`，才会删除本安装器的模型、配置（包括密钥）和专用用户。
系统共享依赖不删除，不修改其他服务或防火墙规则。

## 维护者发布步骤

模型由维护者离线导出；终端用户不会安装 Python 或重新导出。发布前保留上游模型卡、
许可与归属，以及独立导出、LayerNorm 改动说明；清单逐文件核验，权重不进入 Git。

```sh
python3 tools/release/package.py model --out dist
python3 tools/release/package.py id
```

将生成的 `model-<id>.tar.gz` 发布到 `model-<id>` Release，设置为非 Latest。
程序版本使用源代码 tag，运行 `Build and validate native release (draft)` 工作流。
它在原生 x86_64、ARM64 runner 构建，在两边使用固定模型执行真实推理；成功后创建草稿。
更新并检查安装验收记录后再发布草稿为 Latest，避免让安装入口指向未验收的产物。

本地构建使用固定 Debian/Rust 镜像，两种架构分别执行（arm64 的 `--platform` 改成对应架构）：

```sh
mkdir -p dist
docker run --rm --platform linux/amd64 -v "$PWD:/work:ro" -v "$PWD/dist:/out" -w /work \
  rust:1.98.1-slim-bookworm@sha256:ff521445a372125ed4f76e1453a1f8098f2d05332d1601d30db1c1f62757e730 \
  sh tools/release/build.sh v0.1.1 /out
python3 tools/release/package.py release --version v0.1.1 --out dist
```

`runtime-sources-<arch>.tar.gz` 是实际打包 Debian 库的对应源码（原始包、补丁、版本与构建入口），
须和运行包一起分发。相关依据：[LGPL 2.1](https://www.gnu.org/licenses/old-licenses/lgpl-2.1.en.html)、
[GNU 对动态库分发的说明](https://www.gnu.org/licenses/gpl-faq.en.html#LGPLStaticVsDynamic)。
应用与 Rust crate、ONNX Runtime 的许可和第三方通知保存在运行包 `licenses/` 中。
`SHA256SUMS` 验证传输完整性，信任来源仍是仓库及 HTTPS 发布渠道，不把同源哈希称为独立签名。
