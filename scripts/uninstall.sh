#!/usr/bin/env bash
set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/glimpse-paths.sh"

for b in "${binaries[@]}"; do
    rm -f "$bindir/$b"
done

rm -f "$unitdir"/glimpse*.service
rm -f "$unitdir"/glimpse*.target
rm -f "$dbusdir/org.kde.StatusNotifierWatcher.service" "$dbusdir/org.freedesktop.Notifications.service" "$dbusdir/me.aresa.Glimpse.Notifications.service" "$dbusdir/me.aresa.Glimpse.Weather.service"
rm -f "$bindir/glimpse-notificationd"
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
