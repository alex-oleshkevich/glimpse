#!/usr/bin/env bash
# Isolated e2e for the `glimpse-shell applets` group (ls / new / dev).
#
# Tier 1 (always, fast, no compositor): ls + new (every lang/type + errors)
#   and dev's arg/error paths + the build-failure error-surface, driven by
#   piping the exec protocol on stdin (we play "the shell").
#
# Tier 2 (opt-in: APPLETS_E2E_NIRI=1, needs `niri`): a real glimpse-shell in a
#   nested, isolated niri (own Wayland socket) with GLIMPSE_IPC_DIR sandboxing
#   the IPC socket away from any running shell. Drives `applets dev` per
#   language and asserts the new exec.* events via `watch`.
#
# Fully sandboxed (temp dir + XDG_CONFIG_HOME + GLIMPSE_IPC_DIR); never touches
# a running shell and never `pkill`s niri (only kills PIDs it started).
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO"

BIN="$(cargo build -p glimpse-shell --message-format=json 2>/dev/null \
    | python3 -c "import sys,json; [print(o['executable']) for l in sys.stdin for o in [json.loads(l)] if o.get('reason')=='compiler-artifact' and o.get('target',{}).get('name')=='glimpse-shell' and o.get('executable')]" \
    | tail -1)"
[[ -z "$BIN" ]] && { echo "ERROR: could not locate glimpse-shell binary" >&2; exit 1; }
echo "binary: $BIN"

ROOT="$(mktemp -d)"
export XDG_CONFIG_HOME="$ROOT/cfg"
export GLIMPSE_APPLET_TEMPLATES_DIR="$REPO/applet-templates"
export GLIMPSE_IPC_DIR="$ROOT/ipc"
# Distinct GTK single-instance id so a Tier-2 nested shell never re-activates
# (and thus never disturbs) a running glimpse-shell.
export GLIMPSE_SHELL_APP_ID="me.aresa.GlimpseShell.e2e"
USER_APPLETS="$XDG_CONFIG_HOME/glimpse/applets"
NIRI_PID=""; SHELL_PID=""; WATCH_PID=""

FAILED=0
pass() { echo "  PASS: $*"; }
fail() { echo "  FAIL: $*" >&2; FAILED=1; }
expect() { echo "$1" | grep -q -- "$2" || fail "expected '$2' in: $1"; }
reject() { echo "$1" | grep -q -- "$2" && fail "did NOT expect '$2' in: $1" || true; }

cleanup() {
    echo "--- cleanup ---"
    [[ -n "$WATCH_PID" ]] && kill "$WATCH_PID" 2>/dev/null || true
    [[ -n "$SHELL_PID" ]] && kill "$SHELL_PID" 2>/dev/null || true
    [[ -n "$NIRI_PID"  ]] && { kill -TERM "$NIRI_PID" 2>/dev/null; wait "$NIRI_PID" 2>/dev/null || true; }
    rm -rf "$ROOT"
}
trap cleanup EXIT

# ── Tier 1: ls / new / dev (no compositor) ────────────────────────────────────

echo ""; echo "=== help ==="
"$BIN" applets --help | grep -q "SUBCOMMANDS" || fail "applets --help missing SUBCOMMANDS"
"$BIN" applets new --help | grep -q "glimpse-shell-applets-new" || fail "new --help header"
"$BIN" applets dev --help | grep -q "glimpse-shell-applets-dev" || fail "dev --help header"
pass "help text"

echo ""; echo "=== applets ls (empty) ==="
expect "$("$BIN" applets ls)" "no applets found"
expect "$("$BIN" applets ls --json)" "\[\]"
expect "$("$BIN" applets bogus 2>&1 || true)" "unknown applets subcommand"
pass "ls empty + bad subcommand"

