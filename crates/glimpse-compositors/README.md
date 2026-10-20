# glimpse-compositors

Compositor state and control for **niri** and **Hyprland**, behind one model.

A caller asks `detect_compositor()` once, gets a `Compositor`, and from there reads a `Snapshot`,
follows an event stream, and drives keyboard layouts, workspaces, windows and outputs. Which
compositor is underneath shows up in exactly one place a caller has to care about: `Capabilities`.

This crate touches no Wayland object: both backends are Unix sockets carrying text, so the
"anything touching a `wl_` object lives in the owning UI crate" rule does not apply here.

## Contents

| File | Holds |
| --- | --- |
| `lib.rs` | `detect_compositor()`, `Compositor`, `Capabilities` |
| `model.rs` | `Snapshot` and everything in it, the id and target types, the text caps |
| `event.rs` | `Event`, `Resync`, and the line loop both backends' event streams share |
| `error.rs` | `CompositorError` |
| `keyboard.rs` | `layout_code()` — the short badge a panel renders, and the table behind it |
| `niri/` | the JSON protocol, action shapes, event decoding |
| `hyprland/` | the control socket, dispatch strings, event decoding, the monitor-config cache |

## Rules

- **Nothing here retries or reconnects.** If a compositor socket closes, the session is gone and the
  event stream ends.
- **Text from other applications is capped in `model.rs`.** `title` and `app_id` are truncated on a
  char boundary and stripped of control characters and bidi overrides during deserialization, so no
  caller can forget to; ellipsizing and markup escaping stay the UI's job.
- **`WindowId` and `WorkspaceId` are opaque and scoped to one compositor run** — a counter under
  niri, the window's address under Hyprland. Never render or persist one across sessions.
- **The crate reports; the caller diffs.** No event is synthesized by comparing against cached
  state — a service holding the model already does that job.
- **`Compositor::Unsupported` is a value, not an error**, so a daemon under GNOME degrades and keeps
  running instead of refusing to start.
- **A paused cast is still a live session.** `Cast::active` goes `false` when the consumer switches
  scenes, but the portal grant has not ended and niri keeps the cast in `Snapshot.casts` until it
  actually stops — do not read `active` as "is anyone capturing right now".
- **`StopCast` only ends a `CastKind::PipeWire` session** — wlr-screencopy casts cannot be stopped
  through niri's IPC, and under Hyprland every cast is synthetic with `session_id: None`.
- **`Capabilities` carries only differences a caller acts on** — a flag holding the same value on
  every backend is documentation charging runtime rent. `blank_screens` is false under Hyprland,
  whose `dpms off` does not wake on input, so the blank row is omitted there while `output_power`
  stays true on both.
- **A caller that needs only the keyboard layouts calls `keyboard_layouts()`**, one request per
  backend, rather than a whole `snapshot()` that fails when any unrelated part does.
- **`Unsupported` and `Unavailable` are different failures.** `Unsupported` means there is no
  compositor to ask, so every operation answers it and the daemon degrades rather than refusing to
  start. `Unavailable` means a compositor is running and cannot do this one thing, is knowable in
  advance through `Capabilities`, and is what a disabled popover row means.
- **Every action names its subject explicitly.** A rename or a window focus carries the id it acts
  on rather than relying on what is focused; only `Next`/`Prev` targets lean on focus, by design.

## Window geometry and screenshotting a window

- **`Window.rect` is output-relative logical pixels, on both backends.** glimpse-shot crops a
  per-output frozen frame, so a position in any other space would crop the wrong pixels. niri's
  `tile_pos_in_workspace_view` already is relative to the output; Hyprland's `at` is global layout
  coordinates, so `WireClient::into_model` subtracts the window's own monitor's `x`/`y` (matched by
  the numeric `monitor` id every client carries, against `WireMonitor.id`), never the workspace.
- **`Window.rect` is `None` for a tiled window, on purpose.** niri reports no position for a
  window in the scrolling layout — `layout.tile_pos_in_workspace_view` is `null` — and this crate
  does not invent one by cropping to a guess. `Window.size` is set for both a tiled and a floating
  window; only the position is withheld.
