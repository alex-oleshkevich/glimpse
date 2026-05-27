# IPC Developer Specification

Glimpse daemons expose a small Unix-socket IPC protocol from `glimpse-core::ipc`. Use this document when adding IPC to a daemon or extending an existing command surface.

The framework provides socket binding, event broadcasting, request/response command handling, escaping, and shared `watch` / `dispatch` CLI helpers. Daemon code supplies the socket path, state-to-event watcher, and command handler.

## IPC Surfaces

| Service | Socket helper | Default socket | Notes |
|---|---|---|---|
| Shell | `shell_socket_path()` | `$XDG_RUNTIME_DIR/glimpse/ipc.sock` | Main panel IPC surface and service event dispatcher. |
| Applets | `applets_socket_path()` | `$XDG_RUNTIME_DIR/glimpse/applets.sock` | Reserved applet management socket. |
| Idle | `idle_socket_path()` | `$XDG_RUNTIME_DIR/glimpse/idle.sock` | Idle inhibitor events and idle daemon commands. |
| Night light | `sunset_socket_path()` | `$XDG_RUNTIME_DIR/glimpse/sunset.sock` | Night-light state and schedule commands. |
| Wallpaper | `wallpaper_socket_path()` | `$XDG_RUNTIME_DIR/glimpse/wallpaper.sock` | Wallpaper, backdrop, and theme-mode commands. |

`GLIMPSE_IPC_DIR` overrides the socket directory for every server and CLI client. Use it for tests or parallel developer sessions. Without it, sockets live under `$XDG_RUNTIME_DIR/glimpse`. If `$XDG_RUNTIME_DIR` is missing, IPC refuses to start and asks the caller to set `GLIMPSE_IPC_DIR` or run inside a proper user session.

Sockets are created with `0600` permissions. `IpcHandle` owns the server lifetime; dropping the handle cancels the accept loop and removes the socket file.

## Wire Protocol

All messages are newline-terminated UTF-8 text. Values are space-separated `key=value` fields. Values escape backslash, newline, tab, and space as `\\`, `\n`, `\t`, and `\s`.

### Server To Client

```txt
hello version=<crate-version>
<event-name> key=value key2=value2 ts=<unix-seconds>
ack ok=true
ack ok=false error=<escaped-message>
```

The server sends `hello` immediately after a client connects. Event lines are sent only to clients subscribed with `subscribe`. Command clients receive one `ack` line.

### Client To Server

```txt
subscribe <pattern> [<pattern> ...]
unsubscribe <pattern> [<pattern> ...]
<command> [key=value ...]
```

Subscription patterns support three forms:

| Pattern | Matches |
|---|---|
| `*` | Every event. |
| `audio.*` | Events under a namespace, such as `audio.volume_changed`. |
| `audio.volume_changed` | One exact event. |

Malformed client lines return `ack ok=false error=<message>`. Unknown commands are handled by the daemon command handler.

## CLI Behavior

Each daemon that exposes IPC should route these commands before starting the normal daemon process:

```sh
<daemon> watch [--json] [pattern...]
<daemon> dispatch [--json] <command> [key=value...]
```

`watch` subscribes to `*` when no pattern is provided. With `--json`, event lines become objects like:

```json
{"type":"event","name":"audio.volume_changed","volume":"55","ts":1710000000}
```

`dispatch --json` converts an ack line into JSON:

```json
{"ok":true}
```

If dispatch receives `ack ok=false`, the CLI exits with failure after printing the ack.

## Add IPC To A Daemon

### 1. Register The Socket

Add a helper in `glimpse-core/src/ipc/server.rs` and re-export it from `glimpse-core/src/ipc/mod.rs`:

```rust
pub fn my_service_socket_path() -> PathBuf {
    runtime_dir().join("my-service.sock")
}
```

Shell uses `IpcServer::launch(...)` because it has the full `Services` dispatcher. Standalone daemons usually use `IpcServer::launch_at(...)` with their own event channel.

### 2. Start The Server

Create `src/ipc.rs` in the daemon crate:

```rust
use std::{pin::Pin, sync::Arc};

use glimpse_core::ipc::{self, IpcHandle, IpcServer, client::CommandHandler, my_service_socket_path};
use tokio::sync::broadcast;

pub fn start(service: MyServiceHandle) -> IpcHandle {
    let tx = ipc::new_event_channel();
    spawn_watcher(service.subscribe(), tx.clone());
    IpcServer::launch_at(tx, my_service_socket_path(), MyCommandHandler { service })
}
```

Store the returned `IpcHandle` for the daemon lifetime. Assigning it to `_` drops the server immediately.

### 3. Emit Events From A Watcher

Watchers subscribe to service state, diff consecutive snapshots, and emit one event per logical change:

```rust
fn spawn_watcher(mut rx: watch::Receiver<MyState>, tx: broadcast::Sender<Arc<IpcEvent>>) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            if prev.enabled != next.enabled {
                ipc::emit(&tx, "my_service.enabled_changed", vec![
                    ("enabled", next.enabled.to_string()),
                ]);
            }

            prev = next;
        }
    });
}
```

