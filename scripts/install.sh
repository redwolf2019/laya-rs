#!/bin/sh
# Interactive native installer. All persistent paths belong exclusively to this installer.
set -eu
set +x
umask 077
PATH=/usr/sbin:/usr/bin:/sbin:/bin:${PATH:-}
export PATH
REPO=redwolf2019/laya-rs
ROOT=/opt/laya-server
DATA=/var/lib/laya-server
CONF=/etc/laya-server
LOCK=/run/laya-installer.lock
work='' locked='' switched='' old='' manager='' restore_echo='' model_stage=''

die() { printf '错误：%s\n' "$*" >&2; exit 1; }
say() { printf '%s\n' "$*"; }
ask() { printf '%s ' "$1" >&3; IFS= read -r answer <&3 || die '终端输入已关闭'; }
confirm() { ask "$1 [y/N]"; case "$answer" in y|Y|yes) return 0;; *) return 1;; esac; }
field() { awk -F= -v key="$2" '$1 == key {sub(/^[^=]*=/, ""); print; n++} END {if(n != 1) exit 1}' "$1"; }
number() { case "$1" in ''|*[!0-9]*|0*) return 1;; esac; [ "${#1}" -le 10 ] && [ "$1" -ge 1 ] && [ "$1" -le "$2" ]; }
version_ok() { case "$1" in v[0-9]* ) ;; *) return 1;; esac; case "$1" in *[!a-zA-Z0-9.+-]*) return 1;; esac; }
token_ok() { printf '%s\n' "$1" | LC_ALL=C grep -Eq '^[A-Za-z0-9._~+/-]+=*$'; }
hash_ok() { [ "${#1}" -eq 64 ] && case "$1" in *[!0-9a-f]*) return 1;; esac; }
download() {
    curl -q -fL --proto '=https' --proto-redir '=https' --tlsv1.2 \
        --connect-timeout 20 --max-time 3600 --retry 3 --output "$2.part" "$1"
    mv -f "$2.part" "$2"
}
checked_download() {
    hash_ok "$3" || die '发布清单中的 SHA-256 非法'
    download "$1" "$2"
    printf '%s  %s\n' "$3" "$2" | sha256sum -c - || die '下载文件校验失败'
}
unpack() {
    # Reject links, devices, absolute paths and parent traversal before extracting as root.
    tar -tzf "$1" > "$work/members"
    awk '/^\// || /(^|\/)\.\.(\/|$)/ {bad=1} END {exit bad}' "$work/members" || die '压缩包路径非法'
    tar -tvzf "$1" > "$work/types"
    awk 'substr($0,1,1) != "-" && substr($0,1,1) != "d" {bad=1} END {exit bad}' "$work/types" || die '压缩包包含非普通文件'
    mkdir -p "$2"
    tar -xzf "$1" -C "$2"
    chmod -R go-w "$2"
}
detect_manager() {
    if [ -d /run/systemd/system ] && command -v systemctl >/dev/null 2>&1; then manager=systemd
    elif [ -d /run/openrc ] && command -v rc-service >/dev/null 2>&1 && command -v supervise-daemon >/dev/null 2>&1; then manager=openrc
    elif [ -d /var/service ] && command -v sv >/dev/null 2>&1 && command -v chpst >/dev/null 2>&1; then manager=runit
    elif [ -d /etc/dinit.d/boot.d ] && command -v dinitctl >/dev/null 2>&1 && dinitctl status boot >/dev/null 2>&1; then manager=dinit
    else die '尚未适配或未运行的服务管理器；需要 systemd、OpenRC、runit 或 dinit。未安装任何服务。'; fi
}
owned_directory() {
    [ ! -L "$1" ] || die "目录不能是符号链接：$1"
    if [ -e "$1" ]; then
        [ -d "$1" ] || die "路径不是目录：$1"
        [ "$(stat -c %u "$1")" = 0 ] || die "目录必须由 root 所有：$1"
        mode=$(stat -c %a "$1")
        [ "$((0$mode & 0022))" = 0 ] || die "目录不能允许其他用户写入：$1"
        if [ ! -f "$1/.laya-installer" ] || [ "$(cat "$1/.laya-installer")" != 1 ]; then die "拒绝接管非本安装器目录：$1"; fi
    fi
}
preflight() {
    [ "$(uname -s)" = Linux ] || die '仅支持 Linux 宿主机或虚拟机'
    [ "$(id -u)" = 0 ] || die '请使用 sudo sh 执行，或以 root 运行'
    case "$(uname -m)" in x86_64) arch=x86_64;; aarch64|arm64) arch=aarch64;; *) die '仅支持 x86_64 / ARM64';; esac
    if [ -e /.dockerenv ] || [ -e /run/.containerenv ]; then die '不支持容器内安装'; fi
    if command -v systemd-detect-virt >/dev/null 2>&1 && systemd-detect-virt --container --quiet; then die '不支持容器内安装'; fi
    detect_manager
    for dir in "$ROOT" "$DATA" "$CONF"; do owned_directory "$dir"; done
    for dir in /opt /var/lib /etc /run; do
        if [ ! -d "$dir" ] || [ ! -w "$dir" ]; then die "系统目录不可写：$dir"; fi
    done
    mkdir "$LOCK" 2>/dev/null || die "另一个安装器正在执行，或存在未清理的锁：${LOCK}（确认无安装进程后再移除）"
    locked=1
    printf '%s\n' "$$" > "$LOCK/pid"
    work=$(mktemp -d /var/tmp/laya-installer.XXXXXX)
}
ensure_tools() {
    missing=
    for cmd in curl tar gzip sha256sum od awk grep sed; do command -v "$cmd" >/dev/null 2>&1 || missing="$missing $cmd"; done
    [ -n "$missing" ] || return 0
    say "需要安装基础工具：${missing}；系统共享依赖不会在卸载时删除。"
    confirm '允许通过系统包管理器安装这些工具？' || die '未安装所需工具'
    if command -v apt-get >/dev/null 2>&1; then apt-get update; apt-get install -y curl ca-certificates tar gzip coreutils
    elif command -v apk >/dev/null 2>&1; then apk add curl ca-certificates tar gzip coreutils
    elif command -v dnf >/dev/null 2>&1; then dnf install -y curl ca-certificates tar gzip coreutils
    elif command -v yum >/dev/null 2>&1; then yum install -y curl ca-certificates tar gzip coreutils
    elif command -v zypper >/dev/null 2>&1; then zypper --non-interactive install curl ca-certificates tar gzip coreutils
    elif command -v pacman >/dev/null 2>&1; then pacman -S --needed --noconfirm curl ca-certificates tar gzip coreutils
    elif command -v xbps-install >/dev/null 2>&1; then xbps-install -Sy curl ca-certificates tar gzip coreutils
    elif command -v emerge >/dev/null 2>&1; then emerge --noreplace net-misc/curl app-arch/tar app-arch/gzip sys-apps/coreutils app-misc/ca-certificates
    else die '未识别包管理器，请先安装上面列出的基础工具'; fi
    for cmd in curl tar gzip sha256sum od awk grep sed; do command -v "$cmd" >/dev/null 2>&1 || die "缺少工具：$cmd"; done
}
make_directory() {
    if [ ! -d "$1" ]; then mkdir -p "$1"; printf '1\n' > "$1/.laya-installer"; fi
    chmod "$2" "$1"
}
create_user() {
    if id laya-server >/dev/null 2>&1; then
        [ -f "$CONF/user-owned" ] || die '已存在非本安装器创建的 laya-server 用户'
        [ "$(id -u laya-server)" != 0 ] || die '服务用户不能是 root'
        return
    fi
    if command -v useradd >/dev/null 2>&1; then useradd --system --user-group --no-create-home --home-dir "$DATA" --shell /bin/false laya-server
    elif command -v adduser >/dev/null 2>&1; then addgroup -S laya-server; adduser -S -D -H -h "$DATA" -s /bin/false -G laya-server laya-server
    else die '缺少系统用户管理工具 useradd/adduser'; fi
    touch "$CONF/user-owned"
}
load_config() {
    port=8080; threads=2; slots=1; token=
    if [ -f "$CONF/server.conf" ]; then
        port=$(field "$CONF/server.conf" PORT); threads=$(field "$CONF/server.conf" THREADS); slots=$(field "$CONF/server.conf" SLOTS)
        token=$(field "$CONF/token.env" LAYA_API_TOKEN)
    fi
    if ! number "$port" 65535 || ! number "$threads" 2147483647 || ! number "$slots" 1024; then die '现有配置数值非法'; fi
    [ -z "$token" ] || token_ok "$token" || die '现有密钥格式非法'
}
edit_config() {
    ask "监听端口 [$port]："; port=${answer:-$port}
    ask "CPU 推理线程数 [$threads]："; threads=${answer:-$threads}
    ask "推理并发槽数 [$slots]（每槽单独加载模型）："; slots=${answer:-$slots}
    if ! number "$port" 65535 || ! number "$threads" 2147483647 || ! number "$slots" 1024; then die '配置数值非法'; fi
    printf 'API 密钥（隐藏输入，回车保留现有密钥或自动生成）：' >&3
    restore_echo=$(stty -g <&3); stty -echo <&3
    IFS= read -r input_token <&3 || die '密钥输入中断'
    stty "$restore_echo" <&3; restore_echo=
    printf '\n' >&3
    if [ -n "$input_token" ]; then token_ok "$input_token" || die '密钥须符合 Bearer token 格式'; token=$input_token; fi
    unset input_token
}
service_stop() {
    case "$manager" in
        systemd) systemctl stop laya-server;;
        openrc) rc-service laya-server stop;;
        runit) sv -w 135 force-stop /var/service/laya-server;;
        dinit) dinitctl stop laya-server;;
    esac
}
service_file() {
    case "$manager" in
        systemd) printf '%s\n' /etc/systemd/system/laya-server.service;;
        openrc) printf '%s\n' /etc/init.d/laya-server;;
        runit) printf '%s\n' /etc/sv/laya-server/run;;
        dinit) printf '%s\n' /etc/dinit.d/laya-server;;
    esac
}
service_start() {
    case "$manager" in
        systemd) systemctl daemon-reload && systemctl enable --now laya-server && systemctl is-enabled --quiet laya-server && systemctl is-active --quiet laya-server;;
        openrc) rc-update add laya-server default && rc-service laya-server start && rc-service laya-server status;;
        runit) rm -f /etc/sv/laya-server/down || return 1
            if [ ! -L /var/service/laya-server ]; then ln -s /etc/sv/laya-server /var/service/laya-server || return 1; fi
            count=0
            while [ ! -p /etc/sv/laya-server/supervise/control ]; do
                count=$((count + 1)); [ "$count" -lt 30 ] || return 1; sleep 1
            done
            sv -w 30 up /var/service/laya-server;;
        dinit)
            if dinitctl status laya-server >/dev/null 2>&1; then dinitctl reload laya-server || return 1; fi
            if [ ! -L /etc/dinit.d/boot.d/laya-server ]; then dinitctl enable laya-server || return 1; fi
            [ "$(readlink -f /etc/dinit.d/boot.d/laya-server)" = /etc/dinit.d/laya-server ] || return 1
            dinitctl start laya-server && dinitctl is-started laya-server;;
    esac
}
service_remove() {
    case "$manager" in
        systemd) systemctl disable laya-server && rm -f /etc/systemd/system/laya-server.service && systemctl daemon-reload;;
        openrc) rc-update del laya-server default && rm -f /etc/init.d/laya-server;;
        runit) rm -f /var/service/laya-server && rm -rf /etc/sv/laya-server;;
        dinit) dinitctl disable laya-server && dinitctl unload laya-server && rm -f /etc/dinit.d/laya-server;;
    esac
}
write_service() {
    case "$manager" in
        systemd) write_systemd;; openrc) write_openrc;; runit) write_runit;; dinit) write_dinit;;
    esac
}
write_systemd() {
    cat > /etc/systemd/system/laya-server.service <<'EOF'
[Unit]
Description=Laya System One
After=network.target
[Service]
Type=simple
User=laya-server
Group=laya-server
EnvironmentFile=/etc/laya-server/token.env
ExecStart=/opt/laya-server/current/launch
Restart=on-failure
RestartSec=5
TimeoutStopSec=135
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
UMask=0077
[Install]
WantedBy=multi-user.target
EOF
    chmod 644 /etc/systemd/system/laya-server.service
}
write_openrc() {
    prepare_log
    cat > /etc/init.d/laya-server <<'EOF'
#!/sbin/openrc-run
description="Laya System One"
supervisor=supervise-daemon
command=/opt/laya-server/current/launch
command_user=laya-server:laya-server
pidfile=/run/laya-server.pid
respawn_delay=5
respawn_max=5
respawn_period=600
retry="TERM/130/KILL/5"
output_log=/var/log/laya-server.log
error_log=/var/log/laya-server.log
LAYA_API_TOKEN=$(sed -n 's/^LAYA_API_TOKEN=//p' /etc/laya-server/token.env)
export LAYA_API_TOKEN
depend() { need net; }
EOF
    chmod 755 /etc/init.d/laya-server
}
write_runit() {
    mkdir -p /etc/sv/laya-server
    chmod 755 /etc/sv/laya-server
    cat > /etc/sv/laya-server/run <<'EOF'
#!/bin/sh
set -eu
LAYA_API_TOKEN=$(sed -n 's/^LAYA_API_TOKEN=//p' /etc/laya-server/token.env)
export LAYA_API_TOKEN
exec chpst -u laya-server:laya-server /opt/laya-server/current/launch
EOF
    chmod 755 /etc/sv/laya-server/run
}
write_dinit() {
    prepare_log
    cat > /etc/dinit.d/laya-server <<'EOF'
type = process
command = /opt/laya-server/current/launch
env-file = /etc/laya-server/token.env
run-as = laya-server
restart = true
stop-timeout = 135
logfile = /var/log/laya-server.log
EOF
    chmod 644 /etc/dinit.d/laya-server
}
prepare_log() {
    [ ! -L /var/log/laya-server.log ] || die '拒绝使用符号链接日志文件'
    touch /var/log/laya-server.log
    chown laya-server:laya-server /var/log/laya-server.log
    chmod 640 /var/log/laya-server.log
}
check_service_conflicts() {
    [ -f "$CONF/manager" ] && return 0
    for path in /etc/systemd/system/laya-server.service /usr/lib/systemd/system/laya-server.service /lib/systemd/system/laya-server.service /etc/init.d/laya-server /etc/sv/laya-server /var/service/laya-server /etc/dinit.d/laya-server; do
        if [ -e "$path" ] || [ -L "$path" ]; then die "拒绝覆盖既有服务：$path"; fi
    done
}
check_api() {
    api_port=$(field "$CONF/server.conf" PORT) || return 1
    api_token=$(field "$CONF/token.env" LAYA_API_TOKEN) || return 1
    printf 'Authorization: Bearer %s\n' "$api_token" > "$work/auth-header" || return 1
    unset api_token
    ready=0; tries=0
    while [ "$tries" -lt 150 ]; do
        if curl -q --noproxy '*' -fsS --connect-timeout 2 --max-time 2 "http://127.0.0.1:$api_port/readyz" -o "$work/ready.json" 2>/dev/null &&
            "$ROOT/current/exec" jq -e '.status == "ready"' "$work/ready.json" >/dev/null; then ready=1; break; fi
        tries=$((tries + 1)); sleep 2
    done
    [ "$ready" = 1 ] || { say '等待服务就绪超时，请检查服务管理器日志。' >&2; return 1; }
    curl -q --noproxy '*' -fsS --connect-timeout 5 --max-time 150 \
        -H "@$work/auth-header" -H 'Content-Type: application/json' \
        --data-binary "@$ROOT/current/smoke.json" "http://127.0.0.1:$api_port/v1/system-one" -o "$work/response.json" || return 1
    "$ROOT/current/exec" jq -e -f "$ROOT/current/response.jq" "$work/response.json" >/dev/null
}
rollback() {
    say '安装验收失败，正在停止新服务并恢复原状态。' >&2
    service_stop || return 1
    if [ -n "$old" ]; then
        cp "$work/old-server.conf" "$CONF/server.conf" || return 1
        cp "$work/old-token.env" "$CONF/token.env" || return 1
        cp "$work/old-service" "$(service_file)" || return 1
        rm -f "$ROOT/rollback-link" || return 1
        ln -s "$old" "$ROOT/rollback-link" || return 1
        mv -Tf "$ROOT/rollback-link" "$ROOT/current" || return 1
        service_start && check_api || return 1
        say '旧版本已恢复并通过真实推理检查。' >&2
    else
        service_remove || return 1
        rm -f "$ROOT/current" "$CONF/manager"
        say '首次安装未完成；已移除服务，保留下载和配置，供重试。' >&2
    fi
}
cleanup() {
    code=$?
    trap - EXIT HUP INT TERM
    [ -z "$restore_echo" ] || stty "$restore_echo" <&3
    keep_work=0
    if [ -n "$switched" ] && [ "$code" -ne 0 ]; then
        if ! rollback; then
            keep_work=1
            say "自动恢复失败；备份保留在 ${work}，请检查服务管理器日志，不要删除旧版本。" >&2
        fi
    fi
    if [ "$keep_work" = 0 ] && [ -n "$work" ]; then rm -rf "$work"; fi
    [ -z "$model_stage" ] || rm -rf "$model_stage"
    [ -z "$locked" ] || rm -rf "$LOCK"
    exit "$code"
}
resolve_release() {
    version=${1:-}
    if [ -z "$version" ]; then
        resolved=$(curl -q -fLsS --proto '=https' --proto-redir '=https' --connect-timeout 20 --max-time 60 -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest")
        version=${resolved##*/}
    fi
    version_ok "$version" || die '没有可用的稳定版本，或版本参数非法'
    base=https://github.com/$REPO/releases/download/$version
    download "$base/release.env" "$work/release.env"
    [ "$(field "$work/release.env" VERSION)" = "$version" ] || die '发布版本不匹配'
    model_id=$(field "$work/release.env" MODEL_ID)
    case "$model_id" in ''|*[!0-9a-f]*) die '模型版本非法';; esac
    [ "${#model_id}" = 16 ] || die '模型版本非法'
    runtime_sha=$(field "$work/release.env" "RUNTIME_${arch}_SHA256")
    model_sha=$(field "$work/release.env" MODEL_SHA256)
    runtime_bytes=$(field "$work/release.env" "RUNTIME_${arch}_BYTES")
    model_bytes=$(field "$work/release.env" MODEL_BYTES)
    if ! number "$runtime_bytes" 2147483647 || ! number "$model_bytes" 2147483647; then die '下载大小非法'; fi
}
choose_config() {
    cancelled=0
    load_config
    check_service_conflicts
    if [ -f "$CONF/manager" ]; then [ "$(cat "$CONF/manager")" = "$manager" ] || die '服务管理器发生变化，请先卸载'; fi
    if [ -L "$ROOT/current" ]; then
        old=$(readlink "$ROOT/current")
        case "$old" in "$ROOT"/versions/*) ;; *) die '现有版本链接非法';; esac
        say "已安装 $(cat "$old/VERSION")；本次将升级或修复到 ${version}。"
    fi
    say "安装计划：$version / $arch / ${manager}；程序 ${ROOT}；模型 ${DATA}；配置 $CONF"
    say "下载：程序 $runtime_bytes bytes，模型 $model_bytes bytes（已校验模型可复用）。"
    say "监听 0.0.0.0:${port}；线程 ${threads}；并发 ${slots}；不修改防火墙。"
    ask '回车确认安装，输入 e 修改配置，其他输入取消：'
    case "$answer" in '') ;; e|E) edit_config; say "监听 0.0.0.0:${port}；线程 ${threads}；并发 $slots"; confirm '确认执行安装？' || cancelled=1;; *) say '已取消。'; cancelled=1;; esac
}
prepare_runtime() {
    # Leave room for both compressed and expanded model plus rollback copies.
    free=$(df -Pk /var/lib | awk 'END {print $4}')
    [ "$free" -ge 4194304 ] || die '/var/lib 至少需要 4 GiB 可用空间'
    checked_download "$base/laya-server-linux-$arch.tar.gz" "$work/runtime.tgz" "$runtime_sha"
    unpack "$work/runtime.tgz" "$work/runtime"
    if [ "$(cat "$work/runtime/VERSION")" != "$version" ] || [ "$(cat "$work/runtime/MODEL_ID")" != "$model_id" ]; then die '程序包元数据不匹配'; fi
    "$work/runtime/exec" laya-server --help >/dev/null || die '程序或私有运行库无法在本机运行'
    for dir in "$ROOT" "$DATA"; do make_directory "$dir" 755; done
    make_directory "$CONF" 700
}
prepare_model() {
    model=$DATA/models/$model_id
    if [ -d "$model" ] && (cd "$model" && sha256sum -c "$work/runtime/model-files.sha256" >/dev/null 2>&1); then
        say '复用校验通过的模型。'
    else
        [ ! -e "$model" ] || die '现有模型损坏；请将该模型目录移走后重试，未停止旧服务'
        checked_download "https://github.com/$REPO/releases/download/model-$model_id/model-$model_id.tar.gz" "$work/model.tgz" "$model_sha"
        mkdir -p "$DATA/models"
        chmod 755 "$DATA/models"
        model_stage=$DATA/models/.new-$$
        unpack "$work/model.tgz" "$model_stage"
        (cd "$model_stage" && sha256sum -c "$work/runtime/model-files.sha256") || die '模型内容校验失败'
        chmod -R a+rX "$model_stage"
        mv "$model_stage" "$model"
        model_stage=
    fi
}
prepare_version() {
    create_user
    if [ -z "$token" ]; then token=$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n'); fi
    token_ok "$token" || die '密钥格式非法'
    mkdir -p "$ROOT/versions"
    chmod 755 "$ROOT/versions"
    destination=$ROOT/versions/$version-$(date +%s)-$$
    mv "$work/runtime" "$destination"
    chmod -R a+rX "$destination"
    cat > "$destination/launch" <<EOF
#!/bin/sh
exec '$destination/exec' laya-server --model '$model' --ort-library '$destination/lib/libonnxruntime.so' --listen '0.0.0.0:$port' --threads '$threads' --inter-op-threads 1 --max-concurrency '$slots' --shutdown-grace 120
EOF
    chmod 755 "$destination/launch"
}
activate_version() {
    if [ -n "$old" ]; then
        cp "$CONF/server.conf" "$work/old-server.conf"; cp "$CONF/token.env" "$work/old-token.env"
        cp "$(service_file)" "$work/old-service"
        switched=1
        service_stop
    fi
    switched=1
    printf 'PORT=%s\nTHREADS=%s\nSLOTS=%s\n' "$port" "$threads" "$slots" > "$CONF/server.conf"
    printf 'LAYA_API_TOKEN=%s\n' "$token" > "$CONF/token.env"
    chmod 600 "$CONF/token.env" "$CONF/server.conf"
    unset token
    printf '%s\n' "$manager" > "$CONF/manager"
    rm -f "$ROOT/new-link"
    ln -s "$destination" "$ROOT/new-link"
    mv -Tf "$ROOT/new-link" "$ROOT/current"
    write_service
    service_start
    say '等待模型就绪并执行中文分类、评分、命题概率验收……'
    check_api || die '真实 API 推理验收失败'
    switched=
}
install_service() {
    ensure_tools
    resolve_release "${1:-}"
    choose_config
    [ "$cancelled" = 0 ] || return 0
    prepare_runtime
    prepare_model
    prepare_version
    activate_version
    say "安装成功：${version}，监听 0.0.0.0:${port}，已配置开机自启并通过真实推理。"
    say "密钥保存在 $CONF/token.env（仅 root 可读）；读取命令：sudo cat $CONF/token.env"
    say "就绪检查：curl -fsS http://127.0.0.1:$port/readyz"
    say '从其他机器访问时使用本机 IP，并按需配置防火墙；生产 HTTPS 由网关提供。'
}
uninstall_service() {
    [ -d "$CONF" ] || { say '未发现本安装器的安装。'; return 0; }
    if [ -f "$CONF/manager" ]; then [ "$(cat "$CONF/manager")" = "$manager" ] || die '服务管理器发生变化，请先恢复原管理器'; fi
    confirm '卸载服务和程序？默认保留模型与配置。' || return 0
    purge=
    if confirm '同时彻底删除模型和配置（含 API 密钥）？'; then
        ask '输入 DELETE 再次确认彻底删除：'
        [ "$answer" = DELETE ] || die '未确认彻底删除，取消卸载'
        purge=1
    fi
    if [ -f "$CONF/manager" ]; then service_stop; service_remove; rm -f "$CONF/manager"; fi
    rm -rf "$ROOT"
    if [ -n "$purge" ]; then
        if [ -f "$CONF/user-owned" ] && id laya-server >/dev/null 2>&1; then
            if command -v userdel >/dev/null 2>&1; then userdel laya-server
            else deluser laya-server; fi
        fi
        rm -rf "$DATA" "$CONF"
        case "$manager" in openrc|dinit) rm -f /var/log/laya-server.log;; esac
        say '服务、程序、模型和配置已删除；系统共享依赖保留。'
    else say '服务和程序已卸载；模型、配置及专用用户保留，可重新安装。'; fi
}
main() {
    case "${1:-install}" in install|uninstall) action=${1:-install};; *) die '用法：install.sh [install [v版本] | uninstall]';; esac
    [ "$#" -le 2 ] || die '参数过多'
    # The script itself may arrive on stdin; all prompts use the controlling terminal.
    if ! ( : <> /dev/tty ) 2>/dev/null; then die '需要可交互终端，未执行安装或卸载'; fi
    exec 3<> /dev/tty
    if [ "$(id -u)" != 0 ]; then
        command -v sudo >/dev/null 2>&1 || die '需要 root 或 sudo 权限'
        [ -f "$0" ] || die '请先下载脚本再执行，以便通过 sudo 提权'
        exec sudo sh "$0" "$@"
    fi
    trap cleanup EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM HUP
    preflight
    case "$action" in install) install_service "${2:-}";; uninstall) uninstall_service;; esac
}

# Sourcing exposes functions for isolated tests; executing always enforces preflight.
if [ "${LAYA_INSTALLER_TEST_SOURCE:-0}" != 1 ]; then main "$@"; fi
