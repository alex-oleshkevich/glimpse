#!/usr/bin/env bash
set -euo pipefail

portal=/usr/lib/xdg-desktop-portal
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

inside() {
    local tmp="$1" desktop="$2"

    mkdir -p "$tmp"/{etc-upper,etc-work,share-upper,share-work,services,config,data}
    mount -t overlay overlay -o "lowerdir=/etc,upperdir=$tmp/etc-upper,workdir=$tmp/etc-work" /etc
    mkdir -p /etc/xdg-desktop-portal
    cp "$repo"/data/portals/*-portals.conf /etc/xdg-desktop-portal/

    mount -t overlay overlay \
        -o "lowerdir=/usr/share/xdg-desktop-portal,upperdir=$tmp/share-upper,workdir=$tmp/share-work" \
        /usr/share/xdg-desktop-portal
    rm -f /usr/share/xdg-desktop-portal/glimpse-portals.conf
    mkdir -p "$tmp/portals-copy"
    cp -R /usr/share/xdg-desktop-portal/portals/. "$tmp/portals-copy/"
    mount --bind "$tmp/portals-copy" /usr/share/xdg-desktop-portal/portals
    cp "$repo"/data/portals/*.portal /usr/share/xdg-desktop-portal/portals/

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

    XDG_CURRENT_DESKTOP="$desktop" \
        XDG_CONFIG_HOME="$tmp/config" \
        XDG_CONFIG_DIRS=/etc/xdg \
        XDG_DATA_HOME="$tmp/data" \
        dbus-run-session --config-file "$tmp/bus.conf" -- \
        timeout 3 "$portal" -v > "$tmp/portal.log" 2>&1 || true
}

if [[ "${1:-}" == --inside ]]; then
    inside "$2" "$3"
    exit 0
fi

desktop="${1:?usage: portal-routing.sh <XDG_CURRENT_DESKTOP>, e.g. niri or glimpse:niri}"
tmp="$(mktemp -d "${XDG_RUNTIME_DIR:?XDG_RUNTIME_DIR is not set}/glimpse-portal-routing.XXXXXX")"
trap 'chmod -R u+rwx "$tmp" || true; rm -rf "$tmp"' EXIT

unshare --user --map-root-user --mount "${BASH_SOURCE[0]}" --inside "$tmp" "$desktop"

log="$tmp/portal.log"
failed=0
for expected in Inhibit=glimpse ScreenCast=gnome FileChooser=gnome Screenshot=glimpse-shot; do
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
