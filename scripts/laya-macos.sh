#!/bin/bash
# macOS system Bash 3.2; configuration is data, never sourced or evaluated.
set -euo pipefail
set +x
umask 077
PATH=/usr/bin:/bin:/usr/sbin:/sbin
export PATH
export LC_ALL=C
export COPYFILE_DISABLE=1
unset LAYA_API_TOKEN

die() { printf 'laya: %s\n' "$*" >&2; exit 1; }
say() { printf '%s\n' "$*"; }
field() { awk -F= -v key="$2" '$1==key {sub(/^[^=]*=/, ""); print; n++} END {if(n!=1) exit 1}' "$1"; }
number() { [[ $1 =~ ^[1-9][0-9]{0,9}$ ]] && (( $1 <= $2 )); }
version_ok() { [[ $1 =~ ^v[0-9]+[A-Za-z0-9.+-]*$ ]]; }
hash_ok() { [[ $1 =~ ^[0-9a-f]{64}$ ]]; }
safe_path() { [[ $1 = /* && $1 != / && ! $1 =~ [[:cntrl:]] ]]; }
asset_path() { [[ $1 =~ ^[A-Za-z0-9_./+-]+$ && $1 != /* && /$1/ != */../* && /$1/ != */./* ]]; }

usage() {
    cat <<'EOF'
Usage: bash laya-macos.sh install|start|stop|update [options]
  --root DIR                 Default: ~/Library/Application Support/laya-server
  --version TAG              install/update: latest stable or explicit v... tag
  --release-directory DIR    install/update: local verified release candidate
  --program-directory DIR    install: dedicated directory for versioned programs
  --model-directory DIR      install: existing fixed model (verified, not modified)
  --listen IP:PORT           install: default 0.0.0.0:8080
  --threads N                install: default 2
  --inter-op-threads N       install: default 1
  --max-concurrency N        install: default 1
  --shutdown-grace SECONDS   install: default 120
  --recover                  update: restore an interrupted transaction
Start and running updates stay in this terminal. Use stop from another terminal.
EOF
}

parse_args() {
    action=${1:-}; [[ $# = 0 ]] || shift
    case $action in install|start|stop|update) ;; --help|-h) usage; exit 0;; *) usage; exit 2;; esac
    root="$HOME/Library/Application Support/laya-server"
    version=latest; release_directory=; programs=; model_override=; recover=0
    listen=0.0.0.0:8080; threads=2; inter=1; slots=1; grace=120
    while (( $# )); do
        if [[ $1 = --recover ]]; then recover=1; shift; continue; fi
        (( $# >= 2 )) || die 'Option requires a value'
        case $1 in
            --root) root=$2;;
            --version|--release-directory)
                [[ $action = install || $action = update ]] || die 'Download options require install/update'
                if [[ $1 = --version ]]; then version=$2; else release_directory=$2; fi;;
            --program-directory|--model-directory|--listen|--threads|--inter-op-threads|--max-concurrency|--shutdown-grace)
                [[ $action = install ]] || die 'Configuration options require install'
                case $1 in
                    --program-directory) programs=$2;; --model-directory) model_override=$2;;
                    --listen) listen=$2;; --threads) threads=$2;; --inter-op-threads) inter=$2;;
                    --max-concurrency) slots=$2;; --shutdown-grace) grace=$2;;
                esac;;
            *) die 'Unknown option';;
        esac
        shift 2
    done
    number "$threads" 2147483647 || die 'Invalid threads'
    number "$inter" 2147483647 || die 'Invalid inter-op threads'
    number "$slots" 1024 || die 'Invalid max-concurrency'
    number "$grace" 86400 || die 'Invalid shutdown grace'
    [[ $version = latest ]] || version_ok "$version" || die 'Invalid version'
    [[ $recover = 0 || $action = update ]] || die '--recover requires update'
    endpoint "$listen" || die 'Invalid listen endpoint'
}

endpoint() {
    # curl accepts numeric IPv4/IPv6 only; the application's SocketAddr validates it too.
    [[ $1 =~ ^(\[[0-9a-fA-F:]+\]|[0-9.]+):([1-9][0-9]*)$ ]] || return 1
    port=${BASH_REMATCH[2]}; number "$port" 65535 || return 1
    address=${1%:*}
    case $address in 0.0.0.0) address=127.0.0.1;; '[::]') address='[::1]';; esac
    base="http://$address:$port"
}

canonical_directory() {
    [[ -d $1 && ! -L $1 ]] || die 'Directory is missing or is a symlink'
    (cd "$1" && pwd -P)
}

protect_directory() {
    [[ -d $1 && ! -L $1 && $(stat -f %u "$1") = "$uid" ]] || die 'Unsafe directory owner or link'
    chmod -N "$1"
    chmod 700 "$1"
}

managed_directory() {
    safe_path "$1" || die 'Use an absolute, dedicated directory without control characters'
    if [[ ! -e $1 ]]; then mkdir -p "$1"; fi
    [[ ! -L $1 && -d $1 ]] || die 'Managed directory cannot be a link'
    if [[ -f $1/.laya-owner && ! -L $1/.laya-owner ]]; then
        [[ $(cat "$1/.laya-owner") = "$uid:$root" ]] || die 'Directory belongs to another deployment'
    elif [[ -n $(ls -A "$1") ]]; then die 'Refusing a nonempty unmanaged directory'; fi
    protect_directory "$1"
    printf '%s\n' "$uid:$root" > "$1/.laya-owner"
}

open_root() {
    [[ $(uname -s) = Darwin && $(uname -m) = arm64 ]] || die 'Native Apple Silicon macOS is required'
    [[ $(sysctl -in sysctl.proc_translated 2>/dev/null || true) != 1 ]] || die 'Rosetta is unsupported'
    os_version=$(sw_vers -productVersion)
    (( ${os_version%%.*} >= 14 )) || die 'macOS 14 or newer is required'
    uid=$(id -u); (( uid != 0 )) || die 'Run as a normal user, without sudo'
    safe_path "$root" || die 'Root must be an absolute dedicated directory'
    [[ ! -L $root ]] || die 'Root cannot be a symlink'
    mkdir -p "$root"
    root=$(canonical_directory "$root")
    managed_directory "$root"
    for dir in staging run models; do
        [[ ! -L $root/$dir ]] || die 'Managed subdirectory cannot be a link'
        mkdir -p "$root/$dir"; protect_directory "$root/$dir"
    done
}

acquire_lock() {
    if ! mkdir "$root/operation.lock" 2>/dev/null; then
        die 'Operation lock exists. Check operation.lock/owner; see docs/macos.md for stale-lock recovery'
    fi
    locked=1
    printf '%s\n' "PID=$$" "UID=$uid" "START=$(ps -p $$ -o lstart=)" > "$root/operation.lock/owner"
}

release_lock() {
    if [[ $locked = 1 ]]; then rm "$root/operation.lock/owner"; rmdir "$root/operation.lock"; locked=0; fi
}

transfer() {
    local status=0
    curl "$@" &
    transfer_pid=$!
    wait "$transfer_pid" || status=$?
    transfer_pid=
    return "$status"
}

download() {
    transfer -q -fsSL --proto '=https' --proto-redir '=https' --tlsv1.2 \
        --connect-timeout 20 --max-time 1800 --max-filesize "$3" \
        --speed-time 60 --speed-limit 1024 --output "$2.part" "$1"
    mv "$2.part" "$2"
}

check_space() {
    local available
    available=$(df -Pk "$1" | awk 'END {print $4}')
    [[ $available =~ ^[0-9]+$ ]] || die 'Cannot measure free space'
    (( available * 1024 >= $2 )) || die 'Insufficient staging/expansion space'
}

metadata() {
    local tag=$version url key
    if [[ -n $release_directory ]]; then cp "$release_directory/release.env" "$work/release.env"
    else
        if [[ $tag = latest ]]; then
            transfer -q -fsSLI --proto '=https' --proto-redir '=https' --connect-timeout 20 \
                --max-time 60 -o /dev/null -w '%{url_effective}' "$releases/latest" > "$work/latest-url"
            url=$(cat "$work/latest-url")
            [[ $url = "$releases/tag/"* ]] || die 'Unexpected stable release redirect'
            tag=${url#"$releases/tag/"}
        fi
        version_ok "$tag" || die 'Invalid resolved version'
        download "$releases/download/$tag/release.env" "$work/release.env" 65536
    fi
    [[ $(stat -f %z "$work/release.env") -le 65536 ]] || die 'Oversized release metadata'
    awk -F= '!/^[A-Za-z0-9_]+=[^\r]*$/ || seen[$1]++ {bad=1} END {exit bad}' "$work/release.env" || die 'Invalid metadata'
    version=$(field "$work/release.env" VERSION); model_id=$(field "$work/release.env" MODEL_ID)
    version_ok "$version" && [[ $model_id =~ ^[0-9a-f]{16}$ ]] || die 'Invalid release identifiers'
    [[ $tag = latest || $tag = "$version" ]] || die 'Release version mismatch'
    for key in MODEL_SHA256 MACOS_aarch64_SHA256; do hash_ok "$(field "$work/release.env" "$key")" || die 'Invalid release hash'; done
    for key in MODEL_BYTES MACOS_aarch64_BYTES; do number "$(field "$work/release.env" "$key")" 4294967296 || die 'Invalid release size'; done
}

fetch_asset() {
    local name=$1 target=$2 hash=$3 bytes=$4 tag=$5 quarantine
    check_space "$work" "$((bytes + 8589934592))"
    if [[ -n $release_directory ]]; then
        cp "$release_directory/$name" "$target"
        if quarantine=$(xattr -p com.apple.quarantine "$release_directory/$name" 2>/dev/null); then
            xattr -w com.apple.quarantine "$quarantine" "$target"
        fi
    else download "$releases/download/$tag/$name" "$target" "$bytes"; fi
    [[ $(stat -f %z "$target") = "$bytes" && $(shasum -a 256 < "$target" | cut -d ' ' -f 1) = "$hash" ]] \
        || die 'Asset size or SHA-256 verification failed; old deployment is unchanged'
}

unpack() {
    local archive=$1 destination=$2 quarantine
    tar -tzf "$archive" > "$work/members"
    awk '{sub(/^\.\//, ""); sub(/\/$/, ""); if($0=="") next;
        if($0 !~ /^[A-Za-z0-9_.+\/-]+$/ || $0 ~ /^\// || $0 ~ /(^|\/)\.\.?($|\/)/ || seen[$0]++) bad=1}
        END {if(NR>10000) bad=1; exit bad}' "$work/members" || die 'Unsafe or duplicate archive path'
    tar -tvzf "$archive" > "$work/types"
    awk 'substr($0,1,1)!="-" && substr($0,1,1)!="d" {bad=1}
        {size+=$5} END {if(size>4294967296) bad=1; exit bad}' "$work/types" || die 'Archive contains links, special files or excessive expansion'
    mkdir "$destination"
    tar -xzf "$archive" -C "$destination" --no-same-owner --no-same-permissions --no-acls --no-xattrs --no-fflags
    if quarantine=$(xattr -p com.apple.quarantine "$archive" 2>/dev/null); then
        xattr -r -w com.apple.quarantine "$quarantine" "$destination"
    fi
}

verify_files() {
    local directory=$1 catalog=$2 hash bytes name extra path
    [[ -d $directory && ! -L $directory && -f $catalog && ! -L $catalog ]] || return 1
    [[ -z $(find "$directory" -type l -print -quit) ]] || return 1
    [[ $(wc -l < "$catalog") -gt 0 ]] || return 1
    while IFS=$'\t' read -r hash bytes name extra; do
        hash_ok "$hash" && [[ $bytes =~ ^[0-9]+$ && ${#bytes} -le 10 && -z $extra ]] && asset_path "$name" || return 1
        path="$directory/$name"
        [[ -f $path && $(stat -f %z "$path") = "$bytes" ]] || return 1
        [[ $(shasum -a 256 < "$path" | cut -d ' ' -f 1) = "$hash" ]] || return 1
    done < "$catalog"
}

verify_program() {
    local directory=$1 tag=$2 id=$3 name
    verify_files "$directory" "$directory/program-files.tsv" || return 1
    for name in laya-server libonnxruntime.dylib model-manifest.json model-files.tsv smoke.json response-macos.js VERSION MODEL_ID; do
        awk -F '\t' -v name="$name" '$3==name {n++} END {exit n!=1}' "$directory/program-files.tsv" || return 1
    done
    [[ -x $directory/laya-server && $(cat "$directory/VERSION") = "$tag" && $(cat "$directory/MODEL_ID") = "$id" ]] || return 1
    [[ $(shasum -a 256 < "$directory/model-manifest.json" | cut -c 1-16) = "$id" ]]
}

verify_model() { verify_files "$1" "$2/model-files.tsv"; }

prepare_model() {
    local cached archive
    if [[ -n $model_override ]]; then
        model=$(canonical_directory "$model_override")
        verify_model "$model" "$program" || die 'Explicit model directory is corrupt'
        return
    fi
    if [[ -f $root/current.env && $(field "$root/current.env" MODEL_ID) = "$model_id" ]]; then
        cached=$(field "$root/current.env" MODEL)
        if verify_model "$cached" "$program"; then model=$cached; return; fi
    fi
    model="$root/models/$model_id"
    if verify_model "$model" "$program"; then return; fi
    archive="$work/model.tar.gz"
    fetch_asset "model-$model_id.tar.gz" "$archive" "$(field "$work/release.env" MODEL_SHA256)" \
        "$(field "$work/release.env" MODEL_BYTES)" "model-$model_id"
    unpack "$archive" "$work/model"
    verify_model "$work/model" "$program" || die 'Fixed model verification failed'
    if [[ -e $model ]]; then model="$model-$(basename "$work")"; fi
    mv "$work/model" "$model"
}

prepare_candidate() {
    local hash destination
    metadata
    hash=$(field "$work/release.env" MACOS_aarch64_SHA256)
    fetch_asset laya-server-macos-aarch64.tar.gz "$work/program.tar.gz" "$hash" \
        "$(field "$work/release.env" MACOS_aarch64_BYTES)" "$version"
    unpack "$work/program.tar.gz" "$work/program"
    program="$work/program"
    verify_program "$program" "$version" "$model_id" || die 'Program verification failed'
    prepare_model
    check_space "$programs" 4294967296
    destination="$programs/$version-${hash:0:16}"
    if [[ -e $destination ]]; then verify_program "$destination" "$version" "$model_id" || die 'Cached program is corrupt'
    else mv "$program" "$destination"; fi
    program=$destination
    printf '%s\n' "VERSION=$version" "MODEL_ID=$model_id" "PROGRAM=$program" "PROGRAMS=$programs" \
        "MODEL=$model" "LISTEN=$listen" "THREADS=$threads" "INTER=$inter" "SLOTS=$slots" "GRACE=$grace" > "$work/new.env"
}

read_token() {
    [[ -f $root/token && ! -L $root/token && $(stat -f %u "$root/token") = "$uid" ]] || die 'Missing or unsafe token; never regenerated for an existing installation'
    chmod -N "$root/token" || die 'Cannot remove token ACL'
    chmod 600 "$root/token" || die 'Cannot restrict token permissions'
    [[ $(stat -f %z "$root/token") -le 4096 ]] || die 'Invalid token file'
    token=$(cat "$root/token")
    [[ $token =~ ^[A-Za-z0-9._~+/-]+=*$ ]] || die 'Invalid token format'
}

install() {
    [[ ! -e $root/current.env ]] || die 'Already installed; use update'
    programs=${programs:-$root/programs}
    managed_directory "$programs"
    programs=$(canonical_directory "$programs")
    prepare_candidate
    if [[ ! -e $root/token ]]; then
        od -An -N32 -tx1 /dev/urandom | tr -d ' \n' > "$work/token"
        printf '\n' >> "$work/token"
        chmod -N "$work/token"; chmod 600 "$work/token"
        mv "$work/token" "$root/token"
    fi
    read_token
    mv "$work/new.env" "$root/current.env"
    say "Prepared $version. Run start in the terminal; install does not start a process."
}

load_config() {
    local file=$1
    [[ -f $file && ! -L $file ]] || die 'Run install first; configuration is missing or unsafe'
    version=$(field "$file" VERSION); model_id=$(field "$file" MODEL_ID)
    program=$(field "$file" PROGRAM); programs=$(field "$file" PROGRAMS); model=$(field "$file" MODEL)
    listen=$(field "$file" LISTEN); threads=$(field "$file" THREADS); inter=$(field "$file" INTER)
    slots=$(field "$file" SLOTS); grace=$(field "$file" GRACE)
    version_ok "$version" && [[ $model_id =~ ^[0-9a-f]{16}$ ]] || die 'Invalid configuration identifiers'
    if ! safe_path "$programs" || ! safe_path "$program" || ! safe_path "$model"; then die 'Invalid configuration paths'; fi
    [[ $program = "$programs/"* && $program != *'/../'* ]] || die 'Program is outside the deployment'
    if ! number "$threads" 2147483647 || ! number "$inter" 2147483647 || ! number "$slots" 1024 || ! number "$grace" 86400; then die 'Invalid configuration limits'; fi
    endpoint "$listen" || die 'Invalid listen endpoint'
    [[ ! -L $programs && $(cat "$programs/.laya-owner") = "$uid:$root" ]] || die 'Wrong program directory owner'
}

alive() {
    kill -0 "$1" 2>/dev/null && [[ $(ps -p "$1" -o stat=) != *Z* ]]
}

owned_process() {
    local record="$root/process.env" stamp executable instance
    pid=
    [[ -e $record ]] || return 1
    [[ -f $record && ! -L $record ]] || die 'Unsafe process record'
    pid=$(field "$record" PID)
    number "$pid" 2147483647 || die 'Invalid process PID'
    [[ $(field "$record" ROOT) = "$root" && $(field "$record" UID) = "$uid" ]] || die 'Process belongs to another deployment'
    alive "$pid" || return 1
    stamp=$(field "$record" START); executable=$(field "$record" EXECUTABLE); instance=$(field "$record" INSTANCE)
    [[ $instance =~ ^[0-9a-f]{32}$ && $executable = "$programs/"*/laya-server ]] || die 'Invalid process identity'
    if [[ $(ps -p "$pid" -o uid= | tr -d ' ') != "$uid" || $(ps -p "$pid" -o lstart=) != "$stamp" ||
        $(LC_ALL=en_US.UTF-8 ps -ww -p "$pid" -o comm=) != "$executable" ]]; then
        alive "$pid" || return 1
        die 'PID ownership cannot be confirmed; refusing to stop or adopt it'
    fi
    if ! LC_ALL=en_US.UTF-8 lsof -a -p "$pid" -d cwd -Fn 2>/dev/null | grep -Fxq "n$root/run/$instance"; then
        alive "$pid" || return 1
        die 'Deployment instance ownership cannot be confirmed'
    fi
}

