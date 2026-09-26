---
name: applet
description: Writing panel applets in glimpse-panel — the Applet trait, Ctx sources, pull-based indicators, the popover an applet opens on left click, the exhaustive registration match, and zone reconciliation in components/panel.rs. Use for any new applet, any new or changed applet popover, any change under crates/glimpse-panel/src/applets/, and any change to the framework in applet/mod.rs, applet/runtime.rs, applet/popover.rs or applet/catcher.rs. Trigger on the location, not the wording — if the file is an applet or the framework under it, this applies. Indicator, IndicatorGroup and PopoverShell internals belong to the widget skill; this covers what an applet hands them.
---

# applet

An applet owns exactly one `IndicatorGroup`, which renders 0..N `Indicator`s. Every applet's view is
therefore identical and only `Vec<IndicatorSpec>` varies, so an applet is a function from state to
that vector — and, when it has a popover, from the same state to a second surface the runtime opens
on left click. It never opens a D-Bus connection, never holds a socket, and reaches nothing but the
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
    fn start() -> Self {                 // called from the registration match, not the vtable
        Self { count: None, period_ms: DEFAULT_PERIOD_MS, icon: gio::ThemedIcon::new(ICON).upcast() }
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) { ... }

    fn indicators(&self) -> Vec<IndicatorSpec> { ... }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> { ... }   // None: no popover
}
```

`start` and `handle` are the only two without a default, and there are no associated types, so the
trait is object-safe. The runtime stores `Box<dyn Applet>`; `start` stays out of the vtable via
`where Self: Sized` and is called from the registration match, where the concrete type is still
known. `configure`, `view`, `orient` and `anchor` are the remaining defaulted methods.

```rust
pub enum Input {
    Pointer(Pointer),
    Tick,
    Woken,          // the watched state moved, or a popover asked to be re-dressed
}

pub enum Pointer { Press(Button), Scroll(Direction) }
pub enum Button { Left, Middle, Right, Other(u32) }   // Other carries the GDK code: back is 8
pub enum Direction { Up, Down, Left, Right }

impl Ctx {
    pub fn call<C: Command>(&self, args: C::Args);       // spawned, fire-and-forget
    pub fn interval(&self, period: Duration);            // delivers Input::Tick, wall-clock aligned
    pub fn opener(&self) -> Opener;                      // wake, open, close
}

impl Seat {                                              // what popover() gets instead of a Ctx
    pub fn caller(&self) -> Caller;                      // Clone + 'static: capturable by a closure
    pub fn opener(&self) -> Opener;
}