- **`Window.rect`'s position is `tile_pos_in_workspace_view + window_offset_in_tile`.**
- **`Window.output`, `.size` and `.rect` are current only right after a snapshot.**
  `fill_window_outputs` runs inside `snapshot()`, so a window arriving through niri's
  `WindowOpenedOrChanged` or `WindowsChanged` event carries `output: None` until the next snapshot;
  `WindowLayoutsChanged` carries no window at all, only a `layout_order` delta, so it also leaves a
  cached window's `size` and `rect` exactly as they were. `Window.output` is filled from
  `Workspace.output`, never decoded from a window's own wire payload — niri's window JSON has no
  output field at all, and while Hyprland's client JSON does carry a `monitor` id, that id is used
  only to make `rect` output-relative (above), never to fill `output` itself.
- **`screenshot_window` waits for the file, not just the reply.** niri's IPC reply to
  `ScreenshotWindow` arrives before the PNG exists on disk — measured: absent at ~20 ms, present by
  500 ms. The inotify watch on the target's parent directory is armed for `IN_CLOSE_WRITE`
  *before* the action is sent, and the call returns only once that event names the target file, or
  after a 2 s deadline (`CompositorError::NoFile`). The wait runs in `spawn_blocking`, never on the
  async runtime. The path is validated — absolute, valid UTF-8, carrying a file name — before
  anything is armed; a path that fails this is `CompositorError::InvalidPath` and a failure to open
  or watch the directory itself is `CompositorError::Watch`.
- **`ScreenshotWindow` always replaces the clipboard.** Measured: there is no flag to suppress it.
  This crate does not work around that here — restoring the clipboard afterward is the caller's job.
- **Hyprland has no equivalent action.** `screenshot_window` on `Hyprland` returns
  `CompositorError::Unavailable` — a compositor is running and simply cannot do this, the same
  reason `stop_screencast` answers the same way there.

## Where the two compositors differ

| | niri | Hyprland |
| --- | --- | --- |
| Sockets | one, `$NIRI_SOCKET` | two, under `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/` |
| Requests | JSON, `{"Ok":…}` / `{"Err":…}` | text commands, `ok` or an error string; `j/` for JSON |
| Events | JSON lines | `EVENT>>a,b,c`, ambiguous when a field contains a comma |
| Outputs reply | a **map** keyed by connector | an **array** |
| Output disabled | `current_mode: null`, no `enabled` field | `"disabled": true` |
| Refresh rate | integer mHz | float Hz |
| Output geometry / transform | `logical` is size after scaling; transform is a name (`"Normal"`) | pixels divided by `scale` here to match; transform is an integer (`0`) — `Logical` carries no shared `transform` field until something needs rotation |
| Keyboard layouts | `{names, current_idx}` gives descriptions ("Polish") | codes and the active index from `j/devices` (`layout`, `active_layout_index`); `KeyboardLayouts` fills both `names` and `codes` on both backends |
| Event granularity | per-field events | mostly `Resync` — the payload is rarely enough to rebuild a record; niri emits one too, for its own missing output event |
| Monitor toggle | stateless | disabling sends the monitor's `disable` rule (a Lua config refuses `keyword` and takes the `eval` form); enabling sends a plain `reload`, so Hyprland re-applies its own monitor rules and also drops other runtime monitor and keyword changes |
| Concurrent requests | accepted freely | backlog is tiny and `connect` answers `EAGAIN`; `snapshot()`'s five `j/` requests retry with a short backoff |

## Tests

The headless suite runs against `FakeNiri` and `FakeHyprland`, real Unix sockets in a temporary
directory replaying scripted lines; both take their socket path through `Niri::at` / `Hyprland::at`,
so no test touches process environment. `tests/live.rs` holds `#[ignore]` tests that run against
the real session under `just test-compositor` — the only check that the commands sent are ones the
compositor still accepts.
