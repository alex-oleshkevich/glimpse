#!/usr/bin/env bash
# E2E test: stop the glimpse-wallpaper systemd service, run a freshly built
# binary under the live Wayland session, exercise every watch + dispatch path,
# verify via the event stream (there is no `status` command), then restore.
#
# INTRUSIVE: this stops/starts glimpse-wallpaper.service, briefly changes the
# real desktop wallpaper, and edits the on-disk config.toml (restored on exit).
set -euo pipefail

BINARY="$(cargo build -p glimpse-wallpaper --message-format=json 2>/dev/null \
    | python3 -c "import sys,json; [print(o['executable']) for l in sys.stdin for o in [json.loads(l)] if o.get('reason')=='compiler-artifact' and 'glimpse-wallpaper' in o.get('target',{}).get('name','') and o.get('executable')]" \
    | tail -1)"

[[ -z "$BINARY" ]] && { echo "ERROR: could not locate binary" >&2; exit 1; }
echo "binary: $BINARY"

SOCKET="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/glimpse/wallpaper.sock"
SERVICE_WAS_ACTIVE=false
DAEMON_PID=""
WATCHER_PID=""
WATCH_OUT=""
DAEMON_LOG=""
CONFIG_PATH=""
CONFIG_BACKUP=""
TMP_PNG=""

# ── helpers ────────────────────────────────────────────────────────────────────

pass() { echo "  PASS: $*"; }
fail() { echo "  FAIL: $*" >&2; exit 1; }

assert_contains() {
    echo "$1" | grep -q "$2" || fail "expected '$2' in: $1"
}

expect_event_from() {
    local from_line="$1"
    local contains="$2"
    local timeout="${3:-3}"
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
    [[ -n "$DAEMON_LOG" ]] && rm -f "$DAEMON_LOG"
    [[ -n "$TMP_PNG" ]] && rm -f "$TMP_PNG"
    # Restore config.toml BEFORE restarting the service so it reads the original.
    if [[ -n "$CONFIG_BACKUP" && -f "$CONFIG_BACKUP" ]]; then
        echo "restoring $CONFIG_PATH"
        mv -f "$CONFIG_BACKUP" "$CONFIG_PATH"
    fi
    if $SERVICE_WAS_ACTIVE; then
        echo "restoring glimpse-wallpaper.service..."
        systemctl --user start glimpse-wallpaper
        echo "service restored"
    fi
}
trap cleanup EXIT

# ── stop everything ───────────────────────────────────────────────────────────

if systemctl --user is-active --quiet glimpse-wallpaper; then
    SERVICE_WAS_ACTIVE=true
    echo "stopping glimpse-wallpaper.service..."
    systemctl --user stop glimpse-wallpaper
    sleep 1
fi
# `glimpse-wallpaper` (17 chars) exceeds the 15-char process comm limit, so
# `pkill -x` cannot match it. Match the executable path instead, anchored so
# this script's own cmdline (…/glimpse-wallpaper/tests/…) is not killed.
pkill -f -- '/glimpse-wallpaper( |$)' 2>/dev/null || true
for i in $(seq 1 40); do
    a=$(dbus-send --session --print-reply --dest=org.freedesktop.DBus \
        /org/freedesktop/DBus org.freedesktop.DBus.NameHasOwner \
        string:me.aresa.GlimpseWallpaper.App 2>/dev/null | grep -c "true" || true)
    b=$(dbus-send --session --print-reply --dest=org.freedesktop.DBus \
        /org/freedesktop/DBus org.freedesktop.DBus.NameHasOwner \
        string:me.aresa.GlimpseWallpaper 2>/dev/null | grep -c "true" || true)
    [[ "$a" == "0" && "$b" == "0" ]] && break
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
"$BINARY" --version | grep -q "glimpse-wallpaper" || fail "--version missing name"
pass "--version"

echo ""
echo "=== watch --help ==="
"$BINARY" watch --help | grep -q "EVENTS" || fail "watch --help missing EVENTS"
"$BINARY" watch --help | grep -q "wallpaper" || fail "watch --help missing wallpaper events"
pass "watch --help"

echo ""
echo "=== dispatch --help ==="
"$BINARY" dispatch --help | grep -q "COMMANDS" || fail "dispatch --help missing COMMANDS"
"$BINARY" dispatch --help | grep -q "set_image" || fail "dispatch --help missing set_image"
pass "dispatch --help"

echo ""
echo "=== watch/dispatch with no daemon → immediate connection error ==="
! "$BINARY" watch 2>/dev/null || fail "watch with no daemon should exit non-zero"
! "$BINARY" dispatch reload_config 2>/dev/null || fail "dispatch with no daemon should exit non-zero"
pass "no daemon → immediate error"

# ── start daemon ──────────────────────────────────────────────────────────────