pub fn payload<T: Message>(event: &Event) -> Option<T::Payload>;
```

## Decision table

| Task | Go to |
| --- | --- |
| Adding an applet from nothing | `references/anatomy.md` |
| Giving an applet a popover, or changing one it has | `references/popovers.md` |
| Something does not work and you want the symptom, not the theory | `references/pitfalls.md` |
| Writing or judging the tests | the `testing` skill |
| Anything inside `Indicator` / `IndicatorGroup` / `PopoverShell` | the `widget` skill |
| Reaching the daemon, reconnects, subscription limits | the `ipc-client` skill |
| GTK4, libadwaita and relm4 craft in general | the `relm4` and `gtk4-styles` skills |
| Threading, widget boundaries, untrusted text | `.claude/rules/ui.md` |

## Rules that are not already loaded

`.claude/rules/ui.md` loads automatically for the GTK crates and carries the boundary rules: nothing
in a widget calls the socket or D-Bus, no `glib::timeout_add` to refresh from daemon data, a
`Controller` that is not stored is dropped, data changes must not shift layout, and hostile text is
capped before it reaches a label. They are not repeated here. What follows is what none of that says.

1. **State arrives as one typed watch, and `Input::Woken` is the whole notification.** The
   registration closure calls `ctx.watch(handle.subscribe())`; every change becomes `Input::Woken`,
   and the applet reads `handle.snapshot()` from its own arm. The payload is not carried, so there
   is nothing to decode and nothing to name. `Ctx` owns the watch guard, so an applet holds none and
   `start` has no side effects. Blanket teardown is `Ctx` dropping with the runtime; a panicking
   applet is torn down by `ctx.shutdown()`, the same answer `ServiceRuntime::run` gives in its panic
   arm.

2. **An applet that does not match `Input::Woken` never updates.** It is also what a popover sends
   through `opener.wake()`, so the same arm serves both and there is one refresh path rather than
   two.

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

7. **An indicator is an icon, and only an icon.** A device name, a count, a status word — none of
   them belong on the bar; they belong in the tooltip. The exceptions are applets whose value *is*
   the content and which were asked for as such: the clock's time, the weather's reading, the
   keyboard's layout badge, mpris's configured `label_format`. Anything else wanting a label needs
   the user to ask for it first.

8. **Prefer a symbolic icon.** Name the `-symbolic` variant explicitly —
   `audio-volume-high-symbolic`, not `audio-volume-high`. A symbolic icon is recoloured by the CSS
   `color` property, so it follows `@theme_fg_color` from `.indicator` and every theme and accent
   after it; a full-colour icon ignores all of that and reads as a foreign object on the bar,
   worst of all in dark mode. Do not rely on the icon theme falling back to a symbolic variant — ask
   for it.

   The exception is an icon another application supplied: a tray item's own icon name or its ARGB
   pixmap is theirs to choose, and it is rendered as given. `gdk::Texture` implements `gio::Icon`,
   which is why one `Option<gio::Icon>` covers a themed name, a file path and a raw pixmap.

   **An icon the state decides moves with the text it belongs to.** The scan row's plus becomes a
   stop while scanning, set from Rust beside the label, because a blueprint can only declare the
   resting look. **An icon never takes a background and never takes the accent.** A chip behind a
   glyph shrinks it and makes a row read as a button; GNOME's Quick Settings does that because its
   menu is detached from the tile that opened it, and ours is not. An icon name the theme does not
   have is invisible, so check it — `find /usr/share/icons/Adwaita -name 'bluetooth*'` — before
   shipping a hardcoded one; `IconTheme::has_icon` only guards the names chosen at runtime.

   **A row that expands says so.** Its chevron rotates over `--gl-duration` / `--gl-ease` — the icon
   has to be directional, because a rotated symmetrical glyph reads as nothing — and the open row
   keeps full opacity while everything it is read against recedes. A row's detail is an
   `$Expandable`, and `PopoverShell` does the receding, the accordion and click-outside-to-close
   for it; see `references/popovers.md`. An audio stream's volume slider is not a detail and stays a
   plain revealer, because dimming the popover around a slider would be wrong.

9. **`ctx.interval(period)` is the only timer, and it aligns to the wall clock.** It delivers
   `Input::Tick`. The wait is the time since the epoch modulo the period, so a minute-long period
   fires at `:00` rather than wherever the panel happened to start — a `%H:%M` clock changing up to
   a minute late reads as broken, not as late. Calling it again **replaces** the timer rather than
   adding one, which is what makes it safe to ask for from `configure`, and `configure` runs on
   every configuration change. A zero period is refused and logged. Never reach for
   `glib::timeout_add` instead: `.claude/rules/ui.md` reserves that for animation.

10. **`ctx.call` is fire-and-forget and its reply is discarded.** Topics reconcile, and UI state never
   waits on a round trip. An applet that tracks a value it only ever *sets* can drift from the
   daemon; that is the accepted cost, and the case that will justify `ctx.ask` when one appears.

11. **The runtime owns left click, and `popover()` is how an applet answers it.** `HostInput::Pressed`
    delivers the press to `handle` and then toggles the popover — opening it calls `popover(&seat)`,
    pressing again closes what is open. An applet that *also* acts on
    `Pointer::Press(Button::Left)` fires that action every time its popover opens. There is one
    `Catcher` per bar, so one popover is open at a time per monitor and it receives no keyboard
    input at all.

12. **A live popover is held as a `glib::WeakRef` and dressed from the same method that builds the
    chips.** Every open builds a fresh widget; the strong references are the catcher's child and the
    runtime's handle, both dropped on dismissal, so `upgrade()` returning `None` *is* the closed
    state and no `is_open` flag exists to fall out of step. One `refresh()` sets
    `self.spec` and dresses the popover if it is up — two paths is how a popover comes to contradict
    the chip above it. Never hold a strong reference: the widget then outlives its dismissal and the
    next open builds a second one.

13. **A popover talks back by waking, not by calling.** A signal closure has no `&mut self`, so it
    captures `seat.opener()` and calls `wake()`, which arrives as `Input::Woken` and ends in
    `refresh`. `handle` must match `Input::Woken` — an applet matching only `Topic` and `Tick`
    swallows it and the popover never updates. Commands go through `seat.caller()`, which is
    `ctx.call` under another name; connect every signal in `popover()` and never in the dressing
    method, because that one runs on every event and handlers stack.

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
- **The panel calls `Applet::place` with `Placement` for position, orientation, zone, size and output.**
  Group applets receive orientation on their group directly.
- **Nothing in the panel scrolls, and nothing caps a popover's height.** `PopoverShell` does not and
  the catcher does not; the crate's one `Gtk.ScrolledWindow` propagates its natural height and so
  does not either. A popover stays on the screen
  because the applet bounds what it hands over — `upcoming`, `days` and `hours` are counts in the
  config, and the remainder goes behind a drawer. The catcher *does* re-place a popover whose
  measurement changes while it is open, so an applet never re-derives placement — but that
  re-placement is motion under the pointer, which is why content grows downwards and why a card's
  width is pinned by `max-width-chars` rather than left to the widest string.
- **No popover state survives a close.** Each open builds a new widget, so anything that must persist
  — the month a calendar was left on, the entry the bar had chosen — is the applet's field, not the
  widget's.

## Definition of done

- The applet is one arm of the exhaustive `match` in `applets/mod.rs::build`, listing every unbuilt
  `Applet` variant explicitly rather than `_ => None`, so a new applet is a compile error here.
- `indicators()` returns chips in a stable order — `IndicatorGroup` reconciles by position, so a
  list whose order churns rewrites every chip instead of updating the one that changed.
- Text taken off a topic is capped before it reaches an `IndicatorSpec`. Tray titles, MPRIS metadata
  and SSIDs are attacker-controlled; `Indicator` truncates, but the cap belongs upstream too.
- Pure logic is split out of anything needing a `Ctx` so it can be tested headlessly — see the
  `testing` skill. For an applet with a popover that is a `render.rs` beside it, holding every
  formatting function over `now` and the payload, with no GTK in the signatures.
- An applet with a popover reads `common.settings()` into an `Option<(String, Vec<String>)>`, hands
  the label to `set_footer` while dressing, and connects `connect_footer_activated` to
  `popover::run` once, at build.
- Every user-visible string in the popover was formatted by the applet, except a placeholder
  wording, which lives in the `.blp` — nothing in this tree translates a Rust string reaching a
  label.
- `just verify` is clean. `just lint` runs `-D warnings`, and a framework item with no consumer is a
  build failure, not a note.
- `crates/glimpse-panel/README.md` says what changed, in the same commit.
