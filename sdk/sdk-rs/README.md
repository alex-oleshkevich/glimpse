# glimpse-sdk for Rust

Write a glimpse panel applet in Rust. The applet is a Rust binary: the panel starts it as a
systemd user unit, and the SDK speaks the panel's NDJSON protocol on the unix socket named by
`GLIMPSE_APPLET_SOCKET`, which the panel sets (without it the applet exits with code 1). The wire format is
specified in [PROTOCOL.md](../sdk-ts/PROTOCOL.md) and is shared with the other SDKs. The
dependencies are `serde` and `serde_json`.

```rust
use glimpse_sdk::*;
use std::process::ExitCode;

struct App;

enum Msg {}

impl glimpse_sdk::App for App {
    type Msg = Msg;

    fn update(&mut self, _ctx: &Ctx, msg: Msg) {
        match msg {}
    }

    fn build(&self, _ctx: &Ctx) -> Vec<Widget<Msg>> {
        vec![
            indicator()
                .icon("face-smile-symbolic")
                .tooltip("Hello")
                .text("Hello")
                .into(),
            popover()
                .icon("face-smile-symbolic")
                .title("Hello")
                .child(section().title("Hello").child(row().title("It works")))
                .into(),
        ]
    }
}

fn main() -> ExitCode {
    run(App)
}
```

## Model

- `build(ctx)` returns a fresh widget tree on every rebuild. Each element kind has a typed
  builder (`Indicator`, `ToggleRow`, …) from its constructor (`indicator()`, `toggle_row()`, …),
  with one method per prop taking a real type: strings, `bool`, `i64`/`f64`, `Option` where the
  wire value is nullable, and enums (`Severity`, `Tone`, `ClassName`, …) for the string unions.
  Structured props take small structs (`Fact`, `Choice`, `CalendarEvent`). Unset props are
  omitted, strings are capped as the host caps them, and `.key`, `.child` and `.children` work
  on every builder (`.text` on `label`, `indicator` and `button`). `build` still returns `Vec<Widget<Msg>>`; each builder
  converts with `.into()`. Handlers are typed per element (`on_toggle(bool)`, `on_press(u64)`,
  `on_select(usize)`, `on_change`, …) and return a message that `update` applies, because a
  handler cannot capture `&mut self`. `Urgency` names the `notify` levels: pass
  `Urgency::Low.as_str()` where `notify` takes an urgency.
- The runtime rebuilds after every host message and diffs the new tree against the mounted
  one, emitting one protocol commit per rebuild.
- A widget keeps its identity across rebuilds when its kind and key match in the same parent.
  Keyed rows that reorder emit `move`, and their controlled-edit sequence follows them. A kind
  change at a slot unmounts and remounts. A move across parents is a remove plus an insert.
- Controlled props (`value`, `active`, `open`, `selected`) always come from state. An event's
  `seq` is copied onto the confirming `set`; an event older than the last applied `seq` is
  dropped; a rejected edit still confirms, so the panel restores the state value.
- `Ctx` carries `name`, `state_dir`, `options`, `placement`, `popover_open` and `entry` (the
  applet's `AppletEntry`), plus `notify` and `copy`, which spawn `notify-send` and `wl-copy`
  directly, and the host requests `open_uri`, `launch` and `close_popover`.
- Work off the event path (a timer, a thread, a file watch) calls `ctx.waker().wake()`
  to schedule a rebuild; the waker is cloneable and sendable, and `ctx.wake_handle()` returns
  the same handle.
- An app may override `start(&mut self, ctx)`, which the runtime calls once after the host
  `hello`, with options and the state dir filled in. It is a one-shot setup hook: load saved
  state and start threads there, and keep `build` free of side effects. `ctx.load_state()`
  reads and deserializes `<state dir>/state.json` as any `DeserializeOwned` type and returns
  `None` when the file is missing or corrupt; `ctx.save_state(&value)` serializes any
  `Serialize` value to the same file and is a no-op when the state dir is empty.
- Nothing builds before the host `hello`. A panicking handler exits with code 1 and a tree
  over a protocol limit (or a panicking `build`) with code 70; a closed socket exits 0.

## Example and tests

`examples/kitchen_sink/` is its own crate depending on this one by path. It exercises every
element kind, every handler, host requests, persistence, a one-second timer,
and keyed reordering. Run it against an isolated second panel — never the session bar — as
described in `docs/live-testing.md`.

```
cargo test
```
