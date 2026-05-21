#!/usr/bin/env bash
# Non-intrusive E2E for the glimpse-shell IPC command/event surface.
#
# It does NOT stop your running shell: a second glimpse-shell is started with
# an isolated IPC socket (GLIMPSE_IPC_DIR), a distinct GTK app id
# (GLIMPSE_SHELL_APP_ID) and a sandboxed config (XDG_CONFIG_HOME), running in
# your current Wayland session (a second bar is briefly visible). It only ever
# kills its own PIDs — never `pkill`s glimpse-shell.
#
# Side-effect policy (services are the real pipewire/NM/bluez/etc — not
# sandboxable):
#   Tier A  reversible: captured from `status`, exercised, restored (volume,
#           mute, dnd, brightness, power profile, input/keyboard toggles).
#   Tier B  destructive: only the confirm=true guard is checked; never run.
#   Tier C  disruptive/irreversible (wifi/bt radios, media skip, notification
#           dismiss, theme): only arg/validation + harmless no-op paths — the
#           real effect is never triggered.
set -euo pipefail

BINARY="$(cargo build -p glimpse-shell --message-format=json 2>/dev/null \
    | python3 -c "import sys,json; [print(o['executable']) for l in sys.stdin for o in [json.loads(l)] if o.get('reason')=='compiler-artifact' and o.get('target',{}).get('name')=='glimpse-shell' and o.get('executable')]" \
    | tail -1)"
[[ -z "$BINARY" ]] && { echo "ERROR: could not locate binary" >&2; exit 1; }
echo "binary: $BINARY"

ROOT="$(mktemp -d)"
export GLIMPSE_IPC_DIR="$ROOT/ipc"
export GLIMPSE_SHELL_APP_ID="me.aresa.GlimpseShell.e2e"
export XDG_CONFIG_HOME="$ROOT/cfg"
mkdir -p "$XDG_CONFIG_HOME/glimpse"
printf '[[panels]]\nposition = "top"\nleft = ["clock"]\n' > "$XDG_CONFIG_HOME/glimpse/config.toml"
SOCKET="$ROOT/ipc/ipc.sock"
DAEMON_PID=""; WATCHER_PID=""; WATCH_OUT=""; DAEMON_LOG=""
ORIG_DND=""; ORIG_VOLUME=""; ORIG_MUTED=""; ORIG_BRIGHTNESS=""; ORIG_PROFILE=""

pass() { echo "  PASS: $*"; }
fail() { echo "  FAIL: $*" >&2; exit 1; }
ac()   { echo "$1" | grep -q -- "$2" || fail "expected '$2' in: $1"; }
wlc()  { wc -l < "$WATCH_OUT" 2>/dev/null || echo 0; }
sf()   { "$BINARY" dispatch status 2>/dev/null | grep -o "$1=[^ ]*" | head -1 | cut -d= -f2; }
ev_from() {
    local from="$1" want="$2" to="${3:-4}"; local dl=$((SECONDS + to))
    while [[ $SECONDS -lt $dl ]]; do
        tail -n +"$from" "$WATCH_OUT" 2>/dev/null | grep -q -- "$want" && { pass "event: $want"; return 0; }
        sleep 0.1
    done
    fail "timed out waiting for '$want':"$'\n'"$(tail -n +"$from" "$WATCH_OUT" 2>/dev/null || true)"
}

