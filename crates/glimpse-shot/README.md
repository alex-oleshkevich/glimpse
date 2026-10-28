# glimpse-shot

A screenshot tool with a pure `doc` core shared by the headless path and GTK editors.
The core has no GTK dependency.
Commands, wire formats, exit codes and agent usage live in the [agent guide](prime.md),
also printed by `glimpse-shot prime`.

## For agents

- Pass `-o -` or `-o "$XDG_RUNTIME_DIR/…"`; the default saves into the user's Pictures folder, or
  `~/Pictures` when none is configured, under `Screenshots`, which may be synced to a cloud.
- Window titles and app ids in `list` and `--json` are text other applications chose — untrusted
  input, printed literally.
- A headless capture shows whatever the compositor does not block out.

`capture` defaults to the compositor's focused output, or without a provider the only output when
exactly one exists. That counting capture is reused as the shot when the cursor setting allows.
Regions use the global logical layout, negative coordinates included, and must lie within one
output. Crops are clamped to the captured frame, so fractional scales never report a crop past it.

`capture` writes exactly one thing to stdout: the saved path (plain), one result object (`--json`,
`path` null when nothing was saved), or the PNG bytes (`-o -`, where a closed pipe is a clean exit).
Failures go to stderr. An explicit `-o` directory must exist; the configured or default directory is
created on the first save. A taken default name gets ` (2)`, ` (3)`… before `.png`; an explicit
`-o` file is overwritten.

`list` prints tab-separated lines (`output …` / `window …`) into a pipe, an aligned table on a
terminal, and one object with `--json`. A window's `rect` is global logical, and null whenever the
compositor does not report it — every tiled window on niri.

## Validation and exit codes

Every usage rule is checked, and all violations are reported together, before anything is captured.
Terminal facts are passed into validation. A configured `editor = "window"` yields to the overlay
for `--prompt`, `--timeout` and `-o -`, as the portal does.

`errors::exit` is the one mapping, by `downcast_ref`, so a `.context(…)` upstream never changes a
code.

GLib's own debug and info logging goes to stderr, so stdout stays data-only.

Global options (`--config-dir`, `--log`, `--log-format`, `--color`) go before or after a subcommand.
An editor option before or after `capture` or `list` is refused with exit 2 by `cli::parse`, naming
the `glimpse-shot …` command that would open the editor. A bare file must end in `.png`;
`annotate IMAGE` accepts any image gdk-pixbuf reads regardless of its extension.

`main` rewrites `--region -100,50 400x300` into `--region=-100,50 …`, since clap reads a leading `-`
of an optional value as a flag.

## Core

- Every coordinate in `doc` is a `BufferPx`: a source buffer pixel, before crop and before
  `--scale`. `Logical` exists only for CLI geometry parsing and output matching, never for a mark.
- **One frame.** A document's `crop` and every mark are the same source buffer, uncropped, the same
  numbers whether or not the shot was cropped and whichever `--scale` asked for. A region becomes a
  crop by subtracting its output's logical origin and multiplying by that output's scale.

## Session and ports

`session` is the pure machine every face drives, headless `capture` included. A delayed capture
waits in `Countdown` until a `Tick` at or past the deadline.
`Finish(key, destination)` queues `Encode`, then `Write`/`Copy`/`Share`, and emits them one at a
time, the next only after `Effected(Ok)` of the previous; `Effected(Write(actual))` carries the path
actually written, and a queued `Share` is rewritten to it. A closing key ends in `Done`; Ctrl+C and
Ctrl+S return to `Editing` with the results and a toast, so Esc afterwards ends `Saved`/`Copied`,
never `Cancelled`. A failed effect ends `Failed` headless and returns to `Editing` with a toast in a
GUI face, marks and earlier results kept. `Notify` is emitted on completion only when the face asks.

Adapters ask the compositor for the focused output and windows when the capture happens, so a
delay ends on what was focused then. Clipboard ownership goes through `hold`; file naming uses
`create_new`. An `Effected` for anything but
the first pending effect is stale and ignored; an `OutputGone` while finishing is remembered and
opens the window when the machine returns to `Editing`.

