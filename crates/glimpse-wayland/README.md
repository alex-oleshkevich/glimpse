# glimpse-wayland

Screen capture and the data-control clipboard, for the binaries that bind `wl_` objects: the picker,
the ruler, the panel and the screenshot tool. A leaf library — `glimpse-services` never depends on
it, because no service crate may bind a Wayland object.

## Capture

`outputs`, `output` and `toplevel` are synchronous and take one `deadline`. Nothing blocks without
it: the registry is listed through a `wl_display.sync` polled like every other wait, never
`registry_queue_init`, whose roundtrip has no timeout. A caller that must not stall runs them on a
thread.

**`choose` decides the ladder from the advertised globals.** ext wins when both
`ext_image_copy_capture_manager_v1` and `ext_output_image_capture_source_manager_v1` are present;
otherwise `zwlr_screencopy_manager_v1`; neither is `Unsupported`. Windows need the copy manager,
`ext_foreign_toplevel_image_capture_source_manager_v1` and `ext_foreign_toplevel_list_v1` together
— screencopy has no window capture, so `toplevels` is false without them.

**Outputs are named before anything is captured.** Frames are matched to monitors by connector,
which `wl_output` carries only from version 4, so an older output is `Unnamed`. `output(name, …)`
names every output and captures only the match; no match is `NoSuchOutput` with the name asked for.

**A toplevel is found by its `ext_foreign_toplevel_handle_v1` identifier**, read in one roundtrip
after binding the list, which is then stopped and destroyed once the compositor sends `finished`. No
match is `NoSuchToplevel`. Its frame's `connector` is empty. `toplevels` lists every handle's
`identifier`, `app_id` and `title` the same way, capped at 128 and 512 characters; no list global is
`Unsupported`.

**Frames arrive upright and packed.** Capture applies the output transform and `y_invert` once.
`layout` identifies RGB or BGR byte order; normal packed captures retain their original buffers.
`pixel` returns RGB, clamped to the edge. `alpha` says whether the fourth byte is coverage; every
captured frame is opaque and sets it false.

**Only 8-bit buffers are read.** Screencopy takes `Xrgb8888`, `Argb8888`, `Xbgr8888` and
`Abgr8888`. The ext path negotiates from the session's `shm_format` and `buffer_size` and takes only
`Xrgb8888` and `Argb8888`, both `Layout::Bgrx` in memory; anything else is `Format` with its name
rather than a guessed layout. The buffer is a memfd named `glimpse-capture`.

`cursor` paints the pointer into the frame: `overlay_cursor` for screencopy, `paint_cursors` for
ext. A window capture never carries it.

## Selection

**`x-kde-passwordManagerHint` is classified off the mime list before any read**, so a secret never
enters the process. `watch` skips such a selection; `snapshot`
reads none of its mimes and records each as `SkipReason::Sensitive`. `glimpse_utils::is_sensitive`
is the rule.

**Nothing larger than 2 MiB is kept.** Reads detect overflow rather than keeping a fragment that
pastes back as corrupt content. Offers retain at most 64 mimes of 256 bytes each, including any sensitive hint.

**`watch` reads one representation per selection**, the first of a fixed ladder — UTF-8 text, the
other text spellings, HTML, PNG, JPEG — with a 5 s deadline on the pipe read itself. A superseded read
cannot publish after the newer selection. The stream
owns its connection: dropping it closes the connection, so a caller that holds it only while someone
listens opens nothing otherwise. The `wanted` closure is asked before every read, and a selection
nobody wants is destroyed unread — dropping the stream is not immediate enough to be the gate. A
failure is `Unavailable` and a retry two seconds later; a compositor with neither data-control
protocol is `Unavailable` once and the stream ends. The setup runs on the blocking pool with its
`JoinHandle` carried across retries, because dropping one does not cancel it.

**`snapshot` shares one time budget and one 2 MiB total across mimes**, setup included. A mime the
time budget does not reach is `TimedOut`, one exceeding the remaining bytes and every subsequent
mime is `TooLarge`, one whose pipe fails is `Unreadable`.

**`Changes` records every selection change after `start`**, as its mime list, and an empty list for
a cleared selection. `since` makes a roundtrip first, so everything the compositor sent before the
call is counted.

**`Owned::serve` sets the selection and answers every paste until another client takes it**, each
entry under its own mime. It needs a tokio runtime. `current` is asked after the connection is set
up and immediately before the selection is set; `false` sets nothing and returns `None`, which is
how a caller keeps a superseded or cancelled offer from landing after a newer one. It returns only
after a roundtrip, so a returned `Owned` is a selection the compositor accepted and still holds. `cancelled`
resolves once the selection is lost or the connection fails; dropping the `Owned` gives the
selection up. At most eight paste targets are written process-wide, each on the blocking pool with
a 5 s write deadline, so a slow reader cannot stall the queue or hold its thread indefinitely.

**`data_offer` precedes `selection` or `primary_selection`.** The primary arm destroys its offer
too; ignoring it leaks a compositor resource per mouse highlight.

ext-data-control is bound before wlr-data-control when both are offered; the two are the same
protocol under two names, wrapped as enums in `protocol.rs` rather than abstracted.
