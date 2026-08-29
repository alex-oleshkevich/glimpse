# Adding an applet

Four edits. The kind must exist before anything else compiles.

## 1. The kind — `crates/glimpse-config/src/schema/applets.rs`

Add a variant to `Kind`. Names are kebab-case through `#[serde(rename_all = "kebab-case")]`, and
`Kind::from_name` deserializes through that rename rather than matching strings, so a variant rename
cannot leave a stale arm behind.

Then regenerate both shipped documents:

```bash
just gen-config-default
just gen-config-schema
```

`schema/mod.rs` asserts `data/config.default.toml` and `data/config.schema.json` equal the compiled
types. Skipping this fails `just test` with a diff that looks unrelated to the change.

Only add the name to `Panel::default()` if the applet should ship on by default. A name in the
default zones that the panel cannot resolve is a warning on every start of an untouched
installation, which is why `every_applet_named_by_the_default_panels_resolves` exists.

## 2. The module — `crates/glimpse-panel/src/applets/<name>.rs`

```rust
use glimpse_contracts::{SomeCommand, SomeTopic};
use glimpse_widgets::IndicatorSpec;

use glimpse_contracts::Message;

use crate::applet::{Applet, Button, Ctx, Input, Pointer, payload};

const INDICATOR: &str = "value";

pub struct Thing {
    value: Option<Value>,
}

impl Applet for Thing {
    fn topics(&self) -> &'static [&'static str] {
        &[SomeTopic::NAME]
    }

    fn start() -> Self {
        Self { value: None }
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) {
        match input {
            Input::Topic(event) => {
                if let Some(payload) = payload::<SomeTopic>(event) {
                    self.value = Some(payload.value);
                }
            }
            Input::Pointer { pointer: Pointer::Press(Button::Left), .. } => {
                ctx.call::<SomeCommand>(SomeCommand {})
            }
            Input::Pointer { .. } => {}
        }
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        let Some(value) = self.value.as_ref() else {
            return Vec::new();
        };
        vec![IndicatorSpec {
            id: INDICATOR.to_owned(),
            label: Some(value.to_string()),
            ..Default::default()
        }]
    }
}
```

`topics()` is a declaration: the runtime subscribes after `start`, so `start` is pure construction.
Name topics through `T::NAME` rather than string literals — nothing checks that a declared topic and
the `payload::<T>` decoding it agree, so sharing the symbol is the only link there is.

`Input::Pointer { .. }` as a final arm is deliberate — middle click, right click and horizontal
scroll all land there, and an applet that wants none of them says so once.

## 3. Registration — `crates/glimpse-panel/src/applets/mod.rs`

```rust
mod thing;
use thing::Thing;

fn build(kind: AppletKind) -> Option<Builder> {
    match kind {
        AppletKind::Thing => Some(|| Box::new(Thing::start())),
        AppletKind::Audio | /* every kind not yet built */ => None,
    }
}
```

The closure captures nothing, so it coerces to `Builder = fn() -> Box<dyn Applet>`. That function
pointer is what keeps `applet/runtime.rs` from depending on `applets/` — the framework never names a
concrete applet. Bind it to a local before calling it: `init.build()` parses as a method call on
`AppletInit`, and the project forbids the comment that would explain `(init.build)()`.

**Never write `_ => None`.** The exhaustive match is what turns a new `AppletKind` into a compile
error here instead of an applet that silently never appears.

## 4. Reaching the screen

Nothing else. `components/panel.rs` resolves each name in `left`/`center`/`right` through
`applets::resolve` and launches an `AppletHandle` per `(zone, name)`. Add the name to a scratch
config to see it:

```toml
[[panels]]
right = ["thing"]
```

## Multi-instance and multi-monitor

One applet instance exists per `(zone, name)` per bar, and there is one bar per monitor. Three
monitors means three instances of the same applet, each with its own `Ctx` and its own subscription.
That is correct — each renders its own bar — and it costs nothing on the daemon, because the client
registers one pattern however many subscribers share it.

The same name in two different zones is two independent applets. The same name twice in *one* zone
is a config mistake: the reconcile keys on `(zone, name)`, so the duplicate collapses and one
instance is relaunched on the next rebuild. Harmless, but not something to rely on.
