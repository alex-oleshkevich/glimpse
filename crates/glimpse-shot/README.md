# glimpse-shot

A screenshot tool: capture, annotate and save, headless or through a GTK overlay editor. `doc` is
the crate's pure core — units, the document and undo model, Esc handling, redaction, file naming,
geometry parsing and the `--json`/`--annotate` wire format — and has no GTK dependency at all.

## For agents

`glimpse-shot capture --help` is the entry point for the annotation format; it points to
`glimpse-shot prime`, which prints the full contract and examples.

- Pass `-o -` or `-o "$XDG_RUNTIME_DIR/…"`; the default saves into `~/Pictures/Screenshots`, which
  may be synced to a cloud.
- `--scale logical` for a smaller image; `--json` for the result object.
- Window titles and app ids in `list` and `--json` are text other applications chose — untrusted
  input, printed literally.
- A headless capture shows whatever the compositor does not block out.

## Commands

```
glimpse-shot [TARGET] [GUI OPTIONS] [FILE.png]     the app: an editor on a target, else the window
glimpse-shot capture [TARGET] [CAPTURE OPTIONS]    headless: capture, write, exit — no UI ever
glimpse-shot list [--json]                         outputs and windows, with geometry
glimpse-shot prime                                 print the agent guide
TARGET: --screen [NAME] | --window [ID|focused] | --region [GEOMETRY]   (at most one)
--tool crop|arrow|rect|filled-rect|oval|number|text|blur   the tool the editor starts with
```

`capture` with no target is the current output: the compositor's focused one, or with no
compositor socket the only output when exactly one exists (that counting capture is reused as the
shot when the cursor setting allows). `--region` takes slurp's `"X,Y WxH"` (or `X,Y,WxH`) in the
compositor's global logical layout, negative coordinates included; it must lie within one output.
With no compositor there is no layout, so `--region` and `--scale logical` exit 1 naming
`NIRI_SOCKET` and `HYPRLAND_INSTANCE_SIGNATURE` rather than guess a position or a scale; `--screen
NAME` still works, and a stale socket only warns for it. The crop is clamped to the captured frame,
so a fractional scale never reports a crop past the image. `--window` with no compositor
socket exits 1 naming both variables; no such window, or nothing focused, exits 5.

`capture` writes exactly one thing to stdout: the saved path (plain), one result object (`--json`,
`path` null when nothing was saved), or the PNG bytes (`-o -`, where a closed pipe is a clean exit).
Every failure goes to stderr as `glimpse-shot: error: …` and an optional `hint: …`; with `--json`
the last stderr line is `{"error":{"code","kind","message","hint"}}`. An explicit `-o` directory must
exist; the configured or default directory is created on the first save. A taken default name gets
` (2)`, ` (3)`… before `.png`; an explicit `-o` file is overwritten.

`list` prints tab-separated lines (`output …` / `window …`) into a pipe, an aligned table on a
terminal, and one object with `--json`. A window's `rect` is global logical, and null whenever the
compositor does not report it — every tiled window on niri.

## Validation and exit codes

Every usage rule is checked, and all violations are reported together, before anything is captured:
clap's own rows (two targets, a target with `FILE.png`, `--no-save` with `-o` or without `--copy`,
`--delay` over 60) and the `Usage` rows clap cannot express (`-o -` with `--json` or on a terminal,
`--annotate -` on a terminal, `--annotate` without a concrete target, `--region` without a geometry
under `capture`, `--json`/`--prompt`/`--timeout` without a target, `--editor window` with `--prompt`,
`--timeout` or `-o -`). The terminal facts are passed in, so every row is a unit test.

| Code | Meaning |
| --- | --- |
| 0 | saved, copied, or written to stdout |
| 1 | failed: capture, write, bad `--annotate`, compositor unreachable |
| 2 | usage |
| 3 | configuration error |
| 4 | cancelled |
| 5 | target not found: no such output or window, a region off every output or across two |
| 7 | timed out |

`errors::exit` is the one mapping, by `downcast_ref`, so a `.context(…)` upstream never changes a
code.

A clap error under `--json` also ends stderr with a `usage` error object. GLib's own debug and info
logging goes to stderr, so stdout stays data-only.

