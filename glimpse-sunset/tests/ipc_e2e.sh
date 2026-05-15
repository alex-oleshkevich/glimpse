#!/usr/bin/env bash
# E2E test: stop glimpse-sunset systemd service, run a freshly built binary,
# exercise every watch + dispatch combination, verify events, then restore.
set -euo pipefail

BINARY="$(cargo build -p glimpse-sunset --message-format=json 2>/dev/null \
    | python3 -c "import sys,json; [print(o['executable']) for l in sys.stdin for o in [json.loads(l)] if o.get('reason')=='compiler-artifact' and 'glimpse-sunset' in o.get('target',{}).get('name','') and o.get('executable')]" \
    | tail -1)"

[[ -z "$BINARY" ]] && { echo "ERROR: could not locate binary" >&2; exit 1; }
echo "binary: $BINARY"

SOCKET="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/glimpse/sunset.sock"
SERVICE_WAS_ACTIVE=false
DAEMON_PID=""
WATCHER_PID=""
WATCH_OUT=""

# ── helpers ────────────────────────────────────────────────────────────────────

pass() { echo "  PASS: $*"; }
fail() { echo "  FAIL: $*" >&2; exit 1; }

assert_contains() {
    echo "$1" | grep -q "$2" || fail "expected '$2' in: $1"
}

# After a mutating dispatch command, wait up to $3 seconds for an event line
# containing $2 to appear in WATCH_OUT starting from line $1.
expect_event_from() {
    local from_line="$1"
    local contains="$2"
    local timeout="${3:-2}"
    local deadline=$((SECONDS + timeout))
    while [[ $SECONDS -lt $deadline ]]; do
        if tail -n +"$from_line" "$WATCH_OUT" 2>/dev/null | grep -q "$contains"; then
            pass "event received: $contains"
            return 0
        fi
        sleep 0.1
    done
    fail "timed out waiting for event '$contains' (watch output since line $from_line):"$'\n'"$(tail -n +"$from_line" "$WATCH_OUT" 2>/dev/null || true)"
}

watch_line_count() { wc -l < "$WATCH_OUT" 2>/dev/null || echo 0; }

# ── cleanup ────────────────────────────────────────────────────────────────────

cleanup() {
    echo "--- cleanup ---"
    [[ -n "$WATCHER_PID" ]] && kill "$WATCHER_PID" 2>/dev/null || true
    [[ -n "$DAEMON_PID" ]] && { kill "$DAEMON_PID" 2>/dev/null; wait "$DAEMON_PID" 2>/dev/null || true; }
    [[ -n "$WATCH_OUT" ]] && rm -f "$WATCH_OUT"
    if $SERVICE_WAS_ACTIVE; then
        echo "restoring glimpse-sunset.service..."
        systemctl --user start glimpse-sunset
        echo "service restored"
    fi
}
trap cleanup EXIT

# ── stop everything ───────────────────────────────────────────────────────────

if systemctl --user is-active --quiet glimpse-sunset; then
    SERVICE_WAS_ACTIVE=true
    echo "stopping glimpse-sunset.service..."
    systemctl --user stop glimpse-sunset
    sleep 1
fi
pkill -x glimpse-sunset 2>/dev/null || true
for i in $(seq 1 40); do
    prod=$(dbus-send --session --print-reply --dest=org.freedesktop.DBus \
        /org/freedesktop/DBus org.freedesktop.DBus.NameHasOwner \
        string:me.aresa.GlimpseSunset 2>/dev/null | grep -c "true" || true)
    dev=$(dbus-send --session --print-reply --dest=org.freedesktop.DBus \
        /org/freedesktop/DBus org.freedesktop.DBus.NameHasOwner \
        string:me.aresa.GlimpseSunset.Dev 2>/dev/null | grep -c "true" || true)
    [[ "$prod" == "0" && "$dev" == "0" ]] && break
    sleep 0.25
done
rm -f "$SOCKET"

# ── tests: flags and help (no daemon required) ────────────────────────────────

echo ""
echo "=== --help ==="
"$BINARY" --help | grep -q "COMMANDS" || fail "--help missing COMMANDS"
pass "--help"

echo ""
echo "=== --version ==="
"$BINARY" --version | grep -q "glimpse-sunset" || fail "--version missing name"
pass "--version"

echo ""
echo "=== watch --help ==="
"$BINARY" watch --help | grep -q "EVENTS" || fail "watch --help missing EVENTS"
"$BINARY" watch --help | grep -q "nightlight" || fail "watch --help missing nightlight events"
pass "watch --help"

