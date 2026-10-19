#!/usr/bin/env bash
set -euo pipefail

portal=/usr/lib/xdg-desktop-portal
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

desktop="${1:?usage: portal-routing.sh <XDG_CURRENT_DESKTOP>, e.g. niri or glimpse:niri}"
tmp="$(mktemp -d "${XDG_RUNTIME_DIR:?XDG_RUNTIME_DIR is not set}/glimpse-portal-routing.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp"/{config,empty,data,services,portals}
cp "$repo"/data/portals/*-portals.conf "$tmp/portals/"
cp /usr/share/xdg-desktop-portal/portals/*.portal "$tmp/portals/"
cp "$repo"/data/portals/*.portal "$tmp/portals/"

cat > "$tmp/bus.conf" <<EOF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path=$tmp/bus</listen>
  <servicedir>$tmp/services</servicedir>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
EOF

log="$tmp/portal.log"
XDG_CURRENT_DESKTOP="$desktop" \
    XDG_CONFIG_HOME="$tmp/config" \
    XDG_CONFIG_DIRS="$tmp/empty" \
    XDG_DATA_HOME="$tmp/data" \
    XDG_DESKTOP_PORTAL_DIR="$tmp/portals" \
    dbus-run-session --config-file "$tmp/bus.conf" -- \
    timeout 3 "$portal" -v > "$log" 2>&1 || true

failed=0
for expected in Inhibit=glimpse Screenshot=glimpse-shot; do
    interface="org.freedesktop.impl.portal.${expected%%=*}"
    backend="${expected#*=}"
    chosen="$(sed -n "s/^XDP: Using \([^ ]*\)\.portal for $interface (.*/\1/p" "$log" | sort -u)"
    if [[ "$chosen" == "$backend" ]]; then
        printf 'ok   %s -> %s\n' "$interface" "$backend"
    else
        printf 'FAIL %s -> %s, expected %s\n' "$interface" "${chosen:-nothing}" "$backend" >&2
        grep -F -e "$interface" -e "Using portal configuration" "$log" >&2 || true
        [[ -n "$chosen" ]] || tail -n 20 "$log" >&2
        failed=1
    fi
done
exit "$failed"