echo ""; echo "=== applets new: exec, every language ==="
WORK="$ROOT/work"; mkdir -p "$WORK"
for L in rust python typescript go; do
    "$BIN" applets new "ex-$L" --lang "$L" --type exec --dir "$WORK" >/dev/null
    expect "$(cat "$WORK/ex-$L/applet.toml")" 'type = "exec"'
    expect "$(cat "$WORK/ex-$L/applet.toml")" "id   = \"ex-$L\""
    [[ -f "$WORK/ex-$L/applet.toml" ]] && pass "new exec $L" || fail "new exec $L missing toml"
done
# Whole-tree copy: python ships pyproject.toml; __NAME_PY__ hyphen->underscore.
"$BIN" applets new "my-counter" --lang python --type exec --dir "$WORK" >/dev/null
expect "$(cat "$WORK/my-counter/pyproject.toml")" 'name = "my-counter"'
expect "$(cat "$WORK/my-counter/pyproject.toml")" 'module-name = "my_counter"'
! grep -q '__NAME' "$WORK/my-counter/main.py" || fail "unrendered placeholder in main.py"
pass "new render (__NAME__/__NAME_PY__) + whole-tree copy"

echo ""; echo "=== applets new: command type ==="
"$BIN" applets new cmd1 --type command --dir "$WORK" >/dev/null
[[ "$(find "$WORK/cmd1" -type f -printf '%P\n')" == "applet.toml" ]] && pass "command = applet.toml only" || fail "command extra files"
expect "$(cat "$WORK/cmd1/applet.toml")" 'type = "command"'

echo ""; echo "=== applets new: error paths ==="
expect "$("$BIN" applets new 2>&1 || true)" "name is required"
expect "$("$BIN" applets new 'bad name' 2>&1 || true)" "characters other than"
expect "$("$BIN" applets new x --lang cobol 2>&1 || true)" "lang must be"
expect "$("$BIN" applets new x --type widget 2>&1 || true)" "type must be"
expect "$("$BIN" applets new x y 2>&1 || true)" "extra argument"
expect "$(GLIMPSE_APPLET_TEMPLATES_DIR=/nope "$BIN" applets new x 2>&1 || true)" "templates not found"
pass "new errors"

echo ""; echo "=== applets ls: user + dev provenance ==="
mkdir -p "$USER_APPLETS"
# Discovery keys normal applets by their `id`; dev (*.dev.toml) by filename base.
printf 'id = "userone"\ntype = "command"\n[command]\nlabel = "u"\non_click = []\n' \
    > "$USER_APPLETS/userone.toml"
printf 'id = "devone"\ntype = "exec"\n[exec]\ncommand = ["/bin/true"]\n' \
    > "$USER_APPLETS/devone.dev.toml"
LS="$("$BIN" applets ls)"
echo "$LS" | grep -qE "^userone[[:space:]].*[[:space:]]user$" && pass "ls shows user applet" || fail "ls user: $LS"
echo "$LS" | grep -qE "^devone[[:space:]].*[[:space:]]dev$"   && pass "ls shows dev applet"  || fail "ls dev: $LS"
"$BIN" applets ls --json >"$ROOT/ls.json" 2>/dev/null || true
python3 -c "import json;d=json.load(open('$ROOT/ls.json'));assert any(a['source']=='user' for a in d) and any(a['source']=='dev' for a in d)" \
    && pass "ls --json provenance" || fail "ls --json provenance: $(cat "$ROOT/ls.json")"
