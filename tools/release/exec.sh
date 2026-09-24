#!/bin/sh
set -eu
base=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
program=${1:?expected laya-server or jq}
shift
case "$program" in laya-server|jq) ;; *) exit 2;; esac
unset LD_PRELOAD LD_LIBRARY_PATH
exec "$base/ld.so" --library-path "$base/lib" "$base/bin/$program" "$@"