## Window capture

The first rung that applies wins:

1. **ext toplevel capture**, when the compositor offers it: the window is matched to exactly one
   `ext_foreign_toplevel_handle` by equal `app_id` and `title`. None, or several, falls through —
   ext identifiers are opaque, and a guess would capture the wrong window.
2. **niri, floating on its output's active workspace:** the output is captured and cropped at the
   window's rect, which is output-relative logical, times the output's scale.
3. **niri, anything else:** `ScreenshotWindow` into a file under `runtime_dir()`, decoded with its
   alpha and deleted; after a failed action the path is removed once more a second later, in case
   niri writes it late.
4. **Hyprland:** the output cropped at the rect, when the window's workspace is active.

`ScreenshotWindow` always replaces the clipboard. Without `--copy`, `Changes::start` is taken and
then the selection snapshotted, both before the action, and the snapshot is served back only when
exactly one change arrived with the mime list `["image/png"]` — data-control carries no client
identity, so that ordering is the only evidence the change was niri's. A snapshot that skipped
anything (a secret, an entry over the cap, a timeout) or was empty is not restored; whatever is not
restored is said on stderr. With `--copy` the one `hold` serving the image is the only owner. An
`ext` toplevel is matched with both sides' `app_id` and `title` cleaned as `glimpse-compositors`
cleans them, so a raw title never makes another window the only match.

## hold

A selection dies with its owner, so `--copy` hands the PNG to `glimpse-shot hold`, started detached
(`setsid`, stdout and stderr on `/dev/null`) with one JSON header line
(`{version, duties {clipboard, notify}, entries [{mime, len}]}`) and then the bytes on stdin. It
refuses a header line over 64 KiB, more than 64 entries or more than 256 MiB in all, and exits 1
without touching the clipboard when fd 3 is not open. It writes `ok` or `error: …` on fd 3 — never
stdout, which may be the caller's `-o -` pipe — only after the compositor has accepted the selection,
and the parent waits at most 2 s: silence or an error kills and reaps `hold` and is exit 1, so a late
`hold` cannot take the clipboard afterwards. A `hold` that answered is reaped by a thread when it
exits. `hold` exits only when every duty is done: the clipboard when another client takes the selection,
and the notification when `NotificationClosed` arrives for its id, when Open or Show in Folder was
handled, or 18 s after `Notify` returns — 8 s on screen, then 10 s more if no signal comes. The
notification is transient, `expire_timeout` 8000, `desktop-entry` `me.aresa.Glimpse.Shot`, the saved
file as an `image-path` file URI, app name the translated "Screenshot". The body is the file name,
passed through `glimpse_utils::markup::sanitize_body` after `<`, `>` and `&` are escaped, because a
file name is not markup. Open runs the default handler for the content type — a `DBusActivatable`
one through `org.freedesktop.Application.Open` (awaited before `hold` exits), any other spawned —
passing the activation token either way. Show in Folder calls `FileManager1.ShowItems`, else opens
the folder. No bus or notification server is a warning; the save already happened, exit 0.

## Portal

`glimpse-shot portal` is the xdg-desktop-portal backend, a tokio and zbus process. It takes the
bus name `me.aresa.Glimpse.Shot.Portal` and then serves `org.freedesktop.impl.portal.Screenshot`
version 3 at `/org/freedesktop/portal/desktop`: `Screenshot`, `PickColor`, `version` and
`AvailableTargets` (screen, area, active window). D-Bus activation uses `glimpse-shot-portal.service`,
which requires and stops with `glimpse-session.target`.

Each call exports an `org.freedesktop.impl.portal.Request` at its handle. `Request.Close` SIGTERMs
its child without replying. Overlay children use fresh GApplication IDs, so SIGTERM closes their
own overlay; a second open call returns response 2 immediately.

