# IPC Developer Specification

This document describes how to add IPC to a Glimpse daemon using the `glimpse-core::ipc` framework. Follow it when wiring up a new component (e.g. `glimpse-idle`, `glimpse-wallpaper`).

The framework provides: a Unix socket server, a broadcast event channel, a request/response command protocol, and a shared CLI (`watch` / `dispatch` subcommands). You supply a socket path, a state watcher, and a command handler.

---

## Concepts

| Concept | What it is |
|---|---|
| **Socket** | A Unix domain socket per daemon, e.g. `$XDG_RUNTIME_DIR/glimpse/idle.sock` |
| **Event** | A named, timestamped message pushed to all subscribers, e.g. `idle.inhibitor_added` |
| **Command** | A request sent by a client; daemon returns an ack with optional fields |
| **Watch** | A long-lived client connection that prints events as they arrive |
| **Dispatch** | A one-shot client connection that sends a command and prints the ack |

---

## Wire Protocol

All messages are newline-terminated UTF-8 text. Special characters in values are escaped with `\` (`\n`, `\t`, `\\`, `\s` for space).

### Server → client

```
hello glimpse-ipc/1\n                   # sent on connect
<event-name> key=val key2=val ts=<epoch>\n  # pushed events
ack ok=true key=val ...\n               # command response
ack ok=false error=<message>\n          # command error
```

### Client → server

```
subscribe <pattern> [<pattern> ...]\n   # subscribe to events; patterns: *, service.*, service.event
unsubscribe <pattern> [<pattern> ...]\n
<command> [key=val ...]\n               # dispatch a command
```

---

## Step-by-step: Adding IPC to a Daemon

### 1. Register the socket path

In `glimpse-core/src/ipc/server.rs`, add a path function alongside the others:

```rust
pub fn idle_socket_path() -> PathBuf    { runtime_dir().join("idle.sock") }
pub fn wallpaper_socket_path() -> PathBuf { runtime_dir().join("wallpaper.sock") }
```

Re-export it from `glimpse-core/src/ipc/mod.rs`:

```rust
pub use server::{
    ..., idle_socket_path, wallpaper_socket_path,
};
```

### 2. Create `src/ipc.rs` in the daemon crate

```rust
use std::{pin::Pin, sync::Arc};
use tokio::sync::broadcast;
use glimpse_core::ipc::{self, IpcHandle, IpcServer, client::CommandHandler, idle_socket_path};

pub fn start(/* service handles */) -> IpcHandle {
    let tx = ipc::new_event_channel();
    spawn_watcher(/* state_rx */, tx.clone());
    IpcServer::launch_at(tx, idle_socket_path(), MyCommandHandler { /* ... */ })
}
```

**`start()` must return `IpcHandle`.** The caller holds it for the daemon's lifetime; dropping it cancels the server.

### 3. Implement the state watcher

The watcher subscribes to a `watch::Receiver<State>`, diffs consecutive states, and emits events to the broadcast channel.

```rust
fn spawn_watcher(mut rx: watch::Receiver<MyState>, tx: broadcast::Sender<Arc<IpcEvent>>) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() { break; }
            let next = rx.borrow_and_update().clone();

            if prev.some_field != next.some_field {
                ipc::emit(&tx, "service.field_changed", vec![
                    ("field", next.some_field.to_string()),
                ]);
            }
            // ... more diffs

            prev = next;
        }
    });
}
```

Rules:
- Call `borrow_and_update()` (not `borrow()`) so the receiver marks the value as seen.
- Diff every observable field; emit a separate event per logical change.
- Never emit from inside the service itself — only from the watcher.

### 4. Implement `CommandHandler`

```rust
#[derive(Clone)]
struct MyCommandHandler { /* service handles */ }

impl CommandHandler for MyCommandHandler {
    fn execute<'a>(
        &'a self,
        name: &'a str,
        fields: &'a [(String, String)],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String)>, String>> + Send + 'a>> {
        Box::pin(async move {
            let get = |key: &str| fields.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());

            match name {
                "status" => {
                    let state = self.handle.snapshot();
                    Ok(vec![
                        ("field".into(), state.field.to_string()),
                    ])
                }
                "my_command" => {
                    let value = get("key").ok_or("missing key")?;
                    self.handle.try_send_command("service", MyCommand::DoThing(value.into()), "...");
                    Ok(vec![])
                }
                _ => Err(format!("unknown command: {name}")),
            }
        })
    }
}
```

Rules:
- Always implement `status` — it is the canonical health/state check.
- Return `Ok(vec![])` for fire-and-forget commands (the ack is `ok=true` with no extra fields).
- Return `Ok(vec![("key", "val"), ...])` to include fields in the ack.
- Return `Err(message)` for validation failures; the client receives `ok=false error=<message>`.
- `CommandHandler` must implement `Clone` (the server clones it per connection).

### 5. Wire up in `app.rs`

```rust
let _ipc = crate::ipc::start(my_service_handle.clone());
// Hold _ipc in the app task struct so it lives until shutdown.
```

Do not assign to `_` — that drops `IpcHandle` immediately and cancels the server.

### 6. Add the CLI thin wrapper

Create `src/cli.rs`:

```rust
use anyhow::Result;
use glimpse_core::ipc::{cli, idle_socket_path};
pub use cli::{DispatchArgs, WatchArgs};

