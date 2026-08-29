#!/usr/bin/env bash
set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/glimpse-paths.sh"

for b in "${binaries[@]}"; do
    rm -f "$bindir/$b"
done

rm -f "$unitdir"/glimpse*.service
rm -f "$dbusdir/org.kde.StatusNotifierWatcher.service" "$dbusdir/org.freedesktop.Notifications.service"
rm -f "$pamdir/glimpse-lock"
rm -f "$geocluedir/glimpse.conf"
rm -rf "$sharedir"
