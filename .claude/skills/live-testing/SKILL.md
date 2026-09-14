---
name: live-testing
description: Live-testing glimpse on a compositor without touching the user's session bar or ~/.config/glimpse. Use when the user asks to live-test, try it on the bar, screenshot the panel, exercise an applet end-to-end, or open a second panel. Headless tests belong to the testing skill. just preview of a blueprint is not this.
---

# live-testing

A compositor run is not `just test`. It talks to the real niri/Hyprland and must not replace the
session daemon, the session panel, or `~/.config/glimpse`. Scratch config, own socket, own panel
process, grim *that* bar, restore compositor state, kill only the test pids.

Headless tiers, mutation checks, and "never `~/.config`" live in the `testing` skill. Widget look
without a daemon is `just preview` in AGENTS.md.

## Isolated stack

```bash
S=$XDG_RUNTIME_DIR/glimpse-<feature>-live
SOCK=$XDG_RUNTIME_DIR/glimpse-<feature>.sock   # SUN_LEN ~108; not a deep scratch path
mkdir -p "$S/cfg" /tmp/glimpse-<feature>-logs
```

Config is **one** `[[panels]]`, only the applet under test. `position = "bottom"` so it does not
stack under the session top bar. Logs go **outside** `$S/cfg`'s parent: `--config` watches that
parent, and a log written there reloads the document at `DEBOUNCE`.

```toml
[[panels]]
size = 40
position = "bottom"
left = ["<applet>"]
center = []
right = []
```

A table that names only `left` still showed this repo's default center and right applets on the bar (clock, weather, keyboard, …). Set `center = []` and `right = []`.


```bash
./target/debug/glimpsed --config "$S/cfg/config.toml" --socket "$SOCK" \
  >/tmp/glimpse-<feature>-logs/daemon.log 2>&1 &
GLIMPSE_PANEL_APP_ID=me.aresa.GlimpsePanel.<Feature>Test \
  ./target/debug/glimpse-panel --config "$S/cfg/config.toml" --socket "$SOCK" \
  >/tmp/glimpse-<feature>-logs/panel.log 2>&1 &
./target/debug/glimpsectl --socket "$SOCK" get <topic>
```

Do not `cargo` two crates in parallel (file lock). Prefer already-built `target/debug/*`.

## Second panel

`glimpse-panel` is unique on the session bus (`me.aresa.GlimpsePanel`). A second process with that
id **hands off and exits 0** — log shows `loading configuration` and nothing else. Set
`GLIMPSE_PANEL_APP_ID` to a distinct id. Do not kill the session `glimpsed` / `glimpse-panel` to
make room.

## Gamma control needs the session compositor

**A nested niri does not offer `zwlr_gamma_control_manager_v1`.** Measured: the winit backend owns no
real outputs, so `glimpse-sunset` against a nested instance exits 4 — "the compositor does not offer
zwlr_gamma_control_manager_v1". That makes a nested niri the cheapest way to test the *permanent*
failure, and it means the applying path can only be exercised against the session compositor, which
tints the whole display.

Run it there on a **private session bus** — `dbus-daemon --session --print-address=3 --fork` — so the
well-known name and its state never reach the user's bus. Keep it short and stop it with `SIGTERM`:
a clean stop calls `Gamma::reset` and hands the outputs back, and a `SIGKILL` leaves the last ramp
applied. Two private buses against one compositor is also how gamma *contention* is tested without
installing `wlsunset` — the second provider finds every output `failed`, reports
`another gamma client holds the outputs`, and takes over on its next tick once the first releases.

**Kill a test bus by its exact pid.** `pkill -f "dbus-daemon --session"` also matches a session bus
started that way. This machine runs `dbus-broker`, so the user's session survived it; one that does
not would lose the whole session to that command.

## Drive and look

- `glimpsectl --socket "$SOCK"` for topics and commands. The session socket is
  `$XDG_RUNTIME_DIR/glimpse/glimpsed.sock`; using it is testing the user's bar.
- `grim -g "<x>,<y> <w>x<h>"` the **bottom** strip of the output you care about, then read the PNG.
  Confirm the crop is the white/light test bar, not the session top bar.
- A layout/volume/window command hits the **real compositor**. Snapshot before, restore after.
- Pointer clicks use the standalone helper, not the daemon:

  ```bash
  just click output=DP-2 x=1200 y=540 button=left
  ```

  `x` and `y` are logical coordinates relative to the named niri output. Run the same command
  with `--dry-run` first to verify the output lookup and generated `ydotool` coordinates without
  moving the pointer. A real click requires an already-running `ydotoold`; if it is inactive, say
  so and do not start it unasked. The helper cannot query the previous pointer position, so use
  explicit virtual-desktop coordinates when a test must move the pointer back:

  ```bash
  just click output=DP-2 x=1200 y=540 button=left restore_x=640 restore_y=400
  ```

## Stop

Kill only the test pids (the ones whose cmdline has `$SOCK` or the test app id). Leave the session
processes. Restore compositor state before saying the test is done.