cleanup() {
    echo "--- cleanup (restoring real state we touched) ---"
    if [[ -S "$SOCKET" ]]; then
        [[ -n "$ORIG_DND"        ]] && "$BINARY" dispatch set_dnd "enabled=$ORIG_DND" >/dev/null 2>&1 || true
        [[ -n "$ORIG_VOLUME"     ]] && "$BINARY" dispatch set_volume "level=$ORIG_VOLUME" >/dev/null 2>&1 || true
        if [[ -n "$ORIG_MUTED" ]]; then
            cur=$(sf audio_muted); [[ "$cur" != "$ORIG_MUTED" ]] && "$BINARY" dispatch toggle_mute >/dev/null 2>&1 || true
        fi
        [[ -n "$ORIG_BRIGHTNESS" ]] && "$BINARY" dispatch set_brightness "percent=$ORIG_BRIGHTNESS" >/dev/null 2>&1 || true
        [[ -n "$ORIG_PROFILE"    ]] && "$BINARY" dispatch set_power_profile "profile=$ORIG_PROFILE" >/dev/null 2>&1 || true
    fi
    [[ -n "$WATCHER_PID" ]] && kill "$WATCHER_PID" 2>/dev/null || true
    [[ -n "$DAEMON_PID"  ]] && { kill "$DAEMON_PID" 2>/dev/null || true; wait "$DAEMON_PID" 2>/dev/null || true; }
    rm -rf "$ROOT"
}
trap cleanup EXIT

# ── flags / help / no-daemon (isolated socket → never hits your real shell) ────

echo ""; echo "=== --help / --version / no daemon ==="
"$BINARY" --help | grep -q "dispatch" || fail "--help missing dispatch"
"$BINARY" dispatch --help | grep -q "set_volume" || fail "dispatch --help missing catalogue"
"$BINARY" --version | grep -q "glimpse-shell" || fail "--version"
! "$BINARY" dispatch status 2>/dev/null || fail "dispatch with no (sandbox) daemon should fail"
pass "help/version/no-daemon"

# ── start an isolated second shell (your shell keeps running) ─────────────────

echo ""; echo "starting isolated glimpse-shell (sandboxed socket+app-id; your shell untouched)..."
DAEMON_LOG=$(mktemp)
"$BINARY" >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!
for i in $(seq 1 150); do [[ -S "$SOCKET" ]] && break; sleep 0.1; done
[[ -S "$SOCKET" ]] || { echo "ERROR: isolated shell never bound $SOCKET" >&2; sed 's/\x1b\[[0-9;]*m//g' "$DAEMON_LOG" | tail -20 >&2; exit 1; }
sleep 1
echo "isolated shell ready (pid $DAEMON_PID, socket $SOCKET)"
WATCH_OUT=$(mktemp)
"$BINARY" watch >"$WATCH_OUT" 2>&1 & WATCHER_PID=$!
sleep 0.3

echo ""; echo "=== status ==="
OUT=$("$BINARY" dispatch status); echo "  $OUT"
ac "$OUT" "ok=true"; ac "$OUT" "connectivity="; ac "$OUT" "dnd="
pass "status"
ORIG_DND=$(sf dnd); [[ -z "$ORIG_DND" ]] && ORIG_DND=false
ORIG_VOLUME=$(sf audio_volume)
ORIG_MUTED=$(sf audio_muted)
ORIG_BRIGHTNESS=$(sf brightness_percent)
ORIG_PROFILE=$(sf power_profile)

# ── Tier A: reversible, capture → exercise → assert event → restore ───────────

echo ""; echo "=== Tier A: set_dnd ==="
if [[ "$ORIG_DND" == "true" ]]; then td=false; ev=notification.dnd_disabled; else td=true; ev=notification.dnd_enabled; fi
B=$(wlc); ac "$("$BINARY" dispatch set_dnd "enabled=$td")" "ok=true"; ev_from "$((B+1))" "$ev"
"$BINARY" dispatch set_dnd "enabled=$ORIG_DND" >/dev/null; pass "set_dnd"

if [[ -n "$ORIG_VOLUME" ]]; then
  echo ""; echo "=== Tier A: set_volume ==="
  [[ "$ORIG_VOLUME" == 42 ]] && t=43 || t=42
  B=$(wlc); ac "$("$BINARY" dispatch set_volume "level=$t")" "ok=true"; ev_from "$((B+1))" "audio.volume_changed"
  "$BINARY" dispatch set_volume "level=$ORIG_VOLUME" >/dev/null; pass "set_volume"