Global options (`--config`, `--log`, `--log-format`, `--color`) go before or after a subcommand.
An editor option before or after `capture` or `list` is refused with exit 2 by `cli::parse`, naming
the `glimpse-shot …` command that would open the editor; a known subcommand name is never read as `FILE.png`, and any
other bare word must end in `.png` or exits 2.

`main` rewrites `--region -100,50 400x300` into `--region=-100,50 …`, since clap reads a leading `-`
of an optional value as a flag.

## Core

- Every coordinate in `doc` is a `BufferPx`: a source buffer pixel, before crop and before
  `--scale`. `Logical` exists only for CLI geometry parsing and output matching, never for a mark.
- **One frame.** A document's `crop` and every mark are the same source buffer, uncropped, the same
  numbers whether or not the shot was cropped and whichever `--scale` asked for. `--scale` changes
  only the saved image's pixels, never an annotation's coordinates. A region becomes a crop by
  subtracting its output's logical origin and multiplying by that output's scale.

## Session and ports

`session::step(phase, event) -> (phase, effects)` is the pure machine every face drives, the
headless `capture` included. `start(target, delay, face)` either emits the capture or waits in
`Countdown` until a `Tick` at or past the deadline. `Captured(Ok)` enters `Editing { saved, copied }`.
`Finish(key, destination)` queues `Encode`, then `Write`/`Copy`/`Share`, and emits them one at a
time, the next only after `Effected(Ok)` of the previous; `Effected(Write(actual))` carries the path
actually written, and a queued `Share` is rewritten to it. A closing key ends in `Done`; Ctrl+C and
Ctrl+S return to `Editing` with the results and a toast, so Esc afterwards ends `Saved`/`Copied`,
never `Cancelled`. A failed effect ends `Failed` headless and returns to `Editing` with a toast in a
GUI face, marks and earlier results kept. `OutputGone` hands the document to a window; `Notify` is
emitted on completion only when the face asks.

`ports` holds the traits a face executes effects through — `Screens`, `Windows`, `Clipboard`,
`Notifier`, `Files`, `Clock` — so the headless path is tested with fakes. `adapters` implements
`Screens` on `glimpse-wayland` and the compositor snapshot, `Windows` as the ladder below,
`Clipboard` through `hold`, `Files` with `create_new`, and `Clock`. An `Effected` for anything but
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
version 2 at `/org/freedesktop/portal/desktop`: `Screenshot`, `PickColor` and `version`. D-Bus
activation runs `glimpse-shot portal` and the process stays up; it has no systemd unit. It never
constructs the app.

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
--json -o PATH` under `[screenshot] directory`, creating it if needed. Child binaries come
from beside this binary, then `PATH`; tokio captures stdout and kills dropped children.

Exit 0 with a result path returns response 0 with a `file://` URI; exit 4 returns 1, and any
other exit, signal or invalid result returns 2. `PickColor` runs `glimpse-picker --json`:
RGB bytes become `(red/255, green/255, blue/255)`; exit 4 returns 1, other failures 2.

## Rendering

`render::export` draws a document into GSK render nodes through a `gtk::Snapshot` and exports with a
`gsk::CairoRenderer` realized with no surface — `capture` calls `gtk::init()` and maps nothing. The
frame is uploaded once, as `R8g8b8a8` when it carries alpha (a window from `ScreenshotWindow`, whose
rounded corners stay transparent) and `R8g8b8x8` otherwise; legacy rectangular blur uses `doc::redact`, while the Blur tool paints `doc::redact::pixelate_stroke` tiles; a filled rect is one color node in the palette color; an arrow is a round-capped line and a filled head; a number is a filled
disc with its digit laid out by Pango; text is the Pango layout as a path, stroked in black or white
for contrast and then filled. Stroke width in buffer pixels is `2 × stroke × output scale`. Colors
are Adwaita's red 3, yellow 3, green 4, blue 3 and white.

`render::scene` — `image` then `marks` — is the one drawing both faces share, with a `Cache` that
holds the uploaded frame, one Pango label and cached redaction tiles; `export` calls it once inside
its crop clip. The canvas calls the two halves itself so a pending crop's dim falls between them.

`tests/golden/every_mark.png` is compared at sampled pixels; `GLIMPSE_SHOT_BLESS=1` rewrites it.