stop_owned() {
    local stopped count=0
    if ! owned_process; then rm -f "$root/process.env"; return; fi
    stopped=$pid
    kill -TERM "$pid" || return 1
    while alive "$stopped"; do
        owned_process || break
        (( count < (grace + 15) * 5 )) || { say 'Managed process did not exit; no SIGKILL was sent' >&2; return 1; }
        sleep 0.2; count=$((count + 1))
    done
    if [[ $child_pid = "$stopped" ]]; then wait "$child_pid" || say 'Managed process exited nonzero; inspect its server.log' >&2; child_pid=; fi
    rm -f "$root/process.env"
}

launch() {
    local instance executable stamp
    if owned_process; then die 'A managed instance is already running'; fi
    verify_program "$program" "$version" "$model_id" || return 1
    verify_model "$model" "$program" || return 1
    read_token
    instance=$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')
    mkdir "$root/run/$instance" || return 1
    protect_directory "$root/run/$instance"
    executable="$program/laya-server"
    (
        cd "$root/run/$instance" || exit 1
        export LAYA_API_TOKEN="$token"
        exec "$executable" --model "$model" --listen "$listen" --threads "$threads" \
            --inter-op-threads "$inter" --max-concurrency "$slots" --shutdown-grace "$grace"
    ) > "$root/run/$instance/server.log" 2>&1 &
    child_pid=$!
    stamp=$(ps -p "$child_pid" -o lstart=) || return 1
    printf '%s\n' "PID=$child_pid" "UID=$uid" "ROOT=$root" "START=$stamp" \
        "EXECUTABLE=$executable" "INSTANCE=$instance" > "$work/process.env" || return 1
    mv "$work/process.env" "$root/process.env" || return 1
}