fi

echo ""; echo "=== Tier A: toggle_mute (round-trip) ==="
B=$(wlc); ac "$("$BINARY" dispatch toggle_mute)" "ok=true"; ev_from "$((B+1))" "audio."
"$BINARY" dispatch toggle_mute >/dev/null; pass "toggle_mute round-trip"

echo ""; echo "=== Tier A: toggle_input_mute (round-trip) ==="
ac "$("$BINARY" dispatch toggle_input_mute)" "ok=true"
"$BINARY" dispatch toggle_input_mute >/dev/null; pass "toggle_input_mute round-trip"

if [[ -n "$ORIG_BRIGHTNESS" ]]; then
  echo ""; echo "=== Tier A: set_brightness / adjust_brightness ==="
  [[ "$ORIG_BRIGHTNESS" -ge 50 ]] && t=$((ORIG_BRIGHTNESS-10)) || t=$((ORIG_BRIGHTNESS+10))
  ac "$("$BINARY" dispatch set_brightness "percent=$t")" "ok=true"
  ac "$("$BINARY" dispatch adjust_brightness delta=1)" "ok=true"
  ac "$("$BINARY" dispatch adjust_brightness delta=-1)" "ok=true"
  "$BINARY" dispatch set_brightness "percent=$ORIG_BRIGHTNESS" >/dev/null
  pass "brightness set/adjust (restored)"
fi

if [[ -n "$ORIG_PROFILE" ]]; then
  echo ""; echo "=== Tier A: set_power_profile ==="
  [[ "$ORIG_PROFILE" == balanced ]] && t=power-saver || t=balanced
  B=$(wlc); ac "$("$BINARY" dispatch set_power_profile "profile=$t")" "ok=true"
  ev_from "$((B+1))" "power.profile_changed" || true
  "$BINARY" dispatch set_power_profile "profile=$ORIG_PROFILE" >/dev/null; pass "set_power_profile (restored)"
fi

echo ""; echo "=== Tier A: keyboard layout (round-trip, best-effort) ==="
B=$(wlc); ac "$("$BINARY" dispatch next_keyboard_layout)" "ok=true"
if tail -n +"$((B+1))" "$WATCH_OUT" 2>/dev/null | grep -q "input.keyboard_layout_changed" \
   || { sleep 1; tail -n +"$((B+1))" "$WATCH_OUT" | grep -q "input.keyboard_layout_changed"; }; then
    "$BINARY" dispatch prev_keyboard_layout >/dev/null; pass "keyboard layout cycled + restored"
else
    pass "keyboard layout acked (single layout — no change, nothing to restore)"
fi

echo ""; echo "=== harmless triggers (wifi_scan / bluetooth_scan / refresh) ==="
ac "$("$BINARY" dispatch wifi_scan)" "ok=true"
ac "$("$BINARY" dispatch bluetooth_scan action=start)" "ok=true"
ac "$("$BINARY" dispatch bluetooth_scan action=stop)" "ok=true"
ac "$("$BINARY" dispatch refresh service=battery)" "ok=true"
pass "scan/refresh acked"

# ── Tier C: disruptive — validation / no-op only, real effect NOT triggered ───

