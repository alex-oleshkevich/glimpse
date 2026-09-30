#!/usr/bin/env bash
set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/glimpse-paths.sh"

for b in "${binaries[@]}"; do
    rm -f "$bindir/$b"
done

rm -f "$unitdir"/glimpse*.service
rm -f "$unitdir"/glimpse*.target
rm -f "$dbusdir/org.freedesktop.Notifications.service" "$dbusdir/me.aresa.Glimpse.Notifications.service" "$dbusdir/me.aresa.Glimpse.Weather.service" "$dbusdir/me.aresa.Glimpse.Idle.service" "$dbusdir/me.aresa.Glimpse.Idle.Portal.service" "$dbusdir/me.aresa.Glimpse.Shot.service" "$dbusdir/me.aresa.Glimpse.Shot.Portal.service" "$dbusdir/org.freedesktop.ScreenSaver.service"
rm -f "$applicationsdir/me.aresa.Glimpse.Shot.desktop"
rm -f "$destdir$prefix/share/metainfo/me.aresa.Glimpse.Shot.metainfo.xml" "$destdir$prefix/share/icons/hicolor/scalable/apps/me.aresa.Glimpse.Shot.svg" "$destdir$prefix/share/icons/hicolor/symbolic/apps/me.aresa.Glimpse.Shot-symbolic.svg"
rm -f "$portaldir/glimpse.portal" "$portaldir/glimpse-shot.portal"
rm -f "$portalconfdir/glimpse-portals.conf"
for f in data/portals/*-portals.conf; do
    target="$portaletcdir/$(basename "$f")"
    if cmp -s "$target" "$sharedir/portals/$(basename "$f")"; then
        rm -f "$target"
    elif [[ -e "$target" ]]; then
        printf 'keeping %s, which was edited after glimpse installed it\n' "$target" >&2
    fi
done
[[ -d "$portaletcdir" ]] && rmdir --ignore-fail-on-non-empty "$portaletcdir"
rm -f "$bindir/glimpse-notificationd"
rm -f "$bindir/glimpse-dpms"
rm -f "$pamdir/glimpse-lock"
rm -f "$geocluedir/glimpse.conf"
rm -rf "$sharedir"

# Only our own catalog: every language directory here is shared with other packages. The
# `continue` rather than `&&` keeps the script's exit status 0 when nothing is installed,
# since this loop is the last thing it runs.
for f in "$localedir"/*/LC_MESSAGES/glimpse.mo; do
    [[ -e "$f" ]] || continue
    rm -f "$f"
done