listener_owned() {
    alive "$child_pid" || return 1
    local listeners
    listeners=$(lsof -nP -a -iTCP:"$port" -sTCP:LISTEN -Fp 2>/dev/null | sed -n '/^p/p' | sort -u) || return 1
    [[ $listeners = "p$child_pid" ]] || return 1
    owned_process
}

http() {
    local path=$1 auth=$2; shift 2
    if [[ $auth = yes ]]; then printf 'header = "Authorization: Bearer %s"\n' "$token"; fi |
        curl -q -sS --noproxy '*' --connect-timeout 2 --max-time 125 --config - \
            --output "$work/response" --write-out '%{http_code}' "$base$path" "$@"
}

validate_running() {
    local count=0 status name
    endpoint "$listen" || return 1
    while (( count < 180 )); do
        alive "$child_pid" || return 1
        if listener_owned && [[ $(http /readyz no 2>/dev/null) = 200 ]] &&
            [[ $(plutil -extract status raw -o - "$work/response") = ready ]]; then break; fi
        count=$((count + 1)); sleep 1
    done
    (( count < 180 )) || return 1
    [[ $(http /healthz no) = 200 && $(plutil -extract status raw -o - "$work/response") = ok ]] || return 1
    [[ $(http /metrics no) = 401 && $(http /v1/system-one no -X POST) = 401 ]] || return 1
    [[ $(http /metrics yes) = 200 ]] || return 1
    for name in laya_requests_total laya_request_duration_seconds laya_inference_duration_seconds laya_queue_size laya_inference_inflight laya_errors_total; do
        grep -q "$name" "$work/response" || return 1
    done
    status=$(http /v1/system-one yes -H 'Content-Type: application/json' --data-binary "@$program/smoke.json") || return 1
    [[ $status = 200 ]] || return 1
    osascript -l JavaScript "$program/response-macos.js" "$work/response" || return 1
    listener_owned
}