echo ""; echo "=== Tier C: validation only (no real wifi/bt/media/theme effect) ==="
ac "$("$BINARY" dispatch set_wifi enabled=notabool 2>&1 || true)" "ok=false"
ac "$("$BINARY" dispatch set_wifi 2>&1 || true)" "ok=false"
ac "$("$BINARY" dispatch set_bluetooth enabled=notabool 2>&1 || true)" "ok=false"
ac "$("$BINARY" dispatch connect_wifi ssid=x 2>&1 || true)" "ok=false"     # missing path
ac "$("$BINARY" dispatch connect_bluetooth 2>&1 || true)" "ok=false"        # missing address
ac "$("$BINARY" dispatch disconnect_bluetooth 2>&1 || true)" "ok=false"
ac "$("$BINARY" dispatch set_theme mode=bogus 2>&1 || true)" "ok=false"
ac "$("$BINARY" dispatch set_theme 2>&1 || true)" "ok=false"
ac "$("$BINARY" dispatch bluetooth_scan action=bogus 2>&1 || true)" "ok=false"
ac "$("$BINARY" dispatch refresh service=bogus 2>&1 || true)" "ok=false"
ac "$("$BINARY" dispatch dismiss_notification id=abc 2>&1 || true)" "ok=false"
ac "$("$BINARY" dispatch dismiss_notification 2>&1 || true)" "ok=false"
# bogus player id / non-existent notification: acked, no real player/queue touched
ac "$("$BINARY" dispatch media_play_pause 'player=does.not.exist')" "ok=true"
ac "$("$BINARY" dispatch dismiss_notification id=999999999)" "ok=true"
pass "Tier C validation + safe no-op paths"

# ── generic validation / unknown ─────────────────────────────────────────────

echo ""; echo "=== generic validation ==="
ac "$("$BINARY" dispatch frobnicate 2>&1 || true)" "ok=false"
ac "$("$BINARY" dispatch set_volume level=200 2>&1 || true)" "ok=false"
ac "$("$BINARY" dispatch set_volume level=abc 2>&1 || true)" "ok=false"
ac "$("$BINARY" dispatch set_volume 50 2>&1 || true)" "ok=false"   # no key=value
pass "generic validation"

# ── Tier B: destructive guard-only (effects NEVER triggered) ──────────────────

echo ""; echo "=== Tier B: destructive require confirm=true ==="
for c in "forget_wifi uuid=x" "forget_bluetooth address=x" "eject id=x" \
         "poweroff_drive id=x" "clear_clipboard" "clear_clipboard_history"; do
    OUT=$("$BINARY" dispatch $c 2>&1 || true)
    ac "$OUT" "ok=false"
    echo "$OUT" | grep -q "confirm=true" || fail "expected confirm hint for '$c': $OUT"
done
pass "destructive guarded (NOT executed)"

# ── Tier A: privacy detection (real hardware/services, reversible) ──────────
#
# Drives real triggers (gst-launch, pw-record, wf-recorder, where-am-i) and
# asserts the corresponding *.in_use / *.released IPC events. Each block is
# skipped if its trigger tool or hardware is unavailable. All triggers are
# brief and reversed before the next block runs.

where_am_i_bin=""
for c in "$(command -v where-am-i 2>/dev/null || true)" \
         /usr/lib/geoclue-2.0/demos/where-am-i \
         /usr/libexec/geoclue-2.0/demos/where-am-i \
         /usr/lib64/geoclue-2.0/demos/where-am-i; do
    [[ -n "$c" && -x "$c" ]] && { where_am_i_bin="$c"; break; }
done

has_pipewire_camera() {
    command -v pw-dump >/dev/null || return 1
    pw-dump 2>/dev/null \
        | python3 -c 'import sys,json
n=0
for o in json.load(sys.stdin):
    if o.get("type")!="PipeWire:Interface:Node": continue
    p=o.get("info",{}).get("props",{}) or {}
    if p.get("media.class")!="Video/Source": continue
    if (p.get("media.role")=="Camera"
        or (p.get("object.path") or "").startswith("v4l2:")
        or p.get("device.api") in ("v4l2","libcamera")):
        n+=1
print(n)' 2>/dev/null | grep -qv '^0$'
}

if command -v pw-record >/dev/null; then
    echo ""; echo "=== Tier A: privacy/mic (pw-record) ==="
    B=$(wlc)
    out=$(mktemp -t glimpse-e2e-mic.XXXXXX.wav)
    pw-record --rate=48000 --channels=1 --format=s16 "$out" >/dev/null 2>&1 &
    REC_PID=$!
    ev_from "$((B+1))" "mic.in_use" 6
    kill -INT "$REC_PID" 2>/dev/null || true; wait "$REC_PID" 2>/dev/null || true
    ev_from "$((B+1))" "mic.released" 6
    rm -f "$out"
    pass "mic in_use/released"
