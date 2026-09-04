---
name: applet
description: Writing panel applets in glimpse-panel — the Applet trait, Ctx sources, pull-based indicators, the exhaustive registration match, and zone reconciliation in components/panel.rs. Use for any new applet, any change under crates/glimpse-panel/src/applets/, and any change to the framework in applet/mod.rs or applet/runtime.rs. Trigger on the location, not the wording — if the file is an applet or the framework under it, this applies. Indicator and IndicatorGroup internals belong to the widget skill; this covers what an applet hands them.
---

# applet

An applet owns exactly one `IndicatorGroup`, which renders 0..N `Indicator`s. Every applet's view is
therefore identical and only `Vec<IndicatorSpec>` varies, so an applet is a function from state to
that vector. It never opens a D-Bus connection, never holds a socket, and reaches nothing but the
daemon through `Ctx`.

**Verified against the tree at `crates/glimpse-panel/src/applet/` and `src/applets/`.** Every
signature below was read out of the current code. When this file and the code disagree, the code is
right and this file is a bug — fix it in the same change.

## The shape, in one screen

```rust
pub struct Heartbeat {
    count: Option<u64>,          // None renders nothing; 0 is a real value
    period_ms: u64,
    icon: gio::Icon,             // built once, not per render
}

impl Applet for Heartbeat {
    fn topics(&self) -> &'static [&'static str] {
        &[HeartbeatTick::NAME]           // declared, not subscribed; the runtime does that
    }

    fn start() -> Self {
        Self { count: None, period_ms: DEFAULT_PERIOD_MS, icon: gio::ThemedIcon::new(ICON).upcast() }
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) { ... }

    fn indicators(&self) -> Vec<IndicatorSpec> { ... }
}
```

Four methods, one with a default, no associated types, object-safe. The runtime stores `Box<dyn Applet>`; `start` stays
out of the vtable via `where Self: Sized` and is called from the registration match, where the
concrete type is still known.

```rust
pub enum Input {
    Topic(glimpse_ipc::Event),
    Pointer(Pointer),
    Tick,
}

pub enum Pointer { Press(Button), Scroll(Direction) }
pub enum Button { Left, Middle, Right, Other(u32) }   // Other carries the GDK code: back is 8
pub enum Direction { Up, Down, Left, Right }

impl Ctx {
    pub fn call<C: Command>(&self, args: C::Args);       // spawned, fire-and-forget
    pub fn interval(&self, period: Duration);            // delivers Input::Tick, wall-clock aligned
}

pub fn payload<T: Message>(event: &Event) -> Option<T::Payload>;
```

## Decision table

| Task | Go to |
| --- | --- |
| Adding an applet from nothing | `references/anatomy.md` |
| Something does not work and you want the symptom, not the theory | `references/pitfalls.md` |
| Writing or judging the tests | the `testing` skill |
| Anything inside `Indicator` / `IndicatorGroup` | the `widget` skill |
| Reaching the daemon, reconnects, subscription limits | the `ipc-client` skill |
| GTK4, libadwaita and relm4 craft in general | the `relm4` and `gtk4-styles` skills |
| Threading, widget boundaries, untrusted text | `.claude/rules/ui.md` |

## Rules that are not already loaded

`.claude/rules/ui.md` loads automatically for the GTK crates and carries the boundary rules: nothing
in a widget calls the socket or D-Bus, no `glib::timeout_add` to refresh from daemon data, a
`Controller` that is not stored is dropped, data changes must not shift layout, and hostile text is
capped before it reaches a label. They are not repeated here. What follows is what none of that says.

1. **Topics are declared, not subscribed.** `topics()` returns the names an applet wants; the
   runtime subscribes after `start` and `Ctx` owns the guards, so an applet holds none and `start`
   has no side effects. This mirrors `glimpse-services`, where `Live<S>` holds the guards and no service holds one
   (`grep SourceGuard crates/glimpse-services/src/services/` is empty). Blanket teardown is `Ctx`
   dropping with the runtime; a panicking applet is torn down by `ctx.shutdown()`, the same answer
   `ServiceRuntime::run` gives in its panic arm.

2. **A declared topic and the `payload::<T>` that decodes it are two halves of one fact.** Nothing
   checks that they agree — declare `HeartbeatTick::NAME` and decode `SolarStatus` and it compiles,
   then silently never matches. Name the topic through `T::NAME`, never as a string literal, so the
   two halves at least share a symbol.

3. **`indicators()` is a pull and must be cheap and total.** The runtime calls it after every
   `handle` and hands the result to `group.set_items`, which compares before writing. Build nothing
   expensive in it — hoist a `gio::Icon` into a field rather than constructing one per render.

4. **Return an empty `Vec` for "nothing to show".** The group hides itself, so an applet with no
   value yet occupies no space and creates no gap. This is what the 0..N contract is for; do not
   render a placeholder.