Use `borrow_and_update()` so each receiver marks the value as seen. Keep event fields minimal: include the changed value and stable identifiers, not whole snapshots.

### 4. Implement Commands

`CommandHandler` is cloned per connection and returns ack fields or an error string:

```rust
#[derive(Clone)]
struct MyCommandHandler {
    service: MyServiceHandle,
}

impl CommandHandler for MyCommandHandler {
    fn execute<'a>(
        &'a self,
        name: &'a str,
        fields: &'a [(String, String)],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String)>, String>> + Send + 'a>> {
        Box::pin(async move {
            let get = |key: &str| fields.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());

            match name {
                "status" => Ok(vec![("enabled".into(), self.service.snapshot().enabled.to_string())]),
                "set_enabled" => {
                    let enabled = parse_bool(get("enabled").ok_or("missing enabled")?)?;
                    self.service.set_enabled(enabled).await.map_err(|e| e.to_string())?;
                    Ok(vec![])
                }
                _ => Err(format!("unknown command: {name}")),
            }
        })
    }
}
```

Command rules:

| Rule | Reason |
|---|---|
| Implement `status` | It is the standard health and state check. |
| Return `Ok(vec![])` for fire-and-forget commands | The client receives `ack ok=true`. |
| Return fields for stateful commands | The client receives `ack ok=true key=value`. |
| Return `Err(message)` for validation failures | The client receives `ack ok=false error=<message>`. |
| Parse every field inside the handler | The wire protocol carries strings only. |

## Naming Conventions

Events use `<service>.<noun>_<verb>`:

| Pattern | Use |
|---|---|
| `service.thing_changed` | A field changed. |
| `service.thing_added` | An item appeared in a collection. |
| `service.thing_removed` | An item left a collection. |
| `service.activated` / `service.deactivated` | A binary mode toggled. |

Examples: `idle.inhibitor_added`, `idle.backend_health_changed`, `nightlight.phase_changed`.

Commands use `snake_case` verbs:

| Pattern | Use |
|---|---|
| `status` | Return current state. |
| `refresh` | Re-read external state. |
| `set_<noun>` | Set one value. |
| `reset` | Return to config defaults or clear transient overrides. |

Do not add `get_*`; use `status`.

## Existing Command Surfaces

| Service | Commands |
|---|---|
| Shell | `status`, `set_volume`, `set_input_volume`, `set_brightness`, `set_power_profile`, `set_dnd`, `set_theme`, `set_keyboard_layout`, `set_wifi`, `set_bluetooth`, `refresh`, `set_location` |
| Night light | `status`, `refresh`, `set_temperature`, `set_schedule`, `set_times`, `set_location`, `reset` |
| Wallpaper | `status`, `set_image`, `set_color`, `set_fit`, `set_backdrop`, `set_theme_mode` |
| Idle | `status` |

Keep `dispatch --help` in each daemon aligned with the implemented commands.

## Help Text Requirements

Each IPC-enabled daemon should expose:

| Help path | Required content |
|---|---|
| Top-level `--help` | Normal daemon behavior, `watch`, `dispatch`, `--help`, `--version`. |
| `watch --help` | Pattern syntax, `--json`, and every event namespace or event name. |
| `dispatch --help` | Every command with field syntax and allowed values. |

Reject dispatch command names that contain `=` before calling the shared CLI. This catches the common mistake of putting the first `key=value` where the command name belongs.

## Testing

Each daemon with IPC should have an end-to-end test that:

1. Stops any running packaged service for that daemon.
2. Builds the binary.
3. Checks `--help`, `--version`, `watch --help`, and `dispatch --help`.
4. Checks dispatch failure when no daemon is running.
5. Starts the daemon and waits for the socket.
6. Starts `watch` in the background.
7. Dispatches each command and verifies the ack.
8. Verifies expected event lines after mutating commands.
9. Verifies `watch --json` and `dispatch --json` output.
10. Restores the original service state in cleanup.

For event assertions, record the current watch output line count before dispatch, then scan only new lines. That avoids passing because of an old event.

## Reference Files

| File | Use it for |
|---|---|
| `glimpse-core/src/ipc/protocol.rs` | Wire format, escaping, pattern matching, ack/hello encoding. |
| `glimpse-core/src/ipc/client.rs` | `CommandHandler` and per-client subscription loop. |
| `glimpse-core/src/ipc/server.rs` | Socket paths, server lifetime, permissions, `IpcHandle`. |
| `glimpse-core/src/ipc/cli.rs` | Shared `watch` and `dispatch` behavior. |
| `glimpse-shell/src/ipc/handler.rs` | Largest command handler and shell status snapshot. |
| `glimpse-sunset/src/ipc.rs` | Standalone daemon with watcher and mutating commands. |
| `glimpse-wallpaper/src/ipc.rs` | Standalone daemon with wallpaper/backdrop commands. |
| `glimpse-idle/src/ipc.rs` | Minimal daemon IPC surface. |