else
    echo ""; echo "=== Tier A: privacy/mic SKIPPED (pw-record not installed) ==="
fi

if command -v gst-launch-1.0 >/dev/null && has_pipewire_camera; then
    echo ""; echo "=== Tier A: privacy/webcam (gst-launch pipewiresrc) ==="
    B=$(wlc)
    gst-launch-1.0 -q pipewiresrc ! videoconvert ! fakesink >/dev/null 2>&1 &
    GST_PID=$!
    ev_from "$((B+1))" "webcam.in_use" 6
    kill -INT "$GST_PID" 2>/dev/null || true; wait "$GST_PID" 2>/dev/null || true
    ev_from "$((B+1))" "webcam.released" 6
    pass "webcam in_use/released"
else
    echo ""; echo "=== Tier A: privacy/webcam SKIPPED (gst-launch or PipeWire camera unavailable) ==="
fi

if command -v wf-recorder >/dev/null && [[ "${XDG_SESSION_TYPE:-}" == "wayland" ]]; then
    echo ""; echo "=== Tier A: privacy/screencast (wf-recorder) ==="
    B=$(wlc)
    out=$(mktemp -t glimpse-e2e-scr.XXXXXX.mkv)
    wf-recorder -f "$out" >/dev/null 2>&1 &
    REC_PID=$!
    ev_from "$((B+1))" "screencast.in_use" 6
    kill -INT "$REC_PID" 2>/dev/null || true; wait "$REC_PID" 2>/dev/null || true
    ev_from "$((B+1))" "screencast.released" 6
    rm -f "$out"
    pass "screencast in_use/released"
else
    echo ""; echo "=== Tier A: privacy/screencast SKIPPED (wf-recorder or Wayland unavailable) ==="
fi

if [[ -n "$where_am_i_bin" ]]; then
    echo ""; echo "=== Tier A: privacy/location (geoclue where-am-i) ==="
    B=$(wlc)
    "$where_am_i_bin" -t 2 >/dev/null 2>&1 &
    LOC_PID=$!
    ev_from "$((B+1))" "location.in_use" 6
    wait "$LOC_PID" 2>/dev/null || true
    ev_from "$((B+1))" "location.released" 6
    pass "location in_use/released"
else
    echo ""; echo "=== Tier A: privacy/location SKIPPED (geoclue where-am-i demo not found) ==="
fi

# ── watch --json ─────────────────────────────────────────────────────────────

echo ""; echo "=== watch --json ==="
JSON_OUT=$(mktemp)
"$BINARY" watch --json >"$JSON_OUT" 2>&1 & JSON_PID=$!
sleep 0.5
"$BINARY" dispatch set_dnd "enabled=$([[ "$ORIG_DND" == true ]] && echo false || echo true)" >/dev/null
"$BINARY" dispatch set_dnd "enabled=$ORIG_DND" >/dev/null
sleep 0.8
kill "$JSON_PID" 2>/dev/null || true; wait "$JSON_PID" 2>/dev/null || true
[[ -s "$JSON_OUT" ]] || fail "watch --json got no events"
while IFS= read -r line; do
    echo "$line" | python3 -c "import sys,json;d=json.load(sys.stdin);assert d.get('type')=='event';assert 'name' in d;assert isinstance(d.get('ts'),int)" \
        || fail "invalid JSON event: $line"
done < "$JSON_OUT"
rm -f "$JSON_OUT"
pass "watch --json valid"

kill "$WATCHER_PID" 2>/dev/null || true; wait "$WATCHER_PID" 2>/dev/null || true; WATCHER_PID=""
[[ -s "$WATCH_OUT" ]] || fail "watch received no events at all"

echo ""; echo "=== ALL TESTS PASSED (your shell was never stopped) ==="
