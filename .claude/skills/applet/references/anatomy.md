# Adding an applet

Four edits. The kind must exist before anything else compiles.

## 1. The applet — `crates/glimpse-config/src/schema/applets.rs`

Add a variant to `Applet`, the internally-tagged enum on `extends`. Write `Thing {}` — an empty
struct variant — not `Thing`: `deny_unknown_fields` has nothing to deny on a unit variant, so one
silently swallows every setting written under it. Promote to `Thing(Thing)` when it gains settings,
with the config struct beside it.

Names are kebab-case through `rename_all`, and fields through `rename_all_fields`; without the
latter the crate's `every_key_and_enum_value_is_kebab_case` test fails. `Applet::from_name`
deserializes through those renames rather than matching strings, so a rename cannot leave a stale
arm behind.

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

    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletConfig::Thing(settings) = config else {
            return;
        };
        self.settings = settings.clone();
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

fn build(config: &AppletConfig) -> Option<Builder> {
    match config {
        AppletConfig::Thing {} => Some(|| Box::new(Thing::start())),
        AppletConfig::Audio {} | /* every applet not yet built */ => None,
    }
}
```

The closure captures nothing, so it coerces to `Builder = fn() -> Box<dyn Applet>`. That function
pointer is what keeps `applet/runtime.rs` from depending on `applets/` — the framework never names a
concrete applet. Bind it to a local before calling it: `init.build()` parses as a method call on
`AppletInit`, and the project forbids the comment that would explain `(init.build)()`.

**Never write `_ => None`.** The exhaustive match is what turns a new `Applet` variant into a compile
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
