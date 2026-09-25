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
| `event.rs` | `Event` and `Resync` |
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
  every backend is documentation charging runtime rent. `workspace_reorder` is false under
  Hyprland, where a workspace's index *is* its identity, so a popover disables `Move up`/`Move down`
  rather than omitting the rows. `output_power` is kept true on both even though Hyprland's
  wake-on-input after `dpms off` may need an explicit power-on that niri does not.
- **`Unsupported` and `Unavailable` are different failures.** `Unsupported` means there is no
  compositor to ask, so every operation answers it and the daemon degrades rather than refusing to
  start. `Unavailable` means a compositor is running and cannot do this one thing, is knowable in
  advance through `Capabilities`, and is what a disabled popover row means.
- **Every action names its subject explicitly.** niri's `MoveWorkspaceUp`/`Down` take no argument
  and move *the focused* workspace, so a popover row uses `MoveWorkspaceToIndex` instead.
  `MoveWindowToWorkspace`'s `focus` argument defaults to `true` and is passed `false` explicitly, or
  a move drags the user after the window they just sent away. A relative workspace (`Next`/`Prev`)
  is not a reference niri accepts for a window move and answers `Unavailable`; Hyprland's selector
  grammar accepts `+1`/`-1` and is left supported there — the backend is right when they disagree.

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
| Keyboard layouts | `{names, current_idx}` gives descriptions ("Polish") | codes from `j/getoption input:kb_layout` ("pl"), matched to `j/devices`; `KeyboardLayouts` fills both `names` and `codes` on both backends |
| Event granularity | per-field events | mostly `Resync` — the payload is rarely enough to rebuild a record; niri emits one too, for its own missing output event |
| Monitor toggle | stateless | `keyword monitor <name>,disable` **discards the mode**, so it is cached to restore |
| Concurrent requests | accepted freely | backlog is tiny and `connect` answers `EAGAIN`; `snapshot()`'s five `j/` requests retry with a short backoff |

## Tests

The headless suite runs against `FakeNiri` and `FakeHyprland`, real Unix sockets in a temporary
directory replaying scripted lines; both take their socket path through `Niri::at` / `Hyprland::at`,
so no test touches process environment. `tests/live.rs` holds two `#[ignore]` tests that run against
the real session under `just test-compositor` — the only check that the commands sent are ones the
compositor still accepts.