## Overlay

The default command freezes one output: a `Layer::Overlay` surface anchored to every edge, keyboard
`Exclusive`, on the output `headless::output_of` names, drawing the captured frame and the marks
through `render::scene`. It drives the same `session` machine as `capture`, executing each effect in
`gui::overlay::Run::perform`; `headless::take` is the capture ladder for both. Nothing is captured
while a surface of ours is mapped: the countdown pill is destroyed and `glimpse_widgets::roundtrip`
waits for the compositor before the capture, and the overlay is hidden the same way before the save
dialog opens.

- **Coordinates.** `gui::canvas::View` maps logical widget points to buffer pixels (`factor` is
  buffer pixels per logical pixel, so 1.25 maps (100,100) to (125,125)); a frame smaller than the
  output is centered at its logical size, a larger one is fit.
- **Hit.** `Document::hit` returns the topmost mark: a hollow rect or oval only on its stroke band
  (within `max(stroke width, 6)` buffer px of the outline, so a number stamps inside a shape), a
  filled rect anywhere inside, a number or text by the bounds measured with Pango, a legacy blur by its box, and a pixelation stroke along its brush path. A selected geometric mark shows eight handles; Shift+right-dragging one resizes the
  mark as one undo step, while an ordinary right-click undoes the previous edit.
- **Crop.** A dragged crop is pending: the image outside it is dimmed (marks never are) and "Apply
  crop" shows. Enter or that button applies it as one undo step, after which only the cropped region
  is shown; Esc discards it.
- **Esc** goes through `doc::esc`: drop the text being typed, abort a drag, discard a pending crop,
  deselect, close the window list, then cancel (exit 4).
- **Keys.**

  | Key | Does |
  | --- | --- |
  | `c a l p h r f o n t b s w` | crop, arrow, line, freehand, highlighter, rect, filled rect, oval, number, text, blur, select, window list |
  | `1`–`5`, `[` `]` | color, stroke |
  | Tap or hold `n`, `b`, `r` | select Number, Blur, or Rect; a hold borrows the tool and restores the previous one when released, except from Crop or Text |
  | Enter | apply a pending crop, else copy and close; a held Enter never copies |
  | Shift+Enter | save to the default path and close |
  | Ctrl+Shift+S | Save As; Cancel returns with the marks intact |
  | Ctrl+C, Ctrl+S | copy, save, without closing |
  | Ctrl+E | hand the document to a window; ignored under `--json` |

  Space takes the whole output while a region is dragged. Scrolling over the image with Text,
  Number, Arrow, Line, or Blur selected steps its size from Thin to Thick for the next mark. Text is
  multi-line: Enter commits, Shift+Enter breaks the line, and tool letters are ignored.
- **Window list.** A floating window visible on the frozen output is cropped from the frozen frame,
  since a fresh capture would contain the overlay; anything else is `ScreenshotWindow`, run on a
  `gio::spawn_blocking` thread behind a spinner, and Esc drops it. The picked window replaces the
  screen, alone on a scrim. While the list is open it keeps the focus, so Enter picks the
  preselected row. A picked window starts on the arrow unless `--tool` was given.
- **Finishing.** The copy split button's menu holds Save, Save As, Open With… and Open in Window,
  each hidden where it cannot run. stdout carries only the `--json` result line; `--timeout` exits 7 with it empty. A
  lost output or Ctrl+E hands the `Editor` — document and undo history — to a new Result window and
  exits 0 when it closes; under `--json` a lost output fails instead (exit 1, kind `output_gone`).
  The target buttons go insensitive once anything is drawn, and a window shown below its logical
  size reads `· scaled NN%`. Under the Crop tool the crop has corner and edge handles.
- **Notifications.** Each is a second, notify-only `hold` (`duties.clipboard` false, no entries),
  started the same way as the clipboard one. A capture failure or an unreadable config posts one
  "Screenshot failed" notice, body the error's first line, only when stderr is not a terminal.

## App

