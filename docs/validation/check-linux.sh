#!/bin/bash
# 在一次性 Debian ARM64 容器中执行；/assets 为只读官方 ORT 包目录。
set -euxo pipefail
export LC_ALL=C

date -u +%FT%TZ
uname -srm
test "$(uname -m)" = aarch64
cat /etc/os-release
lscpu
cat /proc/meminfo
cat /sys/fs/cgroup/cpu.max /sys/fs/cgroup/cpuset.cpus.effective
cat /sys/fs/cgroup/memory.max /sys/fs/cgroup/memory.swap.max
test "$(cat /sys/fs/cgroup/cpu.max)" = '800000 100000'
test "$(cat /sys/fs/cgroup/memory.max)" = 12884901888
test "$(cat /sys/fs/cgroup/memory.swap.max)" = 0
df -k / /assets
getconf GNU_LIBC_VERSION
for tool in python python3 node nvidia-smi; do
    if command -v "$tool"; then exit 1; fi
done

cd /tmp
printf '%s  %s\n' \
    e15ff8b5d85afe6c144d97c6fd432254bf76a219daaf17658087d6ecb3e8f0bb \
    /assets/onnxruntime-linux-aarch64-1.28.0.tgz | sha256sum -c -
tar -xzf /assets/onnxruntime-linux-aarch64-1.28.0.tgz
ort_dir=/tmp/onnxruntime-linux-aarch64-1.28.0

# 仅诊断容器安装 ELF/编译工具；不构建服务镜像或参考/导出环境。
apt-get update -qq
apt-get install -y -o Dpkg::Use-Pty=0 --no-install-recommends gcc libc6-dev binutils file
dpkg-query -W libc6 libstdc++6 gcc binutils file
ls -l "$ort_dir/lib"
for library in "$ort_dir"/lib/libonnxruntime.so.1.28.0 "$ort_dir"/lib/libonnxruntime_providers_shared.so; do
    file "$library"
    readelf -d "$library"
    readelf --version-info "$library" | sed -n '/Version needs section/,$p'
    ldd "$library"
done

cat > /tmp/ort-probe.c <<'C'
#include <stdio.h>
#include <string.h>
#include "onnxruntime_c_api.h"

static int check(const OrtApi *api, OrtStatus *status) {
    if (!status) return 0;
    fprintf(stderr, "%s\n", api->GetErrorMessage(status));
    api->ReleaseStatus(status);
    return 1;
}

int main(void) {
    const OrtApiBase *base = OrtGetApiBase();
    const OrtApi *api = base->GetApi(ORT_API_VERSION);
    printf("ORT version: %s; requested C API: %d\n", base->GetVersionString(), ORT_API_VERSION);
    if (!api || strcmp(base->GetVersionString(), "1.28.0")) return 1;
    char **providers = NULL;
    int count = 0;
    if (check(api, api->GetAvailableProviders(&providers, &count))) return 1;
    printf("Provider count: %d\n", count);
    for (int i = 0; i < count; ++i) puts(providers[i]);
    int failed = count != 1 || strcmp(providers[0], "CPUExecutionProvider");
    if (check(api, api->ReleaseAvailableProviders(providers, count))) return 1;
    OrtEnv *env = NULL;
    if (check(api, api->CreateEnv(ORT_LOGGING_LEVEL_WARNING, "laya-env-check", &env))) return 1;
    api->ReleaseEnv(env);
    if (failed) return 1;
    puts("PASS: native ORT load, API 28, CPU-only provider, environment creation");
    return 0;
}
C
gcc -Wall -Wextra -Werror -I "$ort_dir/include" /tmp/ort-probe.c \
    -L "$ort_dir/lib" -Wl,-rpath,"$ort_dir/lib" -lonnxruntime -o /tmp/ort-probe
ldd /tmp/ort-probe
/tmp/ort-probe
for tool in python python3 node nvidia-smi; do
    if command -v "$tool"; then exit 1; fi
done
