#!/usr/bin/env bash
# Keeps this machine on the internet while the network applet is being tested by hand.
#
# Live-testing the network applet means deliberately disconnecting, switching and forgetting
# networks. If one of those does not come back, the machine loses the internet and whoever is
# driving the test loses their session with it. This is the plan B: a watchdog that runs
# independently of the shell that started it, notices a lost connection, gives the tester a grace
# period to recover it themselves, and then restores it.
#
# It guards CONNECTIVITY, not one profile. Switching between saved networks is a thing the tester
# does on purpose, so any wireless connection that reaches the internet counts as healthy; `restore`
# is the explicit way to get the recorded profile specifically. Two things this has already caught:
# `nmcli connection down` on a profile with autoconnect on is undone by NetworkManager within
# seconds, so a real disconnect test turns the radio off instead; and after a radio cycle NM
# autoconnects to the STRONGEST saved profile, which is not necessarily the one that was up.
#
#   scripts/net-guard.sh baseline           record what is up now
#   scripts/net-guard.sh restore            put the recorded profile back, once
#   scripts/net-guard.sh watch [seconds]    self-heal until the deadline, in the foreground
#   scripts/net-guard.sh arm [seconds]      the same, detached, surviving this shell
#   scripts/net-guard.sh status             what is recorded, and whether a guard is armed
#   scripts/net-guard.sh disarm             stop the detached guard
set -uo pipefail

STATE_DIR="${XDG_RUNTIME_DIR:-/tmp}/glimpse"
BASELINE="$STATE_DIR/net-baseline"
UNIT="glimpse-net-guard"

# How long a connection may stay down before the guard intervenes. Long enough to watch a
# deliberate disconnect and reconnect by hand; short enough that a mistake is not a lost session.
GRACE_CHECKS="${GLIMPSE_NET_GRACE:-3}"
INTERVAL="${GLIMPSE_NET_INTERVAL:-10}"
DEFAULT_DEADLINE=3600

die() { printf 'net-guard: %s\n' "$1" >&2; exit 1; }
note() { printf 'net-guard: %s\n' "$1"; }

require_nmcli() { command -v nmcli >/dev/null || die "nmcli is not installed"; }

wifi_uuid_up() {
    nmcli -t -f UUID,TYPE connection show --active 2>/dev/null \
        | awk -F: '$2 == "802-11-wireless" { print $1; exit }'
}

online() {
    local state
    state=$(nmcli -t -f CONNECTIVITY general status 2>/dev/null)
    [ "$state" = "full" ] || [ "$state" = "limited" ] || [ "$state" = "portal" ]
}

cmd_baseline() {
    require_nmcli
    mkdir -p "$STATE_DIR"
    local uuid name
    uuid=$(wifi_uuid_up)
    [ -n "$uuid" ] || die "no wireless connection is up; connect first, then record a baseline"
    name=$(nmcli -t -f connection.id connection show "$uuid" 2>/dev/null | cut -d: -f2-)
    printf '%s\n%s\n' "$uuid" "$name" > "$BASELINE"
    chmod 600 "$BASELINE"
    note "recorded $name ($uuid)"
}

recorded_uuid() { [ -r "$BASELINE" ] && head -1 "$BASELINE"; }
recorded_name() { [ -r "$BASELINE" ] && sed -n 2p "$BASELINE"; }

cmd_restore() {
    require_nmcli
    local uuid; uuid=$(recorded_uuid)
    [ -n "$uuid" ] || die "no baseline recorded; run: scripts/net-guard.sh baseline"
    if ! nmcli -t -f UUID connection show 2>/dev/null | grep -qx "$uuid"; then
        die "the recorded profile $uuid no longer exists; re-create it before relying on the guard"
    fi
    nmcli radio wifi on >/dev/null 2>&1
    nmcli networking on >/dev/null 2>&1
    if nmcli connection up uuid "$uuid" >/dev/null 2>&1; then
        note "restored $(recorded_name)"
        return 0
    fi
    note "could not restore $(recorded_name)"
    return 1
}

cmd_watch() {
    require_nmcli
    local deadline="${1:-$DEFAULT_DEADLINE}"
    local uuid; uuid=$(recorded_uuid)
    [ -n "$uuid" ] || die "no baseline recorded"
    local ends=$(( SECONDS + deadline ))
    local down=0
    note "watching $(recorded_name) for ${deadline}s, grace ${GRACE_CHECKS}x${INTERVAL}s"
    while [ "$SECONDS" -lt "$ends" ]; do
        if online && [ -n "$(wifi_uuid_up)" ]; then
            down=0
        else
            down=$(( down + 1 ))
            note "connection down ($down/$GRACE_CHECKS)"
            if [ "$down" -ge "$GRACE_CHECKS" ]; then
                cmd_restore && down=0
            fi
        fi
        sleep "$INTERVAL"
    done
    note "deadline reached; standing down"
}

cmd_arm() {
    local deadline="${1:-$DEFAULT_DEADLINE}"
    cmd_disarm >/dev/null 2>&1
    systemd-run --user --quiet --collect --unit="$UNIT" \
        --property=Restart=on-failure --property=RestartSec=5 \
        -- "$(readlink -f "$0")" watch "$deadline" \
        || die "could not arm the guard"
    note "armed as $UNIT for ${deadline}s — survives this shell"
}

cmd_disarm() {
    systemctl --user stop "$UNIT" 2>/dev/null
    systemctl --user reset-failed "$UNIT" 2>/dev/null
    note "disarmed"
}

cmd_status() {
    if [ -r "$BASELINE" ]; then
        note "baseline: $(recorded_name) ($(recorded_uuid))"
    else
        note "baseline: none recorded"
    fi
    local active; active=$(systemctl --user is-active "$UNIT" 2>/dev/null)
    note "guard: ${active:-inactive}"
    note "connectivity: $(nmcli -t -f CONNECTIVITY general status 2>/dev/null)"
    note "wireless up: $(wifi_uuid_up || echo none)"
}

case "${1:-status}" in
    baseline) cmd_baseline ;;
    restore)  cmd_restore ;;
    watch)    cmd_watch "${2:-}" ;;
    arm)      cmd_arm "${2:-}" ;;
    disarm)   cmd_disarm ;;
    status)   cmd_status ;;
    *)        die "unknown command: $1" ;;
esac