console_wait() {
    local status=0 waited=$child_pid
    release_lock
    say 'Validated. This terminal owns the process; closing it can stop service. Use stop from another terminal.'
    wait "$waited" || status=$?
    child_pid=
    clear_child_record "$waited"
    return "$status"
}

clear_child_record() {
    if [[ $locked = 0 ]]; then
        # The stop/update owner may be reading the record while this parent reaps.
        mkdir "$root/operation.lock" 2>/dev/null || return 0
        locked=1
        printf '%s\n' "PID=$$" "UID=$uid" "START=$(ps -p $$ -o lstart=)" > "$root/operation.lock/owner"
    fi
    if [[ -f $root/process.env && $(field "$root/process.env" PID) = "$1" ]]; then rm "$root/process.env"; fi
}

start() {
    load_config "$root/current.env"
    if owned_process; then die 'A managed instance is already running'; fi
    if ! launch || ! validate_running; then die 'Candidate startup or real API validation failed; inspect run/*/server.log'; fi
    console_wait
}

commit_config() {
    cp "$1" "$work/current.env" || return 1
    mv -f "$work/current.env" "$root/current.env" || return 1
}

restore_transaction() {
    stop_owned || return 1
    load_config "$root/transaction/old.env"
    launch && validate_running || return 1
    if [[ $(cat "$root/transaction/was-running") = 0 ]]; then stop_owned || return 1; fi
    commit_config "$root/transaction/old.env" || return 1
    rm -r "$root/transaction" || return 1
    say 'Previous deployment restored and validated.'
}