5. **Distinguish "no value" from a real zero.** `Option<u64>`, not `u64`. A counter that can be reset
   makes `0` reachable, and a plain integer renders it identically to having no data at all.

6. **A panic stops the applet permanently.** `handle` and `indicators` run inside one
   `catch_unwind`; a panic logs at `error`, drops the applet, calls `ctx.shutdown()` and empties the
   group. No further input reaches it. Unwinding past a `&mut self` mid-mutation leaves state nobody
   can reason about — the same reason `ServiceRuntime` stops a service rather than continuing.

7. **Prefer a symbolic icon.** Name the `-symbolic` variant explicitly —
   `audio-volume-high-symbolic`, not `audio-volume-high`. A symbolic icon is recoloured by the CSS
   `color` property, so it follows `@theme_fg_color` from `.indicator` and every theme and accent
   after it; a full-colour icon ignores all of that and reads as a foreign object on the bar,
   worst of all in dark mode. Do not rely on the icon theme falling back to a symbolic variant — ask
   for it.

   The exception is an icon another application supplied: a tray item's own icon name or its ARGB
   pixmap is theirs to choose, and it is rendered as given. `gdk::Texture` implements `gio::Icon`,
   which is why one `Option<gio::Icon>` covers a themed name, a file path and a raw pixmap.

8. **`ctx.interval(period)` is the only timer, and it aligns to the wall clock.** It delivers
   `Input::Tick`. The wait is the time since the epoch modulo the period, so a minute-long period
   fires at `:00` rather than wherever the panel happened to start — a `%H:%M` clock changing up to
   a minute late reads as broken, not as late. Calling it again **replaces** the timer rather than
   adding one, which is what makes it safe to ask for from `configure`, and `configure` runs on
   every configuration change. A zero period is refused and logged. Never reach for
   `glib::timeout_add` instead: `.claude/rules/ui.md` reserves that for animation.

9. **`ctx.call` is fire-and-forget and its reply is discarded.** Topics reconcile, and UI state never
   waits on a round trip. An applet that tracks a value it only ever *sets* can drift from the
   daemon; that is the accepted cost, and the case that will justify `ctx.ask` when one appears.

## What the framework will not do for you

- **There is no staleness and no `degraded`.** A dead daemon stops sending events and the last value
  stays on screen. `IndicatorState` was deleted from `glimpse-widgets`; do not reintroduce dimming.
- **Configuration is typed and validated at load, not by the applet.** `glimpse_config::Applet` is
  one applet's whole configuration: `common`, the settings every applet understands, and `kind`, an
  internally-tagged enum on `extends` carrying the settings this applet alone understands. A bad
  setting in either half is a load error naming the table and the key — the applet never
  deserializes anything and has no failure path. `configure(&mut self, ctx, config: &AppletConfig)`
  destructures its own variant off `kind`:
  `let AppletKind::Clock(cfg) = &config.kind else { return };`. The runtime compares the config
  before calling, so an unchanged one never reaches the applet.

  **`config.common` is read the same way by every applet**, and reading it is not optional where it
  applies: `tooltip_format` fills `IndicatorSpec.tooltip`, and `common.settings()` returns the
  label and argv for the row an applet's popover puts in its footer, or `None` when the user set
  neither. The loader guarantees they are set together, so an applet never handles a half-set pair.

  Adding settings to an applet means adding a config struct in `glimpse-config` and promoting the
  variant from `Clock {}` to `Clock(Clock)`. **Never write a unit variant** (`Clock`) —
  `deny_unknown_fields` has nothing to deny on one, so it silently swallows every key written under
  it. An empty struct variant refuses them.
- **There is no `Output`.** Commands leave through `ctx.call`; an empty group hides itself; nothing
  is reported to the panel.
- **An applet is not told about orientation, position or its monitor.** The panel sets orientation on
  the group directly. `Placement` returns with the first applet that needs the connector name.

## Definition of done

- The applet is one arm of the exhaustive `match` in `applets/mod.rs::build`, listing every unbuilt
  `Applet` variant explicitly rather than `_ => None`, so a new applet is a compile error here.
- `indicators()` returns chips in a stable order — `IndicatorGroup` reconciles by position, so a
  list whose order churns rewrites every chip instead of updating the one that changed.
- Text taken off a topic is capped before it reaches an `IndicatorSpec`. Tray titles, MPRIS metadata
  and SSIDs are attacker-controlled; `Indicator` truncates, but the cap belongs upstream too.
- Pure logic is split out of anything needing a `Ctx` so it can be tested headlessly — see the
  `testing` skill.
- `just verify` is clean. `just lint` runs `-D warnings`, and a framework item with no consumer is a
  build failure, not a note.
- `crates/glimpse-panel/README.md` says what changed, in the same commit.
