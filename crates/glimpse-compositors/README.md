# glimpse-compositors

Compositor state and control for **niri** and **Hyprland**, behind one model.

A caller asks `detect_compositor()` once, gets a `Compositor`, and from there reads a `Snapshot`,
follows an event stream, and drives keyboard layouts, workspaces, windows and outputs. Which
compositor is underneath shows up in exactly one place a caller has to care about: `Capabilities`.

```rust
let compositor = detect_compositor();
let snapshot = compositor.snapshot().await?;
let mut events = compositor.events().await?;
compositor.focus_workspace(WorkspaceTarget::Next).await?;
```

## This crate touches no Wayland object

Both backends are Unix sockets carrying text — niri's is newline-delimited JSON, Hyprland's is a
command language and a line-oriented event feed. No `wl_` object appears anywhere here, so the
"anything touching a `wl_` object lives in `glimpsed/src/wayland/`" rule does not apply. That is the
first question a reader has, which is why it is the first section.

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

- **Nothing here retries or reconnects.** If a compositor socket closes, the compositor is gone and
  the session with it. The event stream ends and the caller degrades. Retrying a decision the
  backend already made is the thing this codebase does not do.
- **Text from other applications is capped at the boundary, in `model.rs`.** Window titles and app
  ids are unbounded and attacker-controlled — in practice they already carry bidi overrides that
  would reorder whatever a panel draws beside them. `title` and `app_id` are truncated on a char
  boundary and stripped of control characters and bidi overrides during deserialization, so no
  caller can forget to do it. Ellipsizing and markup escaping remain the UI's job.
- **`WindowId` and `WorkspaceId` are opaque and scoped to one compositor run.** Niri assigns a
  counter; Hyprland hands out the window's address. Never render one, never persist one across
  sessions.
- **The crate reports; the caller diffs.** No event is synthesized by comparing against cached
  state. A service holding the model and a `Publisher` that drops an unchanged value already does
  that job, and doing it twice is how the previous implementation got to 3329 lines.
- **`Compositor::Unsupported` is a value, not an error.** `detect_compositor()` cannot fail, so a
  daemon under GNOME degrades and keeps running instead of refusing to start.

## The layout-code table is data, not code

`layout_code()` turns whatever a compositor calls a layout into the two letters a panel shows.
The language-name half of that mapping lives in `data/language-codes.json`, a flat
`{"language": "CODE"}` object, and is read from the first of these that exists:

1. `~/.config/glimpse/language-codes.json` — the user's copy, located via `glimpse_config::user_dir()`
2. `/usr/share/glimpse/language-codes.json` — the installed copy
3. the same file compiled in with `include_str!`, so an uninstalled build still names layouts

A file **replaces** the table rather than extending it, keys match case-insensitively, and a file
that will not parse is logged and skipped rather than left to blank out every layout. Nothing here
covers the two fallbacks that are genuinely rules and not data: a name with no spaces is already an
xkb code and is uppercased whole (`de_ch` → `DE_CH`), and anything else is cut to two letters.

**The read is blocking, once, behind a `OnceLock`.** That is deliberate and authorized — it happens
a single time per process, on the first layout that needs naming, and making it async would push a
`.await` through every caller to save one file open. Do not "fix" it.

## Capabilities carries only differences a caller acts upon

`Capabilities { floating, workspace_reorder }`. Everything else a compositor "supports" is answered
by whether it is `Unsupported`, and an unsupported daemon publishes no compositor topics at all, so
a panel renders nothing without needing to ask. Flags that hold the same value on every backend are
documentation charging runtime rent; this paragraph is cheaper.

`workspace_reorder` is false under Hyprland, where a workspace's index *is* its identity and there
is no position to change. A popover renders `Move up` / `Move down` disabled with that reason rather
than omitting the rows, so the two compositors present the same list and only one of them acts.

## Unsupported and Unavailable are different failures

`Unsupported` is "there is no compositor to ask" — the whole session is something else, and every
operation answers it so the daemon degrades instead of refusing to start. `Unavailable` is "a
compositor is running and cannot do this", which is one operation, is knowable in advance through
`Capabilities`, and is what a disabled row in a popover means. Collapsing them would make a missing
feature indistinguishable from a dead session.

