# Adding an applet

Four edits. The kind must exist before anything else compiles.

## 1. The applet — `crates/glimpse-config/src/schema/applets.rs`

Add a variant to `Kind` (exported as `AppletKind`), the internally-tagged enum on `extends`. Write `Thing {}` — an empty
struct variant — not `Thing`: `deny_unknown_fields` has nothing to deny on a unit variant, so one
silently swallows every setting written under it. Promote to `Thing(Thing)` when it gains settings,
with the config struct beside it.

Names are kebab-case through `rename_all`, and fields through `rename_all_fields`; without the
latter the crate's `every_key_and_enum_value_is_kebab_case` test fails. `Applet::from_name`
deserializes through those renames rather than matching strings, so a rename cannot leave a stale
arm behind.

Then regenerate the shipped documents:

```bash
just gen-config-default
just gen-config-schema
just gen-config-commented
```

`schema/tests.rs` asserts `data/config.default.toml`, `data/config.commented.toml` and
`data/config.schema.json` equal the compiled types. Skipping this fails `just test` with a diff that
looks unrelated to the change.

Only add the name to `Panel::default()` if the applet should ship on by default. A name in the
default zones that does not resolve is an `unknown applet` load error on an untouched installation,
which is why `every_name_in_the_default_zones_resolves_to_a_kind`, in
`applets/tests/configured.rs`, exists.

## 2. The module — `crates/glimpse-panel/src/applets/<name>.rs`

`heartbeat.rs` is the smallest real applet; this is its shape with a setting added:

```rust
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_services::ThingHandle;
use glimpse_widgets::IndicatorSpec;

use crate::applet::{Applet, Button, Ctx, Input, Pointer, spawn_command};

pub struct Thing {
    service: ThingHandle,
    settings: ThingConfig,
    value: Option<u64>,
}

impl Applet for Thing {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Thing(settings) = &config.kind else {
            return;
        };
        self.settings = settings.clone();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => self.value = Some(self.service.snapshot().value),
            Input::Pointer(Pointer::Press(Button::Middle)) => {
                let service = self.service.clone();
                spawn_command("thing.reset", async move { service.reset().await });
            }
            Input::Pointer(_) | Input::Tick => {}
        }
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        let Some(value) = self.value else {
            return Vec::new();
        };
        vec![IndicatorSpec {
            label: Some(value.to_string()),
            ..Default::default()
        }]
    }
}

impl Thing {
    pub fn start(service: ThingHandle) -> Self {
        let value = service.snapshot().value;
        Self {
            service,
            settings: ThingConfig::default(),
            value: Some(value),
        }
    }
}
```

`Thing`, `ThingHandle` and `ThingConfig` stand for the new names. The watch is declared in the
registration closure, not by the applet: `ctx.watch(handle.subscribe())` runs before `start`'s value
is boxed, so `start` is pure construction and holds no guard. Every change arrives as `Input::Woken`
and the applet reads `handle.snapshot()` itself. Left click is left to the runtime, which opens the
popover.

`Input::Pointer(_)` as a final arm is deliberate — middle click, right click and horizontal scroll
all land there, and an applet that wants none of them says so once. Pointer input names no
indicator: the whole `IndicatorGroup` is one clickable target, so an applet rendering three chips is
still one thing to click.

## 3. Registration — `crates/glimpse-panel/src/applets/mod.rs`

```rust
mod thing;

pub fn build(
    config: &AppletConfig,
    handles: &crate::services::Handles,
    placement: glimpse_services::Placement,
    dialog: Option<&relm4::Sender<crate::app::AppInput>>,
) -> Option<Builder> {
    match &config.kind {
        AppletKind::Thing(_) => {
            let thing = handles.thing.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(thing.subscribe());
                Box::new(thing::Thing::start(thing))
            }))
        }
        // one arm for every other AppletKind variant
    }
}
```

**Reference an applet through its module — `thing::Thing`, `clock::Clock` — never
`use thing::Thing;` and then a bare `Thing`.** An applet with more than one file is a directory
module whose `mod.rs` re-exports from private submodules:

```rust
mod indicator;
mod popover;

pub use indicator::Clock;
```

The submodule names repeat across applets — `indicator`, `popover`, `render` — and none of them
means anything without the applet's module in front. The qualifier is what keeps the
registration match readable as the list grows, and it keeps the submodule layout an internal detail
rather than part of the path a caller types.

The closure moves the cloned handles in and is boxed as `Builder = Box<dyn FnOnce(&Ctx) -> Box<dyn
Applet>>`. That type is what keeps `applet/runtime.rs` from depending on `applets/` — the framework
never names a concrete applet. The runtime binds it to a local before calling it: `init.build(&ctx)`
parses as a method call on `AppletInit`, and the project forbids the comment that would explain
`(init.build)(&ctx)`.

**Never write `_ => None`.** The exhaustive match is what turns a new `AppletKind` variant into a
compile error here instead of an applet that silently never appears. An arm returns `None` only when
a dependency is missing, as `Session` does without a `dialog`.

## 4. Reaching the screen

Nothing else. `components/panel.rs` resolves each name in `left`/`center`/`right` through
`applets::configured`, builds it with `applets::build` and launches an `AppletHandle` per
`(zone, name, kind)`. Add the name to a scratch config to see it:

```toml
[[panels]]
right = ["thing"]
```

## Multi-instance and multi-monitor

One applet instance exists per `(zone, name)` per bar, and there is one bar per monitor. Three
monitors means three instances of the same applet, each with its own `Ctx` and its own
`handle.subscribe()`. That is correct — each renders its own bar — and it costs nothing on the
service, which every instance shares in-process through one handle.

The same name in two different zones is two independent applets. The same name twice in *one* zone
is a config mistake: the reconcile keys on `(zone, name, kind)`, so the duplicate collapses and one
instance is relaunched on the next rebuild. Harmless, but not something to rely on.
