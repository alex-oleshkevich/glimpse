# glimpse-compositor-hyprland

The Hyprland provider of `me.aresa.Glimpse.Compositor`: it implements `glimpse_compositors::Backend`
over Hyprland's two sockets and hands it to `provider::run`. `--probe` exits 0 when Hyprland
answers `version` and 1 otherwise; exit 4 means another provider owns the name.

## Contents

- `main.rs` — CLI parse, logging, `provider::run`
- `cli.rs` — the argument surface
- `hyprland.rs` — `impl Backend`, the wire structs and the dispatch commands
- `hyprland/event.rs` — the event-line decoder and the shared cast and urgency state

## Rules

- **Sockets** live under `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/`: `.socket.sock` takes
  requests (`ok` or an error string; `j/` for JSON), `.socket2.sock` streams `EVENT>>a,b,c`. An
  empty signature is the same as an unset one.
- **A window's `rect` is relative to its own monitor.** `at` is global layout coordinates, so the
  window's own monitor origin (matched by the numeric `monitor` id every client carries) is
  subtracted, never the workspace's. A window with no `at`, `size` or known monitor has no rect.
  Rects are logical pixels and are asked for live through `window_geometries`, never cached.
- **Two dialects.** A config running the Lua manager answers `ok` to `eval return 1` and takes the
  `hl.dsp.*` forms; any other reply selects the legacy dispatchers. A monitor disable sends
  `keyword monitor …,disable` first and retries through `eval` only when Hyprland names the
  non-legacy parser. Enabling sends a plain `reload`.
- **Negative workspace ids are addressed by name**, because a bare negative selector reads as a
  relative move. Stepping uses `e+1` / `e-1`, windows `cyclenext` / `cyclenext prev`, layouts
  `switchxkblayout all next|prev|<index>` — a top-level request, not a dispatch.
- **Events rarely carry enough to rebuild a record**, so most become a `Resync`. `configreloaded`
  resyncs the keyboard, which also re-reads `misc:allow_session_lock_restore`.
- **Casts come from `screencastv2>>STATE,TYPE,NAME`**, TYPE being `monitor`, `window` or `region`.
  A start revives a stopping cast of the same `(TYPE, NAME)` with its id and no event, else opens
  one cast with a provider-local id; its output is `NAME` for monitor and region shares and absent
  for a window share, whose `NAME` is an application's title and is never published. A stop marks
  one cast not already stopping — the same `(TYPE, NAME)`, else the oldest of its TYPE, because a
  window share's `NAME` is a title that changes — as stopping for a 2 s grace, and with no such
  cast changes nothing; expiry removes it and emits one `CastsChanged`. `session` and `pw_node`
  are always absent: no command stops a Hyprland cast. The v1 `screencast>>` event is ignored, so
  Hyprland sending both never counts a share twice.
- **Urgency is remembered here** and cleared when the window is focused or closed, because the
  client JSON carries no urgent flag.
- **Unsupported**: `power_off_monitors`, `screenshot_window` and `stop_screencast` answer
  `Unsupported` without touching the socket; the capabilities are `rename-workspace`,
  `output-power` and `window-geometry`.
- **Text from the compositor is capped and stripped** through `glimpse_compositors::sanitize`; an
  empty title, class or description is absent, not `""`.
- **Concurrent requests** meet a tiny socket backlog and `connect` can answer `EAGAIN`, so it
  retries with a short backoff.

## Tests

The suite runs against `FakeHyprland` in `hyprland/testing.rs`: real Unix sockets in a temporary
directory that reply to scripted commands and replay scripted event lines. A backend takes its
directory through `Hyprland::at`, so no test reads the process environment. `spawn_live` paces the
lines and keeps the event socket open, as Hyprland does; it ticks a paused clock every 1 ms, because
tokio's auto-advance jumps to the next timer on every socket handoff.
