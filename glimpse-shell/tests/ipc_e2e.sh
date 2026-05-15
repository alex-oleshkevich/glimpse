#!/usr/bin/env bash
# E2E test: stop the glimpse-shell systemd service, run a freshly built binary
# under the live Wayland session, exercise the IPC action surface, verify via
# the event stream, then restore.
#
# INTRUSIVE: stops/starts glimpse-shell.service (your whole panel), and briefly
# toggles real volume/DND — both captured at start and restored on exit.
# Destructive commands (forget_*, clear_*, eject) are only tested for the
# confirm=true guard; their effects are NEVER triggered.
set -euo pipefail

BINARY="$(cargo build -p glimpse-shell --message-format=json 2>/dev/null \
    | python3 -c "import sys,json; [print(o['executable']) for l in sys.stdin for o in [json.loads(l)] if o.get('reason')=='compiler-artifact' and 'glimpse-shell' in o.get('target',{}).get('name','') and o.get('executable')]" \
    | tail -1)"

[[ -z "$BINARY" ]] && { echo "ERROR: could not locate binary" >&2; exit 1; }
echo "binary: $BINARY"

SOCKET="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/glimpse/ipc.sock"
SERVICE_WAS_ACTIVE=false
DAEMON_PID=""
WATCHER_PID=""
WATCH_OUT=""
DAEMON_LOG=""
ORIG_DND=""
ORIG_VOLUME=""

pass() { echo "  PASS: $*"; }
fail() { echo "  FAIL: $*" >&2; exit 1; }
assert_contains() { echo "$1" | grep -q "$2" || fail "expected '$2' in: $1"; }

expect_event_from() {
    local from_line="$1" contains="$2" timeout="${3:-3}"
    local deadline=$((SECONDS + timeout))
    while [[ $SECONDS -lt $deadline ]]; do
        tail -n +"$from_line" "$WATCH_OUT" 2>/dev/null | grep -q "$contains" && { pass "event: $contains"; return 0; }
        sleep 0.1
    done
    fail "timed out waiting for '$contains':"$'\n'"$(tail -n +"$from_line" "$WATCH_OUT" 2>/dev/null || true)"
}

watch_line_count() { wc -l < "$WATCH_OUT" 2>/dev/null || echo 0; }
status_field() { "$BINARY" dispatch status 2>/dev/null | grep -o "$1=[^ ]*" | head -1 | cut -d= -f2; }

cleanup() {
    echo "--- cleanup ---"
    # Restore real system state we toggled (best-effort; daemon may be ours).
    if [[ -n "$ORIG_DND" && -S "$SOCKET" ]]; then
        "$BINARY" dispatch set_dnd "enabled=$ORIG_DND" >/dev/null 2>&1 || true
    fi
    if [[ -n "$ORIG_VOLUME" && -S "$SOCKET" ]]; then
        "$BINARY" dispatch set_volume "level=$ORIG_VOLUME" >/dev/null 2>&1 || true
    fi
    [[ -n "$WATCHER_PID" ]] && kill "$WATCHER_PID" 2>/dev/null || true
    [[ -n "$DAEMON_PID" ]] && { kill "$DAEMON_PID" 2>/dev/null; wait "$DAEMON_PID" 2>/dev/null || true; }
    [[ -n "$WATCH_OUT" ]] && rm -f "$WATCH_OUT"
    [[ -n "$DAEMON_LOG" ]] && rm -f "$DAEMON_LOG"
    if $SERVICE_WAS_ACTIVE; then
        echo "restoring glimpse-shell.service..."
        systemctl --user start glimpse-shell
        echo "service restored"
    fi
}
trap cleanup EXIT

if systemctl --user is-active --quiet glimpse-shell; then
    SERVICE_WAS_ACTIVE=true
    echo "stopping glimpse-shell.service..."
    systemctl --user stop glimpse-shell
    sleep 1
fi
# 'glimpse-shell' fits the 15-char comm limit, but match the executable path
# anchored so this script's own cmdline (.../glimpse-shell/tests/...) isn't hit.
pkill -f -- '/glimpse-shell( |$)' 2>/dev/null || true
sleep 1
rm -f "$SOCKET"

# ── flags / help (no daemon) ──────────────────────────────────────────────────

echo ""; echo "=== --help / --version ==="
"$BINARY" --help | grep -q "DISPATCH COMMANDS" || fail "--help missing DISPATCH COMMANDS"
"$BINARY" --version | grep -q "glimpse-shell" || fail "--version missing name"
pass "help/version"

echo ""; echo "=== no daemon → immediate error ==="
! "$BINARY" dispatch status 2>/dev/null || fail "dispatch with no daemon should fail"
pass "no daemon → error"

# ── start daemon ──────────────────────────────────────────────────────────────

echo ""; echo "starting daemon..."
DAEMON_LOG=$(mktemp)
"$BINARY" >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!
for i in $(seq 1 100); do [[ -S "$SOCKET" ]] && break; sleep 0.1; done
[[ -S "$SOCKET" ]] || { echo "ERROR: socket never appeared" >&2; sed 's/\x1b\[[0-9;]*m//g' "$DAEMON_LOG" >&2; exit 1; }
sleep 1  # let services populate initial state
echo "daemon ready (pid $DAEMON_PID)"