echo ""
echo "starting daemon..."
DAEMON_LOG=$(mktemp)
"$BINARY" >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!
for i in $(seq 1 80); do [[ -S "$SOCKET" ]] && break; sleep 0.1; done
[[ -S "$SOCKET" ]] || { echo "ERROR: socket never appeared" >&2; cat "$DAEMON_LOG" >&2; exit 1; }
# The daemon logs the config file it watches (via the tracing subscriber, which
# emits ANSI colour codes even to a pipe — strip them before parsing).
strip_ansi() { sed 's/\x1b\[[0-9;]*m//g'; }
for i in $(seq 1 100); do
    CONFIG_PATH=$(strip_ansi <"$DAEMON_LOG" | grep -o 'config_file=[^ ]*' | head -1 | cut -d= -f2 || true)
    [[ -n "$CONFIG_PATH" ]] && break
    sleep 0.1
done
[[ -n "$CONFIG_PATH" && -f "$CONFIG_PATH" ]] || {
    echo "--- daemon log ---" >&2
    strip_ansi <"$DAEMON_LOG" >&2
    fail "could not determine config path from daemon log"
}
echo "daemon ready (pid $DAEMON_PID), config: $CONFIG_PATH"

WATCH_OUT=$(mktemp)
"$BINARY" watch >"$WATCH_OUT" 2>&1 &
WATCHER_PID=$!
sleep 0.3  # let watcher subscribe

# A minimal valid 1x1 PNG for set_image tests.
TMP_PNG=$(mktemp --suffix=.png)
base64 -d > "$TMP_PNG" <<'PNG'
iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==
PNG

# ── set_color ─────────────────────────────────────────────────────────────────