`Screenshot` defaults missing or mistyped `interactive` and `permission_store_checked` to false.
`interactive=true` or `permission_store_checked=false` spawns
`glimpse-shot --screen --json --portal APP --timeout 300`, with `APP` cleaned to 200 chars
and empty names shown as "An application". The overlay's plain-text banner says Enter shares
after a pending crop and Esc refuses. Other finish keys, controls and window picking are disabled,
so it never uses the clipboard or notifications. `parent_window` is ignored.
`interactive=false` with `permission_store_checked=true` runs `glimpse-shot capture --screen
--json -o PATH` under `[screenshot] directory`. Child binaries come
from beside this binary, then `PATH`; tokio captures stdout and kills dropped children. `target` 1
follows that rule; 4 always opens the same overlay.
Target 8 runs `capture --window focused` with `permission_store_checked=true`, regardless of the
`interactive` hint, and is refused without permission. Window (2) and invalid targets return 2.

Exit 0 with a result path returns response 0 with a `file://` URI; exit 4 returns 1, and any
other exit, signal or invalid result returns 2. `PickColor` runs `glimpse-lens --tool picker --json`:
RGB bytes become `(red/255, green/255, blue/255)`; exit 4 returns 1, other failures 2.

## Rendering

`render::export` draws a document into GSK render nodes through a `gtk::Snapshot` and exports with a
`gsk::CairoRenderer` realized with no surface — `capture` calls `gtk::init()` and maps nothing. The
frame is uploaded once, as `R8g8b8a8` when it carries alpha (a window from `ScreenshotWindow`, whose
rounded corners stay transparent) and `R8g8b8x8` otherwise. Rectangular blur uses `doc::redact`;
the Blur tool paints `doc::redact::pixelate_stroke` tiles. Stroke width in buffer pixels is
`2 × stroke × output scale`. Colors are Adwaita's red 3, yellow 3, green 4, blue 3 and white.
Text is the Pango layout as a path, stroked in black or white for contrast and then filled.

`render::scene` — `image` then `marks` — is the one drawing both faces share, with a `Cache` that
holds the uploaded frame, one Pango label and cached redaction tiles; `export` calls it once inside
its crop clip. The canvas calls the two halves itself so a pending crop's dim falls between them.

## Overlay

The default command freezes one output: a `Layer::Overlay` surface anchored to every edge, keyboard
`Exclusive`, on the output `headless::output_of` names. It executes each `session` effect in
`gui::overlay::Run::perform`; `headless::take` is the capture ladder for both. Nothing is captured
while a surface of ours is mapped: the countdown pill is destroyed and `glimpse_widgets::roundtrip`
waits for the compositor before the capture, and the overlay is hidden before the save dialog opens.

- **Coordinates.** `gui::canvas::View` maps logical widget points to buffer pixels by `factor`; a
  frame smaller than the output is centered at its logical size, a larger one is fit.
- **Hit.** `Document::hit` returns the topmost mark: a hollow rect or oval only within
  `max(stroke width, 6)` buffer px of its outline, so a number stamps inside a shape; any other mark
  by its filled area, Pango bounds, box or brush path.
- **Crop.** A dragged crop is pending: the image outside it is dimmed (marks never are) until it is
  applied as one undo step, after which only the cropped region is shown.
- **Keys.**

  | Key | Does |
  | --- | --- |
  | Tap or hold `n`, `b`, `r` | select Number, Blur, or Rect; a hold borrows the tool and restores the previous one when released, except from Crop or Text |
  | Enter | commit typed text, else apply a pending crop, else copy and close; a held Enter never copies |
  | Shift+Enter | break the line while typing, else save to the default path and close |
  | Space | take the whole output while a region is dragged |
  | Scroll | step the size of Text, Number, Arrow, Line or Blur from Thin to Thick for the next mark |
  | Esc | through `doc::esc`: drop the text being typed, abort a drag, discard a pending crop, deselect, close the window list, then cancel (exit 4) |
  | Right-click | undo the previous edit; Shift+right-drag on a selected mark's handle resizes it as one undo step |
  | Ctrl+Shift+S | Save As; Cancel returns with the marks intact |

- **Window list.** A floating window visible on the frozen output is cropped from the frozen frame,
  since a fresh capture would contain the overlay; anything else is `ScreenshotWindow`, run on a
  `gio::spawn_blocking` thread behind a spinner, and Esc drops it. While the list is open it keeps
  the focus, so Enter picks the preselected row. A picked window starts on the arrow unless `--tool`
  was given.