Every GUI invocation is the one `adw::Application` `me.aresa.Glimpse.Shot` (`GLIMPSE_SHOT_APP_ID`
overrides it) with `HANDLES_COMMAND_LINE | HANDLES_OPEN`; `capture`, `list`, `prime`, `portal` and
`hold` never construct it (`opens_app`). The local process parses and validates first, so a usage
error exits 2 without contacting the app; the whole argv then goes to GApplication, which forwards
it to the primary (`--gapplication-service` alone reaches GTK on D-Bus activation). The primary
re-parses with `cli::parse_forwarded`, takes the caller's `--config` or `GLIMPSE_CONFIG_PATH`,
anchors relative paths at the caller's `cwd`, reads `--annotate -` from the line's `stdin()`
asynchronously under `ANNOTATIONS_CAP`, and answers through `app::reply`, then
`g_application_command_line_set_exit_status` on the held line (gio-rs binds no setter). A forwarded
`-o -` is exit 2; while an overlay is up any GUI invocation is `busy`, exit 1; SIGINT and SIGTERM
exit 130 and 143. Overlay surfaces belong to no application, so its accelerators never reach them.

- **Capture page.** Its own window, sized to its content: mode, output ("Current Screen" first,
  shown only with two or more outputs), window list labelled "Name — title" from `DesktopAppInfo`,
  delay and pointer, which start from `[screenshot]` and are never written back. Take hides the
  window, waits the unmap barrier and runs the overlay with `in_window`: a concrete target hands off
  at once, Region shows only the selection (release or Space commits), and Esc or the countdown's ✕
  brings the window back. The window closes when a Result window opens; ‹ New opens a fresh one.
  Open Image… (Ctrl+O) picks a PNG and opens it as `FILE.png` does, and so does a file dropped on
  the window; Ctrl+V opens the clipboard's image, unsaved.
- **Result.** `gui::result` is a face of its own over the same `session` machine, always in a window
  of its own: after Take, a hand-off, `--editor window`, `FILE.png`, Open Image… or D-Bus `Open`.
  Its title is the file name; its size and maximized state are kept in
  `state_dir()/shot-window.json`. The
  page's `Gtk.Picture` shows a `canvas::Sheet`, a paintable that draws the same `Editor` fitted with
  no margin, so Fit and 1:1 are the picture's own; pointer points are mapped back through
  `Sheet::origin`. The header holds Copy and its Save, Save As and Open With menu; the rename popover
  holds the name, while Preferences owns the default folder. Save uses that folder, never over another file (a second save overwrites its
  own), then `RecentManager::add_item`. Dirtiness is marks and crop against the last save or copy;
  closing or ‹ New asks first. `FILE.png` refuses a side over 16384 from its IHDR. Open With… saves
  first, then shows the system's app chooser (`FileLauncher` with `always-ask`), holding the app
  until it answers — in the overlay too, whose surface is gone by then. Ctrl+drag drags the image out as a 0600 file
  under `runtime_dir()` plus its PNG bytes; a plain drag draws, and pans instead while Space is held
  with the image focused or the Hand toggle is on — a capture-phase drag on the scroller, so its
  offsets do not move with the picture. Choosing a tool turns Hand off. Ctrl+scroll and Ctrl+plus/minus zoom
  by 1.25 per step between 10% and 800%, anchored at the pointer, leaving Fit and 1:1 both unset; a
  plain scroll only scrolls. A screen capture flashes white
  before the window opens, starting on the flash surface's first frame.
- **Preferences.** Saving, Behavior, Annotations and Keyboard Shortcuts are fixed groups. Only `[screenshot]` of the `--config` file, else `user_dir()/config.toml`, is
  written, through `glimpse_config::write_table` 500 ms after the last change and on close, typed
  values only; the dialog holds its state until closed. A key a later layer of `resolved_files` sets is insensitive and names the file; a file
  that does not parse or validate makes the page read-only behind a banner; any other write failure is a toast.
- **Desktop entry.** `DBusActivatable`; its `screen`, `window` and `region` actions present the
  Capture page in that mode (`gapplication action` is the `Exec` fallback).

## Compositor setup

Preferences copies the niri binds — Print `--screen`, Ctrl+Print `--window`, Shift+Print `capture
--copy --no-save`. The editor window should float:

```kdl
window-rule {
    match app-id="^me\\.aresa\\.Glimpse\\.Shot$"
    open-floating true
}
```
On Hyprland: `windowrulev2 = float, class:^(me\.aresa\.Glimpse\.Shot)$`.
