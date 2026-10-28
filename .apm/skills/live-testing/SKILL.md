---
name: live-testing
description: Live-testing glimpse on a compositor without touching the user's session bar or ~/.config/glimpse. Use when the user asks to live-test, try it on the bar, screenshot the panel, exercise an applet end-to-end, or open a second panel. Headless tests belong to the testing skill. just preview of a blueprint is not this.
---

# live-testing

A compositor run is not `just test`. It talks to the real niri/Hyprland and must not replace the
session daemon, the session panel, or `~/.config/glimpse`. Scratch config, own socket, own panel
process, grim *that* bar, restore compositor state, kill only the test pids.

Headless tiers, mutation checks, and "never `~/.config`" live in the `testing` skill. Widget look
without a daemon is `just preview`, in `docs/preview.md`.

## Isolated stack

Every run gets its own directory, private bus and nested niri. Only a live tester starts a
nested niri, and only one; builders never open a window.

```bash
S=$(mktemp -d -p "$XDG_RUNTIME_DIR" <feature>.XXXX)
mkdir -p "$S/cfg" "$S/home" "$S/logs" "$S/services"
sed 's#<standard_session_servicedirs */>#<servicedir>'"$S"'/services</servicedir>#' \
  /usr/share/dbus-1/session.conf > "$S/bus.conf"
dbus-daemon --config-file="$S/bus.conf" --print-address=1 --print-pid=2 --fork \
  > "$S/bus.addr" 2> "$S/bus.pid"
printf 'export DBUS_SESSION_BUS_ADDRESS=%q\n' "$(cat "$S/bus.addr")" > "$S/env"
ls "$XDG_RUNTIME_DIR" > "$S/before"
niri -c /usr/share/doc/niri/default-config.kdl > "$S/logs/niri.log" 2>&1 &
```

Each tool call is a fresh shell, so append the nested `WAYLAND_DISPLAY` and `NIRI_SOCKET` to
`$S/env` too and start every later command with `. "$S/env" &&`; a command without it reaches the
user's bus and display.

The nested window opens on the session; move it at once with the **session** `niri msg`:
`niri msg --json windows` gives its id (app id `niri`), then `niri msg action move-window-to-monitor --id <id>
eDP-1`. Never DP-2. Diff `$XDG_RUNTIME_DIR` against `$S/before` for the nested `wayland-N` and
`niri.wayland-N.*.sock`; every glimpse command, `grim` and `wtype` from here on runs
with `WAYLAND_DISPLAY=<nested> NIRI_SOCKET=<nested sock>` in the same command and
`HYPRLAND_INSTANCE_SIGNATURE` unset. Without that prefix input lands in the user's terminal.

The private bus has an empty `<servicedir>`, so nothing installed is activated onto the user's
display; put a `.service` there only for a provider the test means to activate.

Config is **one** `[[panels]]`, only the applet under test. `position = "bottom"` so it does not
stack under the session top bar. Logs go **outside** `$S/cfg`: `--config-dir` watches that
directory, and a log written there rereads the configuration on every line. The panel layout is the
`panel.toml` in it.

```toml
[[panels]]
size = 40
position = "bottom"
left = ["<applet>"]
center = []
right = []
```

A table that names only `left` still showed this repo's default center and right applets on the bar (clock, weather, keyboard, …). Set `center = []` and `right = []`.


Start the compositor provider on the private bus before the panel. `$S/env` must name the nested
socket; for Hyprland use `glimpse-compositor-hyprland` with its nested signature and unset
`NIRI_SOCKET` instead. Record the provider pid for cleanup with the other test processes.

```bash
. "$S/env"
env -u HYPRLAND_INSTANCE_SIGNATURE ./target/debug/glimpse-compositor-niri \
  > "$S/logs/compositor.log" 2>&1 &
COMPOSITOR_PID=$!
WAYLAND_DISPLAY=<nested> NIRI_SOCKET=<nested sock> GLIMPSE_PANEL_APP_ID=me.aresa.GlimpsePanel.<Feature>Test \
  XDG_CONFIG_HOME="$S/home" ./target/debug/glimpse-panel --config-dir "$S/cfg" \
  > "$S/logs/panel.log" 2>&1 &
```

Do not `cargo` two crates in parallel (file lock). Prefer already-built `target/debug/*`.

## Second panel

