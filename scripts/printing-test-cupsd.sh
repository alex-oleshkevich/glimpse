#!/usr/bin/env bash
set -euo pipefail

if [[ ${1:-} == --help ]]; then
    printf '%s\n' 'Run from the repository root: bash scripts/printing-test-cupsd.sh' 'Starts a disposable CUPS container on 8631 and a private host D-Bus, then runs the ignored ServiceRuntime test.'
    exit 0
fi

timeout 5s python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 8631)); s.close()'
scratch=$(mktemp -d "${XDG_RUNTIME_DIR:-/tmp}/gcups.XXXXXX")
container="glimpse-cups-test-$$"
bus_pid=
cleanup() {
    timeout 15s docker rm -f "$container" >/dev/null 2>&1 || true
    if [[ -n $bus_pid ]]; then
        kill "$bus_pid" 2>/dev/null || true
        wait "$bus_pid" 2>/dev/null || true
    fi
    if [[ ${GLIMPSE_TEST_CUPS_KEEP:-0} == 1 ]]; then
        printf 'Harness logs retained at %s\n' "$scratch"
    else
        rm -rf -- "$scratch"
    fi
}
trap cleanup EXIT
trap 'exit 130' INT TERM
mkdir "$scratch/bus" "$scratch/log"
chmod 777 "$scratch/log"
printf '<busconfig><type>system</type><listen>unix:path=%s/bus/system_bus_socket</listen><policy context="default"><allow user="*"/><allow own="*"/><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/></policy></busconfig>\n' "$scratch" > "$scratch/bus.conf"
dbus-daemon --config-file="$scratch/bus.conf" --nofork > "$scratch/bus.log" 2>&1 &
bus_pid=$!
for ((attempt=0; attempt<100; attempt++)); do
    [[ -S $scratch/bus/system_bus_socket ]] && break
    sleep 0.05
done
[[ -S $scratch/bus/system_bus_socket ]]
timeout 30s docker run --detach --rm --name "$container" --network host \
    --volume "$scratch/bus:/run/dbus" --volume "$scratch/log:/test-log" \
    archlinux@sha256:8745817f349ed24373341ddb92776209eeec3f0364ea48f7f645ac5800d30a50 \
    bash -ec '
        pacman -Sy --noconfirm cups dbus
        printf "%s\n" "FileDevice Yes" "AccessLog /test-log/access_log" "ErrorLog /test-log/error_log" "PageLog /test-log/page_log" "LogFilePerm 0644" > /etc/cups/cups-files.conf
        printf "%s\n" "Listen 127.0.0.1:8631" "Browsing Off" "DefaultAuthType None" "LogLevel debug" "AccessLogLevel all" "PreserveJobHistory No" "<Location />" "Order allow,deny" "Allow all" "</Location>" "<Policy default>" "<Limit All>" "Order allow,deny" "Allow all" "</Limit>" "</Policy>" > /etc/cups/cupsd.conf
        exec cupsd -f
    ' >/dev/null
ready=0
for ((attempt=0; attempt<120; attempt++)); do
    if timeout 3s docker exec "$container" lpstat -h 127.0.0.1:8631 -r 2>/dev/null | grep -q 'scheduler is running'; then
        ready=1
        break
    fi
    sleep 0.5
done
if [[ $ready != 1 ]]; then
    timeout 5s docker logs "$container"
    printf '%s\n' 'Isolated CUPS did not start.' >&2
    exit 1
fi
timeout 5s docker exec "$container" lpadmin -h 127.0.0.1:8631 -p test-1 -v file:/dev/null -E
export GLIMPSE_TEST_CUPS_BUS="unix:path=$scratch/bus/system_bus_socket"
export GLIMPSE_TEST_CUPS_CONTAINER="$container"
export GLIMPSE_TEST_CUPS_ACCESS_LOG="$scratch/log/access_log"
export CUPS_SERVER=127.0.0.1:8631
timeout 300s just test-one glimpse-services services::printing::tests::runtime::real_cups_subscription_tracks_held_released_and_cancelled_jobs