update() {
    local requested=$version was_running=0 failed=0
    load_config "$root/current.env"
    read_token
    if [[ $recover = 1 ]]; then
        [[ -d $root/transaction && ! -L $root/transaction ]] || die 'No interrupted transaction to recover'
        restore_transaction || die 'Rollback failed. Keep transaction and run/*/server.log; diagnose, then use update --recover'
    else
        if owned_process; then was_running=1; fi
        version=$requested
        prepare_candidate
        mkdir "$work/transaction"
        cp "$root/current.env" "$work/transaction/old.env"
        cp "$work/new.env" "$work/transaction/new.env"
        printf '%s\n' "$was_running" > "$work/transaction/was-running"
        mv "$work/transaction" "$root/transaction"
        stop_owned || die 'Cannot confirm old process exit; transaction retained'
        if launch && validate_running; then
            if [[ $was_running = 0 ]]; then stop_owned || die 'Candidate did not stop; transaction retained'; fi
            cp "$root/transaction/old.env" "$work/previous.env"
            mv -f "$work/previous.env" "$root/previous.env"
            commit_config "$root/transaction/new.env"
            rm -r "$root/transaction"
            say "Committed $version; parameters, model and token preserved."
        else
            failed=1
            say 'Candidate failed; restoring the previous deployment.' >&2
            restore_transaction || die 'Rollback failed. Keep transaction and run/*/server.log; diagnose, then use update --recover'
        fi
    fi
    if [[ -n $child_pid ]]; then console_wait || return 1; fi
    return "$failed"
}

