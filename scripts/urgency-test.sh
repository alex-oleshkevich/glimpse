#!/usr/bin/env bash
set -uo pipefail

usage() {
    cat <<'USAGE'
Mark a window urgent, so a pager can be watched painting its workspace in the warning color.

Usage: urgency-test.sh [ID] [--clear] [--list]

  ID        the window to mark; without one, the first window on the current workspace
  --clear   unset it again
  --list    print every window with its id, workspace and whether it is urgent

Marking the window you are looking at is the least useful thing to do, because focusing a window
is what clears urgency. Switch to another workspace first, or pass an ID from --list.

Needs niri: it exposes set-window-urgent / unset-window-urgent as actions. Hyprland has no
equivalent dispatcher, and only ever raises urgency from a real xdg-activation request it
declines, so there it has to come from an application asking for attention.
USAGE
}

id=""
action="set-window-urgent"
while [ $# -gt 0 ]; do
    case "$1" in
        --clear) action="unset-window-urgent" ;;
        --list) list=1 ;;
        --id) shift; id="${1:-}" ;;
        --help|-h) usage; exit 0 ;;
        -*) echo "urgency-test.sh: unknown option $1" >&2; exit 2 ;;
        *) id="$1" ;;
    esac
    shift
done

if [ -z "${NIRI_SOCKET:-}" ]; then
    echo "urgency-test.sh: NIRI_SOCKET is unset; this needs niri" >&2
    exit 1
fi

if [ -n "${list:-}" ]; then
    niri msg -j windows | python3 -c "
import json, sys
for w in sorted(json.load(sys.stdin), key=lambda w: (w.get('workspace_id') or 0, w['id'])):
    mark = ' URGENT' if w.get('is_urgent') else ''
    print(f\"{w['id']:>5}  ws={w.get('workspace_id')}  {w.get('app_id')}{mark}  {(w.get('title') or '')[:50]}\")
"
    exit 0
fi

if [ -z "$id" ]; then
    id=$(niri msg -j windows | python3 -c "
import json, sys
windows = json.load(sys.stdin)
here = next((w for w in windows if w.get('is_focused')), None)
workspace = here['workspace_id'] if here else None
on_it = [w for w in windows if w.get('workspace_id') == workspace]
unfocused = [w for w in on_it if not w.get('is_focused')]
pick = (unfocused or on_it)
print(pick[0]['id'] if pick else '')
")
fi

if [ -z "$id" ]; then
    echo "urgency-test.sh: no window on the current workspace; --list shows them all" >&2
    exit 1
fi

niri msg action "$action" --id "$id"
niri msg -j windows | python3 -c "
import json, sys
for w in json.load(sys.stdin):
    if str(w['id']) == '$id':
        print(f\"{'$action'} id={w['id']} ws={w.get('workspace_id')} {w.get('app_id')} urgent={w.get('is_urgent')}\")
"
