#!/bin/sh
# Linux release gate; uses the real model, no Python runtime and no root requirement.
set -eu
package=${1:?usage: smoke.sh PACKAGE_DIRECTORY MODEL_DIRECTORY}
model=${2:?}
package=$(cd "$package" && pwd)
model=$(cd "$model" && pwd)
work=$(mktemp -d)
pid=''
cleanup() {
    code=$?
    trap - EXIT HUP INT TERM
    if [ -n "$pid" ]; then kill -TERM "$pid" 2>/dev/null || :; wait "$pid" || :; fi
    if [ "$code" -ne 0 ]; then cat "$work/server.log" >&2; fi
    rm -rf "$work"
    exit "$code"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' HUP TERM
umask 077
LAYA_API_TOKEN=$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')
export LAYA_API_TOKEN
printf 'Authorization: Bearer %s\n' "$LAYA_API_TOKEN" > "$work/header"
"$package/exec" laya-server --model "$model" --ort-library "$package/lib/libonnxruntime.so" \
    --listen 127.0.0.1:18089 --threads 2 --max-concurrency 1 > "$work/server.log" 2>&1 &
pid=$!
n=0
until curl -q --noproxy '*' -fsS --max-time 2 http://127.0.0.1:18089/readyz > "$work/ready" 2>/dev/null; do
    kill -0 "$pid"; n=$((n + 1)); [ "$n" -lt 150 ]; sleep 2
done
"$package/exec" jq -e '.status == "ready"' "$work/ready" >/dev/null
curl -q --noproxy '*' -fsS --max-time 150 -H "@$work/header" -H 'Content-Type: application/json' \
    --data-binary "@$package/smoke.json" http://127.0.0.1:18089/v1/system-one > "$work/response"
"$package/exec" jq -e -f "$package/response.jq" "$work/response" >/dev/null
printf 'PASS: %s real Chinese Choice/Score/Noul API inference\n' "$(uname -m)"