echo ""
echo "=== dispatch set_color color=#ff00ff → wallpaper.spec_changed ==="
BEFORE=$(watch_line_count)
OUT=$("$BINARY" dispatch set_color color=#ff00ff)
assert_contains "$OUT" "ok=true"
expect_event_from "$((BEFORE + 1))" "wallpaper.spec_changed"
tail -n +"$((BEFORE + 1))" "$WATCH_OUT" | grep "wallpaper.spec_changed" | grep -q "color=#ff00ff" \
    || fail "spec_changed missing color=#ff00ff"
pass "set_color"

echo ""
echo "=== dispatch set_color color=not%%a%%color → ok=false ==="
OUT=$("$BINARY" dispatch set_color "color=not%%a%%color" 2>&1 || true)
assert_contains "$OUT" "ok=false"
pass "set_color invalid → ok=false"

# ── set_image ─────────────────────────────────────────────────────────────────

echo ""
echo "=== dispatch set_image path=<tmp png> → wallpaper.spec_changed mode=image ==="
BEFORE=$(watch_line_count)
OUT=$("$BINARY" dispatch set_image "path=$TMP_PNG")
assert_contains "$OUT" "ok=true"
expect_event_from "$((BEFORE + 1))" "wallpaper.spec_changed"
LINE=$(tail -n +"$((BEFORE + 1))" "$WATCH_OUT" | grep "wallpaper.spec_changed" | tail -1)
assert_contains "$LINE" "mode=image"
assert_contains "$LINE" "path=$TMP_PNG"
pass "set_image"

echo ""
echo "=== dispatch set_image bad paths → ok=false ==="
OUT=$("$BINARY" dispatch set_image path=/definitely/not/here.png 2>&1 || true)
assert_contains "$OUT" "ok=false"
OUT=$("$BINARY" dispatch set_image path=relative.png 2>&1 || true)
assert_contains "$OUT" "ok=false"
pass "set_image rejects missing/relative path"

# ── set_fit (needs an active image, set above) ────────────────────────────────

echo ""
echo "=== dispatch set_fit mode=contain → wallpaper.spec_changed fit=contain ==="
BEFORE=$(watch_line_count)
OUT=$("$BINARY" dispatch set_fit mode=contain)
assert_contains "$OUT" "ok=true"
expect_event_from "$((BEFORE + 1))" "wallpaper.spec_changed"
tail -n +"$((BEFORE + 1))" "$WATCH_OUT" | grep "wallpaper.spec_changed" | grep -q "fit=contain" \
    || fail "spec_changed missing fit=contain"
pass "set_fit"

echo ""
echo "=== dispatch set_fit mode=bogus → ok=false ==="
OUT=$("$BINARY" dispatch set_fit mode=bogus 2>&1 || true)
assert_contains "$OUT" "ok=false"
pass "set_fit invalid → ok=false"

# ── set_backdrop ──────────────────────────────────────────────────────────────

echo ""
echo "=== dispatch set_backdrop enabled=true blur=40 → backdrop=true backdrop_blur=40 ==="
BEFORE=$(watch_line_count)
OUT=$("$BINARY" dispatch set_backdrop enabled=true blur=40)
assert_contains "$OUT" "ok=true"
expect_event_from "$((BEFORE + 1))" "wallpaper.spec_changed"
LINE=$(tail -n +"$((BEFORE + 1))" "$WATCH_OUT" | grep "wallpaper.spec_changed" | tail -1)
assert_contains "$LINE" "backdrop=true"
assert_contains "$LINE" "backdrop_blur=40"
pass "set_backdrop enable"

echo ""
echo "=== dispatch set_backdrop enabled=false → backdrop=false ==="
BEFORE=$(watch_line_count)
OUT=$("$BINARY" dispatch set_backdrop enabled=false)
assert_contains "$OUT" "ok=true"
expect_event_from "$((BEFORE + 1))" "wallpaper.spec_changed"
tail -n +"$((BEFORE + 1))" "$WATCH_OUT" | grep "wallpaper.spec_changed" | grep -q "backdrop=false" \
    || fail "spec_changed missing backdrop=false"
pass "set_backdrop disable"

echo ""
echo "=== dispatch set_backdrop enabled=notabool → ok=false ==="
OUT=$("$BINARY" dispatch set_backdrop enabled=notabool 2>&1 || true)
assert_contains "$OUT" "ok=false"
pass "set_backdrop invalid → ok=false"

# ── set_theme_mode (force light first so dark is a guaranteed transition) ──────

echo ""
echo "=== dispatch set_theme_mode mode=light then mode=dark → wallpaper.theme_changed ==="
"$BINARY" dispatch set_theme_mode mode=light >/dev/null
sleep 0.3
BEFORE=$(watch_line_count)
OUT=$("$BINARY" dispatch set_theme_mode mode=dark)
assert_contains "$OUT" "ok=true"
expect_event_from "$((BEFORE + 1))" "wallpaper.theme_changed" 4
tail -n +"$((BEFORE + 1))" "$WATCH_OUT" | grep "wallpaper.theme_changed" | grep -q "mode=dark" \
    || fail "theme_changed missing mode=dark"
pass "set_theme_mode dark → theme_changed"

echo ""
echo "=== dispatch set_theme_mode mode=auto → ok=true; mode=bogus → ok=false ==="
OUT=$("$BINARY" dispatch set_theme_mode mode=auto)
assert_contains "$OUT" "ok=true"
OUT=$("$BINARY" dispatch set_theme_mode mode=bogus 2>&1 || true)
assert_contains "$OUT" "ok=false"
pass "set_theme_mode auto ok / bogus → ok=false"

# ── override-survives-config-edit regression (the §3 critical path) ───────────

echo ""
echo "=== override survives an unrelated config edit ==="
CONFIG_BACKUP=$(mktemp)
cp "$CONFIG_PATH" "$CONFIG_BACKUP"
"$BINARY" dispatch set_color color=#abcdef >/dev/null
sleep 0.4
BEFORE=$(watch_line_count)
# Bump transition_ms: changes the base spec (not an event-visible field, not
# overridden), so a fresh spec_changed fires while the override colour stays.
sed -i 's/^transition_ms = .*/transition_ms = 801/' "$CONFIG_PATH"
expect_event_from "$((BEFORE + 1))" "wallpaper.spec_changed" 5
tail -n +"$((BEFORE + 1))" "$WATCH_OUT" | grep "wallpaper.spec_changed" | tail -1 | grep -q "color=#abcdef" \
    || fail "override colour did NOT survive config edit (M-config regression)"
pass "IPC override survives unrelated config edit"

# ── reload_config clears the override ─────────────────────────────────────────

echo ""
echo "=== dispatch reload_config → override cleared, colour reverts to config ==="
CONFIG_COLOR=$(grep -m1 '^color = ' "$CONFIG_PATH" | sed 's/^color = "\?\([^"]*\)"\?.*/\1/')
BEFORE=$(watch_line_count)
OUT=$("$BINARY" dispatch reload_config)
assert_contains "$OUT" "ok=true"
expect_event_from "$((BEFORE + 1))" "wallpaper.spec_changed" 5
tail -n +"$((BEFORE + 1))" "$WATCH_OUT" | grep "wallpaper.spec_changed" | tail -1 | grep -q "color=$CONFIG_COLOR" \
    || fail "reload_config did not revert colour to config value '$CONFIG_COLOR'"
pass "reload_config clears override"

# ── error paths ───────────────────────────────────────────────────────────────

echo ""
echo "=== unknown command → ok=false ==="
OUT=$("$BINARY" dispatch frobnicate 2>&1 || true)
assert_contains "$OUT" "ok=false"
pass "unknown command → ok=false"

echo ""
echo "=== malformed arg (no key=value) → ok=false, not a hang ==="
OUT=$("$BINARY" dispatch set_fit contain 2>&1 || true)
assert_contains "$OUT" "ok=false"
pass "malformed arg → ok=false immediately"

# ── watch --json ──────────────────────────────────────────────────────────────

echo ""
echo "=== watch --json ==="
JSON_OUT=$(mktemp)
"$BINARY" watch --json >"$JSON_OUT" 2>&1 &
JSON_PID=$!
sleep 0.5
"$BINARY" dispatch set_color color=#222222 >/dev/null
"$BINARY" dispatch set_color color=#333333 >/dev/null
sleep 0.8
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
grep -q "wallpaper.spec_changed" "$WATCH_OUT" || fail "watch missing wallpaper.spec_changed"
grep -q "wallpaper.theme_changed" "$WATCH_OUT" || fail "watch missing wallpaper.theme_changed"
pass "watch received all expected event types"

echo ""
echo "=== ALL TESTS PASSED ==="
