#!/bin/sh
# Run only in the disposable Linux builder documented in docs/installation.md.
set -eu
version=${1:?usage: build.sh VERSION OUTPUT_DIRECTORY}
out=${2:?}
case "$version" in v[0-9]*) ;; *) echo 'Expected a version beginning with v' >&2; exit 1;; esac
case "$(uname -m)" in x86_64) arch=x86_64; ort_arch=x64; ort_sha=a3e1b79d7bb1bf09696ce675f49e4064e6c81f6202b8225624fff0e93f8d6407;;
    aarch64) arch=aarch64; ort_arch=aarch64; ort_sha=e15ff8b5d85afe6c144d97c6fd432254bf76a219daaf17658087d6ecb3e8f0bb;;
    *) exit 1;; esac
mkdir -p "$out"
out=$(cd "$out" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM
CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$work/target}
export CARGO_TARGET_DIR
pkg=$work/package
mkdir -p "$pkg/bin" "$pkg/lib" "$pkg/licenses/debian" "$work/sources"
sed -i 's/^Types: deb$/Types: deb deb-src/' /etc/apt/sources.list.d/debian.sources
apt-get update
apt-get install -y --no-install-recommends g++ libc6-dev pkg-config ca-certificates curl jq binutils dpkg-dev
cargo build --release --locked --bin laya-server
cp "${CARGO_TARGET_DIR:-target}/release/laya-server" "$pkg/bin/"
strip "$pkg/bin/laya-server"
cp /usr/bin/jq "$pkg/bin/"
curl -fL --proto '=https' --tlsv1.2 --connect-timeout 20 --max-time 600 --retry 3 \
    "https://github.com/microsoft/onnxruntime/releases/download/v1.28.0/onnxruntime-linux-$ort_arch-1.28.0.tgz" -o "$work/ort.tgz"
printf '%s  %s\n' "$ort_sha" "$work/ort.tgz" | sha256sum -c -
tar -xzf "$work/ort.tgz" -C "$work"
cp -L "$work/onnxruntime-linux-$ort_arch-1.28.0/lib/"*.so* "$pkg/lib/"
cp "$work/onnxruntime-linux-$ort_arch-1.28.0/"LICENSE "$pkg/licenses/ONNX-Runtime-LICENSE"
cp "$work/onnxruntime-linux-$ort_arch-1.28.0/ThirdPartyNotices.txt" "$pkg/licenses/ONNX-Runtime-ThirdPartyNotices.txt"
# Include the loader and full dependency closure, so the host libc is never replaced.
for elf in "$pkg/bin/"* "$pkg/lib/"*; do ldd "$elf"; done > "$work/ldd"
if grep -q 'not found' "$work/ldd"; then cat "$work/ldd"; exit 1; fi
awk '/=> \// {print $3} /^[[:space:]]*\// {print $1}' "$work/ldd" | sort -u > "$work/libraries"
while IFS= read -r lib; do cp -L "$lib" "$pkg/lib/"; done < "$work/libraries"
cp -L "$pkg/lib/"ld-linux-*.so.* "$pkg/ld.so"
cp tools/release/exec.sh "$pkg/exec"
chmod 755 "$pkg/exec" "$pkg/ld.so"
cp LICENSE NOTICE.md Cargo.lock "$pkg/licenses/"
cp -R licenses "$pkg/licenses/project"
mkdir "$pkg/licenses/crates"
find "${CARGO_HOME:-/usr/local/cargo}/registry/src" -type f \
    \( -iname '*license*' -o -iname '*copying*' -o -iname '*copyright*' -o -iname '*notice*' -o -name Cargo.toml \) \
    -exec cp --parents -t "$pkg/licenses/crates" {} +
cp -R /usr/local/rustup/toolchains/*/share/doc/rust/licenses "$pkg/licenses/rust"
cp /usr/local/rustup/toolchains/*/share/doc/rust/COPYRIGHT*.html "$pkg/licenses/rust/"
# Distribute exact Debian corresponding sources alongside the shared libraries.
for name in libc6 libgcc-s1 libstdc++6 libjq1 libonig5; do
    cp "/usr/share/doc/$name/copyright" "$pkg/licenses/debian/$name"
    dpkg-query -W -f='${Package}\t${Version}\t${source:Package}\t${source:Version}\n' "$name"
done > "$pkg/licenses/debian/packages.tsv"
awk '{print $3 "=" $4}' "$pkg/licenses/debian/packages.tsv" | sort -u > "$work/source-packages"
while IFS= read -r source; do (cd "$work/sources" && apt-get source --download-only "$source"); done < "$work/source-packages"
cp tools/release/build.sh "$work/sources/build.sh"
cp "$pkg/licenses/debian/packages.tsv" "$work/sources/"
tar -czf "$out/runtime-sources-$arch.tar.gz" -C "$work/sources" .
jq -r '.files[] | "\(.sha256)  \(.path)"' docs/model-manifest.json > "$pkg/model-files.sha256"
sha256sum docs/model-manifest.json | cut -c1-16 > "$pkg/MODEL_ID"
printf '%s\n' "$version" > "$pkg/VERSION"
cp tools/release/smoke.json tools/release/response.jq "$pkg/"
"$pkg/exec" laya-server --help > /dev/null
"$pkg/exec" jq --version
tar -czf "$out/laya-server-linux-$arch.tar.gz" -C "$pkg" .
printf 'Built %s (%s)\n' "$version" "$arch"