echo ""
echo "=== dispatch --help ==="
"$BINARY" dispatch --help | grep -q "COMMANDS" || fail "dispatch --help missing COMMANDS"
"$BINARY" dispatch --help | grep -q "set_schedule" || fail "dispatch --help missing set_schedule"
pass "dispatch --help"

echo ""
echo "=== watch/dispatch with no daemon → immediate connection error ==="
! "$BINARY" watch 2>/dev/null || fail "watch with no daemon should exit non-zero"
! "$BINARY" dispatch status 2>/dev/null || fail "dispatch with no daemon should exit non-zero"
pass "no daemon → immediate error"

# ── start daemon ──────────────────────────────────────────────────────────────

echo ""
echo "starting daemon..."
"$BINARY" &
DAEMON_PID=$!
for i in $(seq 1 50); do [[ -S "$SOCKET" ]] && break; sleep 0.1; done
[[ -S "$SOCKET" ]] || { echo "ERROR: socket never appeared" >&2; exit 1; }
echo "daemon ready (pid $DAEMON_PID)"

WATCH_OUT=$(mktemp)
"$BINARY" watch >"$WATCH_OUT" 2>&1 &
WATCHER_PID=$!
sleep 0.3  # let watcher subscribe

# ── dispatch: status ──────────────────────────────────────────────────────────

echo ""
echo "=== dispatch status ==="
OUT=$("$BINARY" dispatch status)
echo "  $OUT"
assert_contains "$OUT" "ok=true"
assert_contains "$OUT" "phase="
assert_contains "$OUT" "kelvin="
assert_contains "$OUT" "schedule="
assert_contains "$OUT" "health="
pass "dispatch status"

echo ""
echo "=== dispatch status --json ==="
OUT=$("$BINARY" dispatch --json status)
echo "  $OUT"
echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['ok']==True; assert all(k in d for k in ['phase','kelvin','schedule','health'])" \
    || fail "status --json missing fields or ok!=true"
pass "dispatch status --json"

# ── dispatch: solar ───────────────────────────────────────────────────────────

echo ""
echo "=== dispatch solar ==="
OUT=$("$BINARY" dispatch solar)
echo "  $OUT"
assert_contains "$OUT" "ok=true"
assert_contains "$OUT" "state="
pass "dispatch solar"

echo ""
echo "=== dispatch solar --json ==="
OUT=$("$BINARY" dispatch --json solar)
echo "  $OUT"
echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['ok']==True; assert 'state' in d" \
    || fail "solar --json missing state or ok!=true"
pass "dispatch solar --json"

# ── dispatch: disable → event ─────────────────────────────────────────────────

echo ""
echo "=== dispatch disable → event nightlight.phase_changed ==="
BEFORE=$(watch_line_count)
OUT=$("$BINARY" dispatch disable)
assert_contains "$OUT" "ok=true"
expect_event_from "$((BEFORE + 1))" "nightlight.phase_changed"
STATUS=$("$BINARY" dispatch status)
assert_contains "$STATUS" "schedule=off"
pass "dispatch disable"

# ── dispatch: enable → event ──────────────────────────────────────────────────

echo ""
echo "=== dispatch enable → event nightlight.phase_changed ==="
BEFORE=$(watch_line_count)
OUT=$("$BINARY" dispatch enable)
assert_contains "$OUT" "ok=true"
expect_event_from "$((BEFORE + 1))" "nightlight.phase_changed"
STATUS=$("$BINARY" dispatch status)
assert_contains "$STATUS" "schedule=automatic"
pass "dispatch enable"

# ── dispatch: activate ────────────────────────────────────────────────────────

echo ""
echo "=== dispatch activate → nightlight.activated ==="
BEFORE=$(watch_line_count)
OUT=$("$BINARY" dispatch activate)
assert_contains "$OUT" "ok=true"
expect_event_from "$((BEFORE + 1))" "nightlight.activated" 4
STATUS=$("$BINARY" dispatch status)
assert_contains "$STATUS" "phase=night"
pass "dispatch activate"

# Restore to automatic before continuing
BEFORE=$(watch_line_count)
"$BINARY" dispatch enable >/dev/null
expect_event_from "$((BEFORE + 1))" "nightlight.phase_changed" 4

# ── M1 regression: disable clears the manual override ─────────────────────────
# activate sets a forced-Night override; disable must clear it, otherwise a
# later set_schedule resurrects forced Night because ApplyConfig preserves it.