`glimpse-panel` is unique on the session bus (`me.aresa.GlimpsePanel`). A second process with that
id **hands off and exits 0** — log shows `load config path=` and nothing else. Set
`GLIMPSE_PANEL_APP_ID` to a distinct id. Do not kill the session `glimpse-panel` to make room.

`glimpse-notifications` is the same, through `GLIMPSE_NOTIFICATIONS_APP_ID`. This matters when the
thing under test is a *bus name* rather than a window: the GTK handoff fires before either name is
requested, so two default-id processes prove nothing about name ownership. Give each a distinct app
id and the second reaches the request, logs
`cannot serve notifications; exiting` (`name already taken on the bus`) and exits —
`me.aresa.Glimpse.Notifications` and `org.freedesktop.Notifications` both stay with the first.

## Gamma control needs the session compositor

**A nested niri does not offer `zwlr_gamma_control_manager_v1`.** Measured: the winit backend owns no
real outputs, so `glimpse-sunset` against a nested instance exits 4 — "the compositor does not offer
zwlr_gamma_control_manager_v1". That makes a nested niri the cheapest way to test the *permanent*
failure, and it means the applying path needs a compositor with real outputs: the `var/vm` QEMU box,
never the user's session.

Run it there on the private bus above, so the well-known name and its state stay off any other
bus. Keep it short and stop it with `SIGTERM`:
a clean stop calls `Gamma::reset` and hands the outputs back, and a `SIGKILL` leaves the last ramp
applied. Two private buses against one compositor is also how gamma *contention* is tested without
installing `wlsunset` — the second provider finds every output `failed`, reports
`another gamma client holds the outputs`, and takes over on its next tick once the first releases.

**A `--session` private bus activates the installed providers onto the user's display.** It reads
`/usr/share/dbus-1/services`, so the first proxy the test panel builds starts `/usr/bin/glimpse-
notifications`, `glimpse-weather` and `glimpse-idle` with the bus daemon's environment — the
launching shell's `WAYLAND_DISPLAY`, the user's session. Measured: all three came up on `wayland-1`
behind a panel under test in a nested niri. Give the private bus its own `--config-file` with a
`<servicedir>` holding only what the test should activate — which is also how D-Bus activation of a
provider under test is exercised, through a `.service` whose `Exec` sets the nested socket.

**Kill a test bus by its exact pid.** `pkill -f "dbus-daemon --session"` also matches a session bus
started that way. This machine runs `dbus-broker`, so the user's session survived it; one that does
not would lose the whole session to that command.

## Drive and look

- `glimpsectl` reads the three providers over their D-Bus names. On the session bus that is the
  user's own weather and notification daemons, so a test that drives a provider gives it a private
  bus — see the gamma section above.
- `WAYLAND_DISPLAY=<nested> grim "$S/shot.png"` captures the nested compositor only; crop the
  bottom strip with `grim -g` and read the PNG. A tester that cannot read images says so and
  checks pixels with `magick "$S/shot.png" txt:` or the log instead.
- A layout/volume/window command hits the compositor it is pointed at. Point it at the nested one.
- Pointer, key, clipboard and screenshots go through wayland-mcp: `sandbox_start`, move its window to
  eDP-1, `target_select` the sandbox, then `click`/`drag`/`key`/`screenshot` in its logical
  coordinates. gtk-mcp (`list_apps`, `snapshot`, `audit_layout`) reads a debug build's widget tree;
  set `GTK_MCP_DIR=$XDG_RUNTIME_DIR/gtk-mcp` so the broker finds a process started by hand.
- **The wayland-mcp sandbox bus activates installed services onto the host display.** Its
  `dbus-daemon --session` hands its own environment, the session's `WAYLAND_DISPLAY`, to everything
  it activates: a portal call on it started `/usr/bin/glimpse-wallpaper` on the user's session.
  Run glimpse on the private bus above, with `WAYLAND_DISPLAY` set to the sandbox's absolute socket
  path, and use the sandbox only as the compositor.

## Stop

`SIGTERM` the test pids you recorded (glimpse binaries first, then niri), then the bus by the pid
in `$S/bus.pid`, then `rm -rf "$S"`. `pgrep -af "$S"` and `pgrep -af <Feature>Test` show nothing;
the session `glimpse-panel` pid is the one you saw at the start. Never `pkill -x glimpse-*`.