WATCH_OUT=$(mktemp)
"$BINARY" watch >"$WATCH_OUT" 2>&1 &
WATCHER_PID=$!
sleep 0.3

# ── status ────────────────────────────────────────────────────────────────────

echo ""; echo "=== dispatch status ==="
OUT=$("$BINARY" dispatch status)
echo "  $OUT"
assert_contains "$OUT" "ok=true"
assert_contains "$OUT" "connectivity="
assert_contains "$OUT" "bluetooth_powered="
assert_contains "$OUT" "dnd="
pass "status"

ORIG_DND=$(status_field dnd); [[ -z "$ORIG_DND" ]] && ORIG_DND=false
ORIG_VOLUME=$(status_field audio_volume)

# ── set_dnd → event ───────────────────────────────────────────────────────────

echo ""; echo "=== set_dnd toggles + emits ==="
if [[ "$ORIG_DND" == "true" ]]; then target_dnd=false; ev="notification.dnd_disabled"; else target_dnd=true; ev="notification.dnd_enabled"; fi
BEFORE=$(watch_line_count)
OUT=$("$BINARY" dispatch set_dnd "enabled=$target_dnd")
assert_contains "$OUT" "ok=true"
expect_event_from "$((BEFORE + 1))" "$ev"
"$BINARY" dispatch set_dnd "enabled=$ORIG_DND" >/dev/null
pass "set_dnd"

# ── set_volume → audio.volume_changed ─────────────────────────────────────────

if [[ -n "$ORIG_VOLUME" ]]; then
    echo ""; echo "=== set_volume → audio.volume_changed ==="
    if [[ "$ORIG_VOLUME" == "42" ]]; then tgt=43; else tgt=42; fi
    BEFORE=$(watch_line_count)
    OUT=$("$BINARY" dispatch set_volume "level=$tgt")
    assert_contains "$OUT" "ok=true"
    expect_event_from "$((BEFORE + 1))" "audio.volume_changed"
    "$BINARY" dispatch set_volume "level=$ORIG_VOLUME" >/dev/null
    pass "set_volume"
fi

# ── validation / error paths ──────────────────────────────────────────────────

echo ""; echo "=== validation rejects bad input ==="
assert_contains "$("$BINARY" dispatch frobnicate 2>&1 || true)" "ok=false"
assert_contains "$("$BINARY" dispatch set_volume level=200 2>&1 || true)" "ok=false"
assert_contains "$("$BINARY" dispatch set_volume level=abc 2>&1 || true)" "ok=false"
assert_contains "$("$BINARY" dispatch set_volume 50 2>&1 || true)" "ok=false"
assert_contains "$("$BINARY" dispatch set_dnd enabled=maybe 2>&1 || true)" "ok=false"
assert_contains "$("$BINARY" dispatch set_dnd 2>&1 || true)" "ok=false"
pass "validation"

echo ""; echo "=== destructive commands rejected without confirm=true ==="
for c in "forget_wifi uuid=x" "forget_bluetooth address=x" "eject id=x" \
         "poweroff_drive id=x" "clear_clipboard" "clear_clipboard_history"; do
    OUT=$("$BINARY" dispatch $c 2>&1 || true)
    assert_contains "$OUT" "ok=false"
    echo "$OUT" | grep -q "confirm=true" || fail "expected confirm hint for '$c': $OUT"
done
pass "destructive commands require confirm=true (effects NOT triggered)"

# ── watch --json ──────────────────────────────────────────────────────────────

echo ""; echo "=== watch --json ==="
JSON_OUT=$(mktemp)
"$BINARY" watch --json >"$JSON_OUT" 2>&1 &
JSON_PID=$!
sleep 0.5
"$BINARY" dispatch set_dnd "enabled=$([[ "$ORIG_DND" == "true" ]] && echo false || echo true)" >/dev/null
"$BINARY" dispatch set_dnd "enabled=$ORIG_DND" >/dev/null
sleep 0.8
kill "$JSON_PID" 2>/dev/null || true; wait "$JSON_PID" 2>/dev/null || true
if [[ -s "$JSON_OUT" ]]; then
    while IFS= read -r line; do
        echo "$line" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d.get('type')=='event'; assert 'name' in d; assert isinstance(d.get('ts'),int)" \
            || fail "invalid JSON event: $line"
    done < "$JSON_OUT"
    pass "watch --json valid"
else
    fail "watch --json received no events"
fi
rm -f "$JSON_OUT"

kill "$WATCHER_PID" 2>/dev/null || true; wait "$WATCHER_PID" 2>/dev/null || true; WATCHER_PID=""

echo ""; echo "=== watch output ==="
cat "$WATCH_OUT"
[[ -s "$WATCH_OUT" ]] || fail "watch received no events at all"

echo ""; echo "=== ALL TESTS PASSED ==="