echo ""
echo "=== M1: disable clears manual override ==="
BEFORE=$(watch_line_count)
"$BINARY" dispatch activate >/dev/null
expect_event_from "$((BEFORE + 1))" "nightlight.activated" 4
STATUS=$("$BINARY" dispatch status)
assert_contains "$STATUS" "manual=night"
BEFORE=$(watch_line_count)
"$BINARY" dispatch disable >/dev/null
expect_event_from "$((BEFORE + 1))" "nightlight.phase_changed" 4
BEFORE=$(watch_line_count)
"$BINARY" dispatch set_schedule schedule=automatic >/dev/null
expect_event_from "$((BEFORE + 1))" "nightlight.phase_changed" 4
STATUS=$("$BINARY" dispatch status)
echo "  $STATUS"
echo "$STATUS" | grep -q "manual=night" \
    && fail "manual override survived disable (M1 regression)"
pass "disable clears manual override"

# ── dispatch: set_schedule → event ────────────────────────────────────────────

echo ""
echo "=== dispatch set_schedule schedule=off → event ==="
BEFORE=$(watch_line_count)
OUT=$("$BINARY" dispatch set_schedule schedule=off)
assert_contains "$OUT" "ok=true"
expect_event_from "$((BEFORE + 1))" "nightlight.phase_changed"
STATUS=$("$BINARY" dispatch status)
assert_contains "$STATUS" "schedule=off"
pass "dispatch set_schedule off"

echo ""
echo "=== dispatch set_schedule schedule=automatic → event ==="
BEFORE=$(watch_line_count)
OUT=$("$BINARY" dispatch set_schedule schedule=automatic)
assert_contains "$OUT" "ok=true"
expect_event_from "$((BEFORE + 1))" "nightlight.phase_changed"
STATUS=$("$BINARY" dispatch status)
assert_contains "$STATUS" "schedule=automatic"
pass "dispatch set_schedule automatic"

echo ""
echo "=== dispatch set_schedule schedule=invalid → ok=false ==="
OUT=$("$BINARY" dispatch set_schedule schedule=invalid 2>&1 || true)
assert_contains "$OUT" "ok=false"
pass "dispatch set_schedule invalid → ok=false"

# ── dispatch: set_temperature (verify via status; effective only changes at night) ──

echo ""
echo "=== dispatch set_temperature kelvin=3500 → target_kelvin in status ==="
OUT=$("$BINARY" dispatch set_temperature kelvin=3500)
assert_contains "$OUT" "ok=true"
STATUS=$("$BINARY" dispatch status)
assert_contains "$STATUS" "target_kelvin=3500"
pass "dispatch set_temperature"

echo ""
echo "=== dispatch set_temperature out-of-range → ok=false (M2 guard) ==="
OUT=$("$BINARY" dispatch set_temperature kelvin=500 2>&1 || true)
assert_contains "$OUT" "ok=false"
OUT=$("$BINARY" dispatch set_temperature kelvin=9000 2>&1 || true)
assert_contains "$OUT" "ok=false"
STATUS=$("$BINARY" dispatch status)
assert_contains "$STATUS" "target_kelvin=3500"
pass "set_temperature rejects out-of-range kelvin, target unchanged"

# Force Night phase via Manual(true) command to test temperature_changed in Night phase.
echo ""
echo "=== activate → nightlight.activated + temperature_changed ==="
BEFORE=$(watch_line_count)
"$BINARY" dispatch activate >/dev/null
expect_event_from "$((BEFORE + 1))" "nightlight.activated" 4
STATUS=$("$BINARY" dispatch status)
assert_contains "$STATUS" "phase=night"
assert_contains "$STATUS" "manual=night"
BEFORE2=$(watch_line_count)
OUT=$("$BINARY" dispatch set_temperature kelvin=2700)
assert_contains "$OUT" "ok=true"
expect_event_from "$((BEFORE2 + 1))" "nightlight.temperature_changed" 3
pass "set_temperature in Night phase → nightlight.temperature_changed"

# ── dispatch: reset → temperature_changed ─────────────────────────────────────

echo ""
echo "=== dispatch reset → nightlight.temperature_changed ==="
BEFORE=$(watch_line_count)
OUT=$("$BINARY" dispatch reset)
assert_contains "$OUT" "ok=true"
expect_event_from "$((BEFORE + 1))" "nightlight.temperature_changed" 3
pass "dispatch reset"

# Restore to automatic schedule
"$BINARY" dispatch set_schedule schedule=automatic >/dev/null

