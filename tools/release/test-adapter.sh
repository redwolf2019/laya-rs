#!/bin/sh
# Disposable Alpine container only; bypasses host preflight to test a real supervisor.
set -eu
[ -f /.dockerenv ] || { echo 'Run only in a disposable Docker test container' >&2; exit 1; }
adapter=${1:?system adapter: openrc, runit or dinit}
archive=${2:?runtime archive}
model_path=${3:?real model directory}
case "$adapter" in openrc|runit|dinit) ;; *) exit 1;; esac
apk add --no-cache "$adapter" curl >/dev/null
mkdir -p /opt/laya-server/current /etc/laya-server /var/lib/laya-server
tar -xzf "$archive" -C /opt/laya-server/current
LAYA_INSTALLER_TEST_SOURCE=1
# shellcheck source=scripts/install.sh
. ./scripts/install.sh
create_user
# A leading tilde is valid b64token; it must not undergo shell expansion.
printf 'LAYA_API_TOKEN=~\n' > "$CONF/token.env"
printf 'PORT=8080\nTHREADS=2\nSLOTS=1\n' > "$CONF/server.conf"
chmod 700 "$CONF"
chmod 600 "$CONF/token.env"
cat > "$ROOT/current/launch" <<EOF
#!/bin/sh
exec '$ROOT/current/exec' laya-server --model '$model_path' --ort-library '$ROOT/current/lib/libonnxruntime.so' --threads 2 --max-concurrency 1
EOF
chmod 755 "$ROOT/current/launch"
case "$adapter" in
    openrc) mkdir -p /run/openrc; touch /run/openrc/softlevel;;
    runit) mkdir -p /etc/sv /var/service; runsvdir /var/service >/tmp/supervisor.log 2>&1 &;;
    dinit)
        mkdir -p /etc/dinit.d/boot.d
        printf 'type = internal\nwaits-for.d = boot.d\n' > /etc/dinit.d/boot
        dinit --system --container -d /etc/dinit.d boot >/tmp/supervisor.log 2>&1 &
        sleep 2;;
esac
manager=$adapter
work=$(mktemp -d)
write_service
service_start
check_api
service_stop
printf 'LAYA_API_TOKEN=rotated-adapter-test-secret\n' > "$CONF/token.env"
service_start
check_api
service_stop
service_remove
printf 'PASS: %s real start/API/stop/token reload/removal (not host installation or boot)\n' "$adapter"
