#!/bin/sh
# Development acceptance inside laya-loader-8 after cargo build --locked.
set -eu
out=/tmp/observability-cli
mkdir -p "$out"
pid=
cleanup() {
    if [ -n "$pid" ]; then kill -KILL "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; fi
}
trap cleanup EXIT
base=http://127.0.0.1:18087
metric() { curl -fsS "$base/metrics"; }
wait_metric() {
    i=0
    until metric | grep -q "$1"; do
        i=$((i + 1)); test "$i" -lt 200; sleep 0.01
    done
}
for mode in drain expire idle; do
    grace=120
    inference=120
    if [ "$mode" = expire ]; then grace=1; inference=1; fi
    /target/debug/laya-server --model /work/models/multilingual \
        --ort-library /ort/lib/libonnxruntime.so --listen 127.0.0.1:18087 \
        --threads 1 --max-concurrency 1 --inference-timeout "$inference" \
        --shutdown-grace "$grace" >"$out/$mode.stdout" 2>"$out/$mode.stderr" &
    pid=$!
    i=0
    until curl -fsS "$base/readyz" > /dev/null 2>&1; do
        i=$((i + 1)); test "$i" -lt 600; sleep 0.1
    done
    echo "$mode: ready=200"
    code=$(curl -sS -o "$out/error.json" -w '%{http_code}' "$base/v1/system-one" \
        -H 'Content-Type: application/json' --data-binary '{"state":"secret-state-17","questions":{"private":{"type":"private","instructions":"secret-instructions-17"}}}')
    test "$code" = 400
    if [ "$mode" != idle ]; then
        curl -sS -o "$out/$mode.response" -w '%{http_code}' "$base/v1/system-one" \
            -H 'Content-Type: application/json' --data-binary @/work/target/observability-request.json >"$out/$mode.status" &
        request_pid=$!
        wait_metric '^laya_inference_inflight 1$'
        if [ "$mode" = expire ]; then
            wait "$request_pid"
            test "$(cat "$out/$mode.status")" = 504
            wait_metric '^laya_inference_inflight 1$'
            echo 'HTTP=504, actual CPU inflight=1'
        fi
        kill -TERM "$pid"
        code=$(curl -sS -o /dev/null -w '%{http_code}' "$base/readyz")
        test "$code" = 503
        echo "$mode: ready=503"
    else
        kill -INT "$pid"
    fi
    status=0
    wait "$pid" || status=$?
    pid=
    if [ "$mode" = expire ]; then test "$status" = 1; else test "$status" = 0; fi
    if [ "$mode" = drain ]; then wait "$request_pid"; test "$(cat "$out/$mode.status")" = 200; fi
    if grep -E 'secret-state-17|secret-instructions-17|/work/models' "$out/$mode.stderr" "$out/error.json"; then exit 1; fi
    echo "$mode: exit=$status; sanitized logs:"
    cat "$out/$mode.stderr"
done
