# Rust SDK

The `glimpse-sdk` crate. Its only dependencies are `serde` and `serde_json`. Elements and their props
are in [Elements](/applets/exec/reference/elements); this page covers how a Rust applet is put
together.

::: tip Create an applet

```sh
glimpsectl applet new --language rs --id me.aresa.GlimpseTodo --yes
cd glimpse-todo
just dev
```

[Start an applet](/applets/exec/start) explains each step and the files it creates.

:::

```rust
use glimpse_sdk::*;
use std::process::ExitCode;

struct Todo {
    done: bool,
}

enum Msg {
    Done(bool),
}

impl App for Todo {
    type Msg = Msg;

    fn start(&mut self, ctx: &Ctx) {
        self.done = ctx.load_state().unwrap_or(false);
    }

    fn update(&mut self, ctx: &Ctx, msg: Msg) {
        match msg {
            Msg::Done(done) => {
                self.done = done;
                let _ = ctx.save_state(&done);
            }
        }
    }

    fn build(&self, _ctx: &Ctx) -> Vec<Widget<Msg>> {
        vec![
            indicator().icon("view-list-symbolic").text("Todo").into(),
            popover()
                .title("Todo")
                .child(
                    section().child(
                        toggle_row()
                            .title("Buy groceries")
                            .active(self.done)
                            .on_toggle(Msg::Done),
                    ),
                )
                .into(),
        ]
    }
}

fn main() -> ExitCode {
    run(Todo { done: false })
}
```

## The applet

Implement `App` for your type. A handler cannot change `self`, so it returns a message and `update`
applies it.

| Item | What it does |
| --- | --- |
| `type Msg` | Your message type, usually an enum |
| `build(&self, ctx) -> Vec<Widget<Msg>>` | Returns the whole widget tree. Called after every change; keep it free of side effects. |
| `update(&mut self, ctx, msg)` | Applies a message from a handler. A rebuild follows. |
| `start(&mut self, ctx)` | Optional. Runs once after the panel's `hello`, with options and the state directory filled in. Load state and start threads here. |
| `run(app) -> ExitCode` | Runs the applet. |

Each element has a constructor (`toggle_row()`, `action_row()`, `label()`) and one builder method per
prop in snake_case (`.title()`, `.active()`, `.class_name(ClassName::Heading)`). Enum props take
typed enums. Handlers take a closure returning a message: `.on_toggle(Msg::Done)`,
`.on_activate(|| Msg::Save)`. `.child()`, `.children()` and `.key()` work on every builder, and
`.into()` turns a builder into a `Widget`.

## The context

```rust
pub struct Ctx {
    pub name: String,                     // this instance's name in the config
    pub state_dir: String,                // a directory for your files, mode 0700
    pub options: serde_json::Value,       // the config's options; updates without a restart
    pub placement: Option<serde_json::Value>, // output, position, orientation, zone, size
    pub popover_open: bool,               // whether the popover is showing
    pub entry: Option<AppletEntry>,       // the desktop entry
}

pub struct AppletEntry {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
}

impl Ctx {
    pub fn notify(&self, summary: &str, body: Option<&str>, icon: Option<&str>, urgency: Option<&str>);
    pub fn copy(&self, text: &str);
    pub fn open_uri(&self, uri: &str);
    pub fn launch(&self, id: &str);
    pub fn close_popover(&self);
    pub fn load_state<S: DeserializeOwned>(&self) -> Option<S>;
    pub fn save_state<S: Serialize>(&self, state: &S) -> std::io::Result<()>;
    pub fn wake(&self);
    pub fn waker(&self) -> Waker;
}
```

`notify` runs `notify-send` and `copy` runs `wl-copy`. `open_uri`, `launch` and `close_popover` ask
the panel; see [Notify, copy, open links and launch apps](/applets/exec/host). `load_state` reads
`state.json` from the state directory and returns `None` when it is missing or unreadable;
`save_state` writes it. A `Waker` can move to another thread, where `.wake()` asks for a rebuild.

## Exit codes

| Code | Cause |
| --- | --- |
| 0 | The panel closed the socket |
| 1 | A handler panicked, or `GLIMPSE_APPLET_SOCKET` is not set |
| 70 | The tree broke a protocol limit, or `build` panicked |