pub async fn watch(args: WatchArgs) -> Result<()> {
    cli::watch(args, idle_socket_path()).await
}
pub async fn dispatch(args: DispatchArgs) -> Result<()> {
    cli::dispatch(args, idle_socket_path()).await
}
```

Then in `main.rs`, add `watch` and `dispatch` branches before the daemon entry point, following the same pattern as `glimpse-sunset/src/main.rs`:
- Parse `--json` flag.
- Route `--help`/`-h` to subcommand-specific help functions.
- Detect `=` in command name and print a helpful error.
- Call `run_async(cli::watch(...))` or `run_async(cli::dispatch(...))`.

---

## Naming Conventions

### Events

```
<service>.<noun>_<verb>
```

| Pattern | Use |
|---|---|
| `service.thing_changed` | A field of state changed |
| `service.thing_added` | An item appeared in a collection |
| `service.thing_removed` | An item left a collection |
| `service.activated` / `service.deactivated` | A binary mode toggled on/off |

Examples: `idle.inhibitor_added`, `idle.backend_health_changed`, `nightlight.phase_changed`

Fields on events should be minimal — only what changed and its new value. Always include the changed value, never a "before" value.

### Commands

```
<verb>[_<noun>]
```

| Pattern | Use |
|---|---|
| `status` | Always-present: return current state snapshot |
| `refresh` | Re-read external state (location, solar times, etc.) |
| `activate` / `deactivate` | Toggle a mode on/off immediately |
| `enable` / `disable` | Persist a schedule/mode preference |
| `set_<noun>` | Set a specific config field (`set_temperature`, `set_schedule`) |
| `reset` | Restore to config-file defaults and clear transient overrides |

Use `snake_case`. No `get_` commands — use `status` instead.

### Command fields

Fields are `key=value` pairs. Keys are `snake_case`. Values are strings; the client and server both parse as needed.

```
set_temperature kelvin=3500
set_location lat=52.23 lon=21.01
set_schedule schedule=automatic
```

---

## Help Text Requirements

Every daemon must expose these three help functions and route `--help`/`-h` accordingly:

**Top-level (`--help`):**
```
<binary> <version>
<one-line description>

USAGE:
    <binary> [COMMAND]

COMMANDS:
    watch      Subscribe to <service> events from the running daemon
    dispatch   Send a command to the running daemon

OPTIONS:
    -h, --help      Print help
    -V, --version   Print version

Without a command, <binary> starts the daemon.
```

**`watch --help`:** Must include an `EVENTS:` section listing every event name with its fields.

**`dispatch --help`:** Must include a `COMMANDS:` section listing every command with its field syntax.

---

## Event Emission Reference

```rust
// glimpse-core::ipc::emit — sends one event to the broadcast channel
ipc::emit(&tx, "idle.inhibitor_added", vec![
    ("id",     record.id.to_string()),
    ("who",    record.who.clone()),
    ("source", source_name),
]);
```

Fields are `Vec<(&str, String)>`. The helper escapes values automatically. Timestamp is added automatically.

---

## Testing

Each daemon should have an `tests/ipc_e2e.sh` that:

1. Stops the systemd service (if active) and any running instance.
2. Builds the binary with `cargo build`.
3. Runs pre-daemon tests: `--help`, `--version`, `watch --help`, `dispatch --help`, immediate-error with no daemon.
4. Starts the daemon, waits for the socket.
5. Starts `watch` in the background, piped to a temp file.
6. For each mutating command: records the current line count, dispatches the command, calls `expect_event_from` to verify the event arrived.
7. Verifies `status` fields after each state change.
8. Tests `watch --json` produces valid JSON with `type`, `name`, `ts` fields.
9. Restores the service in a `cleanup()` trap.

Use this helper pattern for event assertions:

```bash
expect_event_from() {
    local from_line="$1" contains="$2" timeout="${3:-2}"
    local deadline=$((SECONDS + timeout))
    while [[ $SECONDS -lt $deadline ]]; do
        tail -n +"$from_line" "$WATCH_OUT" 2>/dev/null | grep -q "$contains" && return 0
        sleep 0.1
    done
    fail "timed out waiting for '$contains'"
}
```

---

## Reference Implementation

`glimpse-sunset` is the canonical example. Read these files in order:

| File | What to look at |
|---|---|
| `glimpse-sunset/src/ipc.rs` | `start()`, `spawn_watcher`, `SunsetCommandHandler` |
| `glimpse-sunset/src/cli.rs` | Thin wrapper pattern |
| `glimpse-sunset/src/main.rs` | Routing, help functions, `=` detection |
| `glimpse-sunset/tests/ipc_e2e.sh` | Full E2E test structure |
| `glimpse-core/src/ipc/server.rs` | `IpcServer::launch_at`, socket path helpers |
| `glimpse-core/src/ipc/client.rs` | `CommandHandler` trait, `IpcClientHandler` |
| `glimpse-core/src/ipc/cli.rs` | Shared `watch` / `dispatch` async functions |