rm -f "$USER_APPLETS"/*.toml

echo ""; echo "=== applets dev: arg / error paths ==="
expect "$("$BIN" applets dev "$WORK/nonexistent" 2>&1 || true)" "resolve project path"
mkdir -p "$ROOT/noapplet"
expect "$(cd "$ROOT/noapplet" && "$BIN" applets dev . 2>&1 || true)" "applet.toml"
pass "dev arg/errors"

echo ""; echo "=== applets dev: build-failure error-surface (pipe-driven) ==="
"$BIN" applets new rusterr --lang rust --type exec --dir "$WORK" >/dev/null
echo 'this is not valid toml @@@' >> "$WORK/rusterr/Cargo.toml"   # cargo fails at manifest parse — fast, no dep build
OUT="$ROOT/dev.out"
( printf 'init {"instance":"rusterr","options":{}}\nevent {"id":"popover","type":"open","source":"popover"}\n'; sleep 5 ) \
    | "$BIN" applets dev "$WORK/rusterr" >"$OUT" 2>/dev/null &
DEVPID=$!; sleep 6; kill "$DEVPID" 2>/dev/null || true; wait "$DEVPID" 2>/dev/null || true
grep -q '^status .*dialog-error-symbolic' "$OUT" && pass "dev emits error status frame" || fail "no error status: $(cat "$OUT")"
grep -q '^popover .*Applet build failed' "$OUT" && pass "dev emits error popover on open" || fail "no error popover"
grep -q '"selectable":true' "$OUT" && pass "error popover has selectable detail" || fail "popover detail missing"

echo ""; echo "=== applets link / unlink (isolated) ==="
"$BIN" applets link --help | grep -q "glimpse-shell-applets-link" || fail "link --help header"
"$BIN" applets unlink --help | grep -q "glimpse-shell-applets-unlink" || fail "unlink --help header"
"$BIN" applets new linkme --type command --dir "$WORK" >/dev/null
expect "$("$BIN" applets link "$WORK/linkme")" "linked (command)"
[[ -L "$USER_APPLETS/linkme.toml" ]] && pass "link created symlink" || fail "link no symlink"
expect "$("$BIN" applets link "$WORK/linkme")" "already linked"
echo "$("$BIN" applets ls)" | grep -qE "^linkme[[:space:]].*[[:space:]]user$" \
    && pass "linked applet shows as user" || fail "linked not in ls"
rm -rf "$WORK/linkme"
expect "$("$BIN" applets unlink linkme)" "unlinked:"
[[ ! -e "$USER_APPLETS/linkme.toml" ]] && pass "unlink by bare id (project gone)" || fail "unlink left file"
expect "$("$BIN" applets unlink linkme)" "is not linked"
# Broken applet: declared exec but no [exec] table — link must refuse.
mkdir -p "$ROOT/bork"; printf 'id = "bork"\ntype = "exec"\n' > "$ROOT/bork/applet.toml"
expect "$("$BIN" applets link "$ROOT/bork" 2>&1 || true)" "but has no"
# Non-symlink conflict needs --force.
mkdir -p "$USER_APPLETS"; : > "$USER_APPLETS/conf.toml"
"$BIN" applets new conf --type command --dir "$WORK" >/dev/null
expect "$("$BIN" applets link "$WORK/conf" 2>&1 || true)" "pass --force"
expect "$("$BIN" applets link --force "$WORK/conf")" "linked (command)"
[[ -L "$USER_APPLETS/conf.toml" ]] && pass "--force replaced regular file" || fail "--force failed"
rm -f "$USER_APPLETS"/*.toml

echo ""; echo "=== applets doctor (isolated) ==="
"$BIN" applets doctor --help | grep -q "glimpse-shell-applets-doctor" || fail "doctor --help header"
DOC="$("$BIN" applets doctor)"; DOC_RC=$?
expect "$DOC" "Glimpse applet environment check"
expect "$DOC" "Summary:"
expect "$DOC" "applet-templates"        # GLIMPSE_APPLET_TEMPLATES_DIR is set → present
[[ $DOC_RC -eq 0 ]] && pass "doctor runs, exits 0 without --strict" || fail "doctor rc=$DOC_RC"
RUSTONLY="$("$BIN" applets doctor --lang rust)"
expect "$RUSTONLY" "cargo"
reject "$RUSTONLY" "go version"
pass "doctor --lang filters to one toolchain"
expect "$("$BIN" applets doctor --lang cobol 2>&1 || true)" "unknown language"
("$BIN" applets doctor --lang rust --strict >/dev/null) && pass "doctor --strict ok when toolchain present" \
    || fail "doctor --strict unexpectedly failed for rust"

# ── Tier 2: nested-niri per-language healthy dev (opt-in) ──────────────────────

if [[ "${APPLETS_E2E_NIRI:-0}" == "1" ]] && command -v niri >/dev/null; then
    echo ""; echo "=== Tier 2: nested niri + per-language healthy dev ==="
    : > "$ROOT/niri.kdl"
    mkdir -p "$XDG_CONFIG_HOME/glimpse"
    cat > "$XDG_CONFIG_HOME/glimpse/config.toml" <<'TOML'
[[panels]]
position = "top"
left = ["__dev__"]
TOML
    niri -c "$ROOT/niri.kdl" -- "$BIN" >"$ROOT/shell.log" 2>&1 &
    NIRI_PID=$!
    for i in $(seq 1 100); do [[ -S "$GLIMPSE_IPC_DIR/ipc.sock" ]] && break; sleep 0.2; done
    if [[ ! -S "$GLIMPSE_IPC_DIR/ipc.sock" ]]; then
        fail "nested-niri shell never bound IPC socket"; sed 's/\x1b\[[0-9;]*m//g' "$ROOT/shell.log" | tail -20 >&2
    else
        pass "nested-niri glimpse-shell up (isolated socket)"
        "$BIN" watch 'exec.*' >"$ROOT/exec.events" 2>/dev/null & WATCH_PID=$!
        sleep 0.5
        for L in python rust go typescript; do
            tool=$(case $L in python) echo uv;; rust) echo cargo;; go) echo go;; typescript) echo npx;; esac)
            command -v "$tool" >/dev/null || { echo "  SKIP $L (no $tool)"; continue; }
            proj="$WORK/dev-$L"
            "$BIN" applets new "dev$L" --lang "$L" --type exec --dir "$WORK" >/dev/null
            mv "$WORK/dev$L" "$proj" 2>/dev/null || true
            id="dev$L"
            # Deploy the dev config directly (what `applets dev` writes; the
            # is_terminal()-gated standalone path can't run under a harness).
            python3 - "$proj" "$BIN" "$USER_APPLETS/$id.dev.toml" "$id" <<'PY'
import sys,tomllib,pathlib
proj,binp,dest,aid=sys.argv[1:5]
src=pathlib.Path(proj)/"applet.toml"
data=tomllib.loads(src.read_text())
data.setdefault("exec",{})
data["exec"]["command"]=[binp,"applets","dev",proj]
data["exec"]["work_dir"]=proj
def dump(v):
    if isinstance(v,list): return "["+", ".join(dump(x) for x in v)+"]"
    if isinstance(v,str): return '"'+v.replace('\\','\\\\').replace('"','\\"')+'"'
    return str(v)
lines=[f'id = "{data["id"]}"','type = "exec"','[exec]']
for k,val in data["exec"].items(): lines.append(f'{k} = {dump(val)}')
pathlib.Path(dest).write_text("\n".join(lines)+"\n")
PY
            BEFORE=$(wc -l < "$ROOT/exec.events" 2>/dev/null || echo 0)
            deadline=$((SECONDS+300))
            ok=0
            while [[ $SECONDS -lt $deadline ]]; do
                if tail -n +"$((BEFORE+1))" "$ROOT/exec.events" 2>/dev/null \
                   | grep -q "exec.applet_status .*name=$id"; then ok=1; break; fi
                sleep 1
            done
            line=$(tail -n +"$((BEFORE+1))" "$ROOT/exec.events" | grep "name=$id" | tail -1 || true)
            if [[ $ok == 1 ]] && ! echo "$line" | grep -q "id=glimpse-dev-error"; then
                pass "dev[$L]: built + ran, healthy exec.applet_status ($line)"
            else
                fail "dev[$L]: no healthy status (last: ${line:-none}); shell.log: $(grep -ai "$id\|build" "$ROOT/shell.log" | tail -3)"
            fi
            rm -f "$USER_APPLETS/$id.dev.toml"
        done
    fi
else
    echo ""; echo "(Tier 2 skipped — set APPLETS_E2E_NIRI=1 and install niri to run per-language healthy dev)"
fi

echo ""
[[ $FAILED -eq 0 ]] && echo "=== ALL APPLETS E2E PASSED ===" || { echo "=== SOME APPLETS E2E FAILED ==="; exit 1; }