cleanup() {
    local status=$?
    trap - EXIT
    if [[ -n $transfer_pid ]]; then
        if alive "$transfer_pid" && [[ $(ps -p "$transfer_pid" -o ppid= | tr -d ' ') = "$$" ]]; then
            kill -TERM "$transfer_pid" || say 'Could not cancel download' >&2
        fi
        wait "$transfer_pid" || true
    fi
    if [[ -n $child_pid ]]; then
        if alive "$child_pid" && [[ $(ps -p "$child_pid" -o ppid= | tr -d ' ') = "$$" ]]; then
            kill -TERM "$child_pid" || say 'Could not signal own child' >&2
            count=0
            while alive "$child_pid" && (( count < (grace + 15) * 5 )); do sleep 0.2; count=$((count + 1)); done
        fi
        if alive "$child_pid"; then say 'Child did not exit; process record retained. Manual recovery required.' >&2
        else
            wait "$child_pid" || true
            clear_child_record "$child_pid"
        fi
    fi
    if [[ -n $work ]]; then rm -rf "$work"; fi
    release_lock
    exit "$status"
}

main() {
    parse_args "$@"
    open_root
    acquire_lock
    work=$(mktemp -d "$root/staging/operation.XXXXXX")
    protect_directory "$work"
    if [[ -e $root/transaction && $recover != 1 ]]; then die 'Interrupted transaction detected; inspect it and run update --recover'; fi
    case $action in
        install) install;; start) start;;
        stop) load_config "$root/current.env"; stop_owned; say 'Managed process stopped';;
        update) update;;
    esac
}

locked=0; work=; child_pid=; transfer_pid=; grace=120
releases=https://github.com/redwolf2019/laya-rs/releases
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
main "$@"