## Every action names its subject

The addons — `move_workspace_to_output`, `reorder_workspace`, `move_window_to_workspace`,
`close_window`, `focus_output` — all take an explicit id. That is not incidental. niri's
`MoveWorkspaceUp`/`Down` take no argument at all and move *the focused* workspace, so driving a
popover row with them would mean focus, move, restore focus: three round trips and a visible flicker
to reorder a workspace the user is not standing in. `MoveWorkspaceToIndex { index, reference }` is
what a row headed `Move up` actually means, which is why reordering is an index and not a direction.

`MoveWindowToWorkspace` is the one that bites in the other direction: its `focus` argument defaults
to `true`, so an unset one drags the user after the window they just sent away. It is passed `false`
explicitly, and Hyprland's counterpart is `movetoworkspacesilent` for the same reason.

A relative workspace — `Next`, `Prev` — is not a reference niri accepts for a window move, so that
combination answers `Unavailable`. Hyprland's selector grammar does accept `+1`/`-1`, and it is left
supported there: the backend is right when the two disagree.

## Where the two compositors differ

Every row below moved a decision in the public API. They are the reason both backends were built at
once rather than niri first and Hyprland later.

| | niri | Hyprland |
| --- | --- | --- |
| Sockets | one, `$NIRI_SOCKET` | two, under `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/` |
| Requests | JSON, `{"Ok":…}` / `{"Err":…}` | text commands, `ok` or an error string; `j/` for JSON |
| Events | JSON lines | `EVENT>>a,b,c`, ambiguous when a field contains a comma |
| Window id | a counter | the window's address |
| Outputs reply | a **map** keyed by connector | an **array** |
| Output disabled | `current_mode: null` — there is no `enabled` field | `"disabled": true` |
| Refresh rate | integer mHz | float Hz |
| Output geometry | `logical` is the size after scaling | the mode's pixels — divided by `scale` here so both mean the same thing |
| Output transform | a name (`"Normal"`) | an integer (`0`) |
| Keyboard layouts | `{names, current_idx}` in one call | codes from `j/getoption input:kb_layout`, the active display name from `j/devices`, matched back |
| Event granularity | per-field events | mostly `Resync` — the payload is rarely enough to rebuild a record |
| Monitor toggle | stateless | `keyword monitor <name>,disable` **discards the mode**, so it is cached to restore |
| Concurrent requests | accepted freely | the control socket's backlog is tiny and `connect` answers `EAGAIN` |

**Hyprland's control socket refuses concurrent connections.** `snapshot()` fires its five `j/`
requests through `try_join!`, each opening its own connection, and that alone sits near the limit;
two snapshots at once return `WouldBlock` from `connect` rather than queuing. `connect` retries with
a short backoff, which also covers contention from `hyprctl` or a second client — something a local
lock could not. Measured against Hyprland by running three live tests in parallel, where every one
of them failed on the connect rather than on anything it was testing.

`KeyboardLayouts` carries `names` *and* `codes` because the two compositors supply opposite halves:
niri gives descriptions ("Polish"), Hyprland gives xkb codes ("pl"). Both are filled on both, so a
panel reading `names` and a config file matching `codes` each get what they expect.

`Logical` carries no `transform`. The two compositors describe rotation in incompatible
vocabularies, and a field meaning `"Normal"` under one and `"0"` under the other is worse than an
absent one — a caller cannot use it without knowing which compositor it is talking to, which is the
thing this crate exists to hide. It comes back as a shared enum in the change that needs rotation.

`Resync` is the answer to an event that says something changed without saying what. Hyprland emits
these constantly, niri rarely — including for its own missing output event, which is inferred from
the set of monitors the workspace list mentions shifting.

## Tests

The headless suite runs against `FakeNiri` and `FakeHyprland`, which bind real Unix sockets in a
temporary directory and replay scripted lines. No compositor, no bus, nothing in
`$XDG_RUNTIME_DIR`. Both backends take their socket path through `Niri::at` / `Hyprland::at`, so no
test touches process environment.

`tests/live.rs` holds two `#[ignore]` tests that run against the real session under
`just test-compositor`: a snapshot, and a layout switch round-tripping back through the event
stream. They are the only check that the commands we send are ones the compositor still accepts.