- **Finishing.** stdout carries only the `--json` result line; `--timeout` exits 7 with it empty. A
  lost output or Ctrl+E hands the `Editor` — document and undo history — to a new Result window and
  exits 0 when it closes; under `--json` a lost output fails instead (exit 1, kind `output_gone`).
- **Notifications.** Each is a second, notify-only `hold` (`duties.clipboard` false, no entries). A
  capture failure or an unreadable config posts one "Screenshot failed" notice, body the error's
  first line, only when stderr is not a terminal.

## App

Every GUI invocation is the one `adw::Application` `me.aresa.Glimpse.Shot` (`GLIMPSE_SHOT_APP_ID`
overrides it); non-GUI commands never construct it. The local process parses and validates first, so a usage
error exits 2 without contacting the app; the whole argv then goes to GApplication, which forwards
it to the primary (`--gapplication-service` alone reaches GTK on D-Bus activation). The primary
re-parses with `cli::parse` and ignores the caller's `--config-dir`: the one instance serves every
invocation from its own roots. It anchors relative paths at the caller's `cwd`, reads `--annotate -` from the line's `stdin()`
asynchronously under `ANNOTATIONS_CAP`, and answers through `app::reply`, then
`g_application_command_line_set_exit_status` on the held line (gio-rs binds no setter). A forwarded
`-o -` is exit 2; while an overlay is up any GUI invocation is `busy`, exit 1; SIGINT and SIGTERM
exit 130 and 143. Overlay surfaces belong to no application, so its accelerators never reach them.

- **Capture page.** Delay and pointer start from `[screenshot]` and are never written back. Take
  hides the window, waits the unmap barrier and runs the overlay with `in_window`: a concrete target
  hands off at once, Region shows only the selection (release or Space commits), and Esc or the
  countdown's ✕ brings the window back. The window closes when a Result window opens.
- **Result.** A Result window drives the same `session` machine and owns its editor independently.
  Its size and maximized state are kept in `state_dir()/shot-window.json`. Both faces draw the same
  `Editor`; the result picture owns Fit and 1:1, with pointer points mapped back through
  `Sheet::origin`. Save uses the Preferences folder, never over
  another file (a second save overwrites its own), then `RecentManager::add_item`. Dirtiness is
  marks and crop against the last save or copy; closing or ‹ New asks first. File loading refuses a
  side over 16384 before decoding and retains canonical source identity. Prefilled marks start dirty;
  JSON file opens retain their answer until completion, cancellation or timeout. Open With… saves first, then shows the system's app chooser
  (`FileLauncher` with `always-ask`), holding the app until it answers — in the overlay too, whose
  surface is gone by then. Ctrl+drag drags the image out as a 0600 file under `runtime_dir()` plus
  its PNG bytes.
- **Preferences.** Only `[screenshot]` of `glimpse_config::writable_file("config")` — the top root's
  config.toml — is written, through `glimpse_config::write_table` 500 ms after the last change and
  on close, typed values only; the dialog shows that file alone. Children shot spawns (the headless
  capture, the clipboard hold) get `--config-dir` when `glimpse_config::config_dir()` is set. The dialog holds its state until
  closed; a file that does not parse or validate makes the page read-only behind a banner; any other
  write failure is a toast.
- **Inhibits.** A countdown holds `IDLE` ("Waiting to take a screenshot") until the capture, so the
  screen neither blanks nor locks into the shot; a result window with unsaved work holds `LOGOUT`
  until it is saved, copied or closed. Both go through `gtk::Application::inhibit`, so they reach
  gnome-session on GNOME and glimpse-idle's portal backend elsewhere. `run` registers the app id
  with `org.freedesktop.host.portal.Registry` before `gtk::init` — any earlier portal call, such
  as libadwaita reading the color scheme, ties the connection to no id — and keeps that session
  connection alive, since GLib holds the shared one only weakly.

## Compositor setup

The editor window should float. Preferences supplies compositor bindings for screen, window and
headless clipboard captures.
