#!/usr/bin/env bash
set -uo pipefail

usage() {
    cat <<'USAGE'
Mark a window urgent, so a pager can be watched painting its workspace in the warning color.

Usage: urgency-test.sh [--id ID] [--clear] [--list]

  --id ID   the window to mark; defaults to the focused one
  --clear   unset it again
  --list    print the windows this compositor is showing, with their ids

Needs niri: it exposes set-window-urgent / unset-window-urgent as actions. Hyprland has no
equivalent dispatcher, and only ever raises urgency from a real xdg-activation request it
declines, so there it has to come from an application asking for attention on its own.
USAGE
}

id=""
action="set-window-urgent"
for arg in "$@"; do
    case "$arg" in
        --clear) action="unset-window-urgent" ;;
        --list) niri msg -j windows | python3 -c "
import json, sys
for w in json.load(sys.stdin):
    print(w['id'], w.get('app_id'), '--', (w.get('title') or '')[:60])
"; exit 0 ;;
        --id) shift ;;
        --help|-h) usage; exit 0 ;;
        *) [ -z "$id" ] && id="$arg" ;;
    esac
done

if [ -z "${NIRI_SOCKET:-}" ]; then
    echo "urgency-test.sh: NIRI_SOCKET is unset; this needs niri" >&2
    exit 1
fi

if [ -z "$id" ]; then
    id=$(niri msg -j focused-window | python3 -c "import json,sys; w=json.load(sys.stdin); print(w['id'] if w else '')")
fi
if [ -z "$id" ]; then
    echo "urgency-test.sh: no window to mark; pass --id, or --list to see them" >&2
    exit 1
fi

niri msg action "$action" --id "$id"
echo "$action id=$id"