# ── dispatch: set_location ────────────────────────────────────────────────────

echo ""
echo "=== dispatch set_location lat=52.23 lon=21.01 ==="
OUT=$("$BINARY" dispatch set_location lat=52.23 lon=21.01)
echo "  $OUT"
assert_contains "$OUT" "ok=true"
pass "dispatch set_location"

echo ""
echo "=== dispatch set_location out-of-range → ok=false (bounds guard) ==="
OUT=$("$BINARY" dispatch set_location lat=999 lon=0 2>&1 || true)
assert_contains "$OUT" "ok=false"
OUT=$("$BINARY" dispatch set_location lat=0 lon=999 2>&1 || true)
assert_contains "$OUT" "ok=false"
pass "set_location rejects out-of-range coordinates"

# ── dispatch: set_times (L3: switches schedule to Schedule) ───────────────────

echo ""
echo "=== dispatch set_times start=22:00 end=06:00 → schedule=schedule ==="
OUT=$("$BINARY" dispatch set_times start=22:00 end=06:00)
echo "  $OUT"
assert_contains "$OUT" "ok=true"
sleep 0.3
STATUS=$("$BINARY" dispatch status)
echo "  $STATUS"
assert_contains "$STATUS" "schedule=schedule"
pass "set_times sets the manual window and switches to Schedule mode"

# Restore to automatic schedule
"$BINARY" dispatch set_schedule schedule=automatic >/dev/null

# ── dispatch: refresh ─────────────────────────────────────────────────────────

echo ""
echo "=== dispatch refresh ==="
OUT=$("$BINARY" dispatch refresh)
assert_contains "$OUT" "ok=true"
pass "dispatch refresh"

# ── dispatch: unknown command → ok=false ──────────────────────────────────────

echo ""
echo "=== dispatch unknown command → ok=false ==="
OUT=$("$BINARY" dispatch frobnicate 2>&1 || true)
assert_contains "$OUT" "ok=false"
pass "dispatch unknown command → ok=false"

# ── dispatch: malformed field → ok=false, not a hang ─────────────────────────

echo ""
echo "=== dispatch malformed arg (no key=value) → ok=false, not a hang ==="
OUT=$("$BINARY" dispatch set_schedule off 2>&1 || true)
assert_contains "$OUT" "ok=false"
pass "dispatch malformed arg → ok=false immediately"

# Wait for the Night→Day transition from the previous block to complete.
for i in $(seq 1 30); do
    _phase=$("$BINARY" dispatch status 2>/dev/null | grep -o "phase=[a-z_]*" | cut -d= -f2 || true)
    [[ "$_phase" == "day" || "$_phase" == "disabled" ]] && break
    sleep 0.3
done
sleep 0.3

# ── watch --json ──────────────────────────────────────────────────────────────

echo ""
echo "=== watch --json ==="
JSON_OUT=$(mktemp)
"$BINARY" watch --json >"$JSON_OUT" 2>&1 &
JSON_PID=$!
sleep 0.5  # let watcher connect and subscribe

# Trigger a reliable event pair (disable then enable produces two phase_changed).
"$BINARY" dispatch disable >/dev/null
"$BINARY" dispatch enable >/dev/null
sleep 0.8  # let events arrive

kill "$JSON_PID" 2>/dev/null || true
wait "$JSON_PID" 2>/dev/null || true

if [[ -s "$JSON_OUT" ]]; then
    while IFS= read -r line; do
        echo "$line" | python3 -c \
            "import sys,json; d=json.load(sys.stdin); assert d.get('type')=='event'; assert 'name' in d; assert isinstance(d.get('ts'),int)" \
            || fail "invalid JSON event line: $line"
    done < "$JSON_OUT"
    pass "watch --json events are valid JSON"
else
    fail "watch --json received no events"
fi
rm -f "$JSON_OUT"

# ── watch output summary ──────────────────────────────────────────────────────

kill "$WATCHER_PID" 2>/dev/null || true
wait "$WATCHER_PID" 2>/dev/null || true
WATCHER_PID=""

echo ""
echo "=== watch output (all events received) ==="
cat "$WATCH_OUT"
[[ -s "$WATCH_OUT" ]] || fail "watch received no events at all"
grep -q "nightlight.phase_changed" "$WATCH_OUT" || fail "watch missing nightlight.phase_changed"
grep -q "nightlight.temperature_changed" "$WATCH_OUT" || fail "watch missing nightlight.temperature_changed"
pass "watch received all expected event types"

echo ""
echo "=== ALL TESTS PASSED ==="
