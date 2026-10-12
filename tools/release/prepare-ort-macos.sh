#!/bin/bash
# Release dependency preparation only; never invoked by the application/installer.
set -euo pipefail
source_dir=${1:?usage: prepare-ort-macos.sh SOURCE OUTPUT}
output=${2:?usage: prepare-ort-macos.sh SOURCE OUTPUT}
revision=da9b5e364c465de65c49d91e696cd6485270757f
[[ $(uname -s) = Darwin && $(uname -m) = arm64 ]]
export MACOSX_DEPLOYMENT_TARGET=14.0
unset CPATH C_INCLUDE_PATH CPLUS_INCLUDE_PATH LIBRARY_PATH CMAKE_PREFIX_PATH CFLAGS CXXFLAGS LDFLAGS
if [[ ! -d $source_dir ]]; then git clone --branch v1.28.0 --depth 1 https://github.com/microsoft/onnxruntime.git "$source_dir"; fi
[[ $(git -C "$source_dir" rev-parse HEAD) = "$revision" ]]
[[ -z $(git -C "$source_dir" status --porcelain) ]]
git -C "$source_dir" submodule update --init --recursive
mkdir -p "$output"
output=$(cd "$output" && pwd -P)
source_dir=$(cd "$source_dir" && pwd -P)
python3 "$source_dir/tools/ci_build/build.py" --build_dir "$output/build" \
    --config Release --update --build --parallel 6 --build_shared_lib --skip_tests \
    --skip_submodule_sync --osx_arch arm64 --apple_deploy_target 14.0 \
    --compile_no_warning_as_error --cmake_extra_defines \
    FETCHCONTENT_TRY_FIND_PACKAGE_MODE=NEVER CMAKE_FIND_USE_PACKAGE_REGISTRY=OFF \
    CMAKE_OSX_DEPLOYMENT_TARGET=14.0 onnxruntime_BUILD_UNIT_TESTS=OFF \
    onnxruntime_USE_COREML=OFF onnxruntime_USE_WEBGPU=OFF onnxruntime_USE_XNNPACK=OFF
cp "$output/build/Release/libonnxruntime.1.28.0.dylib" "$output/libonnxruntime.dylib"
cp "$source_dir/LICENSE" "$source_dir/ThirdPartyNotices.txt" "$output/"
{
    printf 'ORT_SOURCE=%s\nMACOSX_DEPLOYMENT_TARGET=14.0\n' "$revision"
    git -C "$source_dir" submodule status --recursive
    shasum -a 256 "$source_dir/cmake/deps.txt" "$output/libonnxruntime.dylib"
    sw_vers
    xcodebuild -version
    xcrun --show-sdk-version
    cmake --version
    cat "$output/build/Release/CMakeCache.txt"
} > "$output/build-record.txt"
codesign --force --sign - "$output/libonnxruntime.dylib"
codesign --verify --strict "$output/libonnxruntime.dylib"
shasum -a 256 "$output/libonnxruntime.dylib" > "$output/SHA256SUMS"
cc -mmacosx-version-min=14.0 -I "$source_dir/include/onnxruntime/core/session" \
    "$(dirname "$0")/providers-macos.c" -o "$output/providers-probe"
"$output/providers-probe" "$output/libonnxruntime.dylib" | tee "$output/providers.txt"
