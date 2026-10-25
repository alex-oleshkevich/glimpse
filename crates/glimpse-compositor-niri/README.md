# glimpse-compositor-niri

The niri provider: serves `me.aresa.Glimpse.Compositor` by implementing `glimpse_compositors::Backend`
over niri's IPC socket (`$NIRI_SOCKET`). It reads no glimpse configuration.

## Rules

- **`--probe` is a real request.** It sends `Version` and exits 0 only on a well-formed reply within
  2 s; an unset or empty `$NIRI_SOCKET`, a stale socket file nothing listens on, and a silent
  listener all exit 1. The same unset variable also exits 1 without `--probe`.
- **niri answers one request per connection.** Only the path is held; `EventStream` owns its
  connection and consumes its initial workspace, window and keyboard replication before returning;
  the provider's subsequent snapshot supplies those values. Nothing here retries or reconnects.
- **Exit 4 means another provider owns the bus name** and is never restarted.
- **A cast carries `session` only when it is a PipeWire cast.** `StopCast` works on no other kind.
  `pw_node` is passed through as niri reports it, `output` is set only for an output target, and
  `id` is niri's `stream_id`.
- **Text from niri is capped on the way in** with `glimpse_compositors::sanitize`; the refusal reason
  niri gives is capped the same way.
- **Window geometry is output-relative logical pixels.** The rect is `tile_pos_in_workspace_view +
  window_offset_in_tile` with `window_size` as its extent. niri reports no position for a window in
  the scrolling layout, so a tiled window has no entry and is captured through `ScreenshotWindow`.
- **`screenshot_window` waits for the file, not the reply.** niri answers before the PNG exists. An
  inotify watch on the target's parent directory is armed for `IN_CLOSE_WRITE` and `IN_MOVED_TO`
  *before* the action is sent, and the call returns once an event names the target file or fails
  after 2 s. The wait runs in `spawn_blocking`. The path must be UTF-8 with a file name and a
  parent, or it is `Refused` before anything is armed.
- **`ScreenshotWindow` always replaces the clipboard**; restoring it is the caller's job.
- **Steps use niri's own actions**: `FocusWorkspaceDown`/`Up`, `FocusWindowDownOrColumnRight`/
  `UpOrColumnLeft` and `SwitchLayout` `Next`/`Prev`, which follow niri's monitor layout.
- **An empty rename name unsets the workspace name.**
- **An output is enabled when it has a current mode**; niri has no `enabled` field.
- **niri has no output event.** A change in the set of outputs named by `WorkspacesChanged`, and a
  successful `ConfigLoaded`, ask for a resync.
- **There is no session-lock-restore concept**, so that property is not served.

## Tests

The suite runs against `FakeNiri`, a Unix socket in a temporary directory replaying scripted lines;
every test passes its path through `Niri::at`, so none touches the process environment.
