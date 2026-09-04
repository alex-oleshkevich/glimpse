# glimpse-panel

The panel: layer-shell bars, applets, popovers and notification popups.

Builds the binary named `glimpse`.

## Contents

- `main.rs` — GTK application, layer-shell setup, one bar per output across hotplug
- `app.rs` — one bar per (panel config × monitor), the shared `Client`, config and theme watches
- `components/panel.rs` — bar window, zones, applet reconciliation
- `applet/` — the applet framework: the trait, `Ctx`, the relm4 runtime
- `applets/` — one module per applet, plus the registration match; `pager/` renders workspaces or
  windows into its own `Pager` widget rather than into indicators
- `popups/` — notification popups, OSD _(pending)_

## Applets

Most applets own exactly one `IndicatorGroup`, which renders 0..N `Indicator`s, so their view is the
same shape and only `Vec<IndicatorSpec>` varies. The trait is object-safe and the runtime stores
`Box<dyn Applet>`:

```rust
fn topics(&self) -> &'static [&'static str]   // declared; the runtime subscribes
fn start() -> Self
fn handle(&mut self, ctx: &Ctx, input: &Input)
fn view(&mut self, ctx: &Ctx) -> Option<gtk4::Widget>   // None: the runtime supplies the group
fn indicators(&self) -> Vec<IndicatorSpec>
```

## An applet may supply its own widget

`view()` returning `Some` replaces the group entirely, and `indicators()` is then never called. The
pager is the first case: it takes a click *per slot* over a list whose length changes, and
`IndicatorGroup` takes one click for the whole row. A graph or a strip will not be the last.

**The root is a `gtk4::Box` carrying the `applet` class, not the group.** relm4's `init_root()` takes
no arguments, so the root cannot depend on an applet that is built later in `init()`. The box is that
socket, and it gives every applet a uniform CSS hook the group-as-root never did.

**An applet that supplies a view receives no `Input::Pointer`.** It supplied the widget; the widget
owns its pointer, which is what lets the pager give each slot its own `Gtk.Button` and its own scroll
axes without fighting a controller the runtime installed over the top.

**Orientation is handed to the applet, not applied behind its back.** `orient` sets the box, then the
`IndicatorGroup` when there is one and `Applet::orient` otherwise. Reaching into the view's own
`BoxLayout` was tried first and is wrong: it turns the widget sideways without telling it, so the
widget cannot restyle for the new axis. Measured on the pager — a vertical bar stretched every dot
across the column and lost the shape that says which workspace is current, because the rule that
lengthens the active one is keyed on `min-width`. A widget has to know its axis to draw for it.

**Signals are wired in `view`, which is called once**, before the first `configure`. A GTK callback
outlives any `&Ctx`, so `ctx.caller()` hands out a `Caller` — name plus `Client`, cheap to clone —
carrying only `call`. Settings a callback needs at click time live behind an `Rc<Cell<_>>` the applet
updates in `configure`; capturing them by value would freeze them at wiring time.

**`ctx.output()` is the connector this bar is on**, `None` when the monitor has no name. It exists
for the pager's `scope = "output"` and is the `Placement` the applet skill said would arrive with the
first applet that needed it.

`indicators()` is a pull, called after every `handle`, and its result goes to `set_items`, which
compares before writing. An empty vector hides the group, which is how an applet says it has nothing
yet — never a placeholder.

**`Ctx` owns every source.** `topics()` is a declaration, not an action; the runtime subscribes and
holds the guards, so no applet holds one and `start` has no side effects. This is `Live<S>` in
`glimpse-services` — no service holds a `SourceGuard` either. Teardown is `Ctx` dropping with the
runtime, and a panicking applet gets `ctx.shutdown()`.

**A panic stops one applet, not the panel.** `handle` and `indicators` run inside one `catch_unwind`;
a panic logs, drops the applet, stops its sources and empties its group. Unwinding past a `&mut self`
mid-mutation leaves state nobody can reason about, which is why `ServiceRuntime` stops a service
rather than continuing with it.

**Scroll reaches an applet as whole notches.** The `IndicatorGroup` emits raw deltas and a touchpad
sends many small ones; the runtime accumulates per axis and drains in whole units, so a wheel detent
is one notch and ten `0.4` deltas are four. The accumulator belongs to the group, not to a chip, so
it survives a re-render — an applet whose indicators change mid-gesture does not lose the remainder.

**Pointer input names no indicator.** The whole group is one clickable target, so `Input::Pointer`
carries only the button or direction. An applet that renders three chips is still one thing to
click, which is what an applet is from the outside.

**Zone reconciliation is keyed by `(zone, name)`.** `MonitorsChanged` and `ThemeChanged` both reach
`reconcile_panels`, so the guard comparing the desired key sequence against the current one is what
stops every applet being rebuilt on every theme write. A name with no implementation still occupies a
`Slot` with `handle: None` — that is what keeps the sequences comparable, and skipping such names
instead makes the guard never hold.

An unresolvable *name* is a user typo and is logged at `warn`; a name that resolves to a kind nothing
implements is expected and is `debug`. The shipped default config names nineteen applets, so
collapsing the two severities means nineteen warnings on an untouched installation.

There is deliberately no staleness, no `degraded`, no per-applet configuration, no timer and no
applet `Output`. A dead daemon stops delivering events and the last value stays on screen.

## The popover

`Applet::popover(&Seat)` builds the tree on open and the runtime drops it on close. Nothing is
cached, so a dismissed popover leaves no widget tree behind and the next open starts from the
current session rather than from wherever the last one stopped.

An open popover still follows events: the applet keeps a `glib::WeakRef` to what it built and
pushes every render into it. Weak on purpose — a strong reference would hold the tree alive past
dismissal, which is the thing destroy-on-close exists to prevent.

### It is not a `Gtk.Popover`

`applet/catcher.rs` holds the container: a second layer surface, anchored to all four edges of the
panel's own monitor, covering every part of it the bars have not reserved, and mapped only while a
popover is up. The applet's tree goes inside it. This is the pattern eww and AGS both settle on, and
the panel takes it for one reason: **a `Gtk.Popover` on a layer surface cannot be dismissed by a
click on another application.** That dismissal is `xdg_popup.grab`, `autohide` is how GTK asks for
it, and the grab costs the keyboard — measured, `KeyboardMode::OnDemand` plus `autohide(true)`
leaves `focused-window` at `None` for as long as the popover is open, which is the focus theft the
previous generation was reported for. Owning the surface buys outside-click dismissal, one popover
at a time and an exit animation, and costs hand-rolled placement and a drawn arrow.

One catcher per panel, shared by every applet on it (`Panel` holds the `Rc` and hands a clone to
each `AppletHandle::launch`). **One popover at a time is therefore structural** rather than a rule
someone has to enforce: there is one slot, and `open` tears down whatever was in it.

**`KeyboardMode::None`, so nothing is ever taken from the focused window.** The cost is that
`Escape` dismisses nothing — there is no keyboard to hear it.

### Dismissal

A `GestureClick` on the catcher window `pick`s the press and closes unless it landed inside the
content. Because the surface covers the output, a click anywhere else on the desktop is a press on
*us*, which is the whole point: the event exists to react to, where under a popover it did not.

`open` takes the dismissal callback, so only the applet that owns the current popover hears about
it. Registering one listener per applet for the life of the panel was the earlier shape and leaked:
an applet removed by a config change left its closure — and its `Sender` — in the catcher forever.

The runtime asks `Catcher::holds` before acting on anything. `shown` alone is not enough, because a
replaced applet still holds its handle until the queued `PopoverDismissed` reaches it, and in that
window a press on it would otherwise close *someone else's* popover.

### Placement

`Applet::anchor` names a widget inside the view; the runtime turns it into a centre coordinate in
panel-window space with `compute_bounds`, along whichever axis the panel's edge implies. An applet
that names nothing anchors to its whole box, which is already right for a single indicator.

`placement()` is the whole arithmetic, and it is a free function so it can be asserted without a
display. It returns two numbers — where the body starts, and where the arrow sits inside it — and
holds four properties, one test each:

- the arrow's centre is the pressed item's centre;
- the body never leaves the output;
- the body keeps a **gutter** from the output edge rather than sitting flush in the corner;
- the arrow never sits on the body's rounded corner.

**The gutter yields to the arrow, and that ordering is the design.** A 418px popover anchored 28px
from the edge cannot both keep a gutter and put its arrow over the item — the arrow would have to
start inside the corner radius. Clamping to zero and letting the arrow win was the first version and
looked broken; a fixed gutter was the second and slid the arrow 17px off the item. So the gutter is
`arrow`, shrunk to whatever still lets the arrow reach: centred items get the full gutter, and only
an item closer to the edge than `arrow × 2.5` gives any of it up. Both mutations — a gutter that
never yields, and no gutter at all — fail a test.

One CSS length drives all three of the arrow's size, its inset from the corner and the body's
gutter, because it is read back from the measured arrow. That is deliberate: they are proportional
to each other visually, and none of them is written in Rust.

### The animation is `AdwTimedAnimation`, not a CSS transition

The fade is `opacity` on the slot, driven by `adw::TimedAnimation` with a
`PropertyAnimationTarget`. Measured in a nested niri: `0 → 0.702 at 58ms → 1.000 at 158ms` opening,
`0.961 → 0.157 at 50ms → 0.000 at 148ms` closing, then `dismissed` and the child unparented.

A CSS `transition: opacity` on the same node was tried first and did not animate. Two properties of
libadwaita's animation are the reason not to go back: `done` is an exact clock, so the teardown
timer and its duplicated duration constant are gone; and an unmapped widget or
`gtk-enable-animations: false` makes `play()` skip straight to the end and emit `done` synchronously,
which is what lets the state machine be one path instead of two.

**A layer surface has no size and is not mapped until the compositor configures it.** An idle right
after `present()` measures `width=0` on an unmapped widget: nothing to centre against, and GTK skips
animating what is not on screen. `open` waits on a tick callback for a real allocation, settles, and
only then plays. This one fact caused three separate bug reports — popover at the screen edge, no
arrow, no animation — before it was found.

**The shadow is in `px`, and it has to be.** `box-shadow` with `rem` lengths renders **nothing** in
GTK4 — measured both ways with an opaque spread — and it fails silently, so the symptom is a popover
with no shadow and no diagnostic. `.claude/rules/ui.md` carries the rule; this is the change that
found it.

**The arrow is a `Gtk.DrawingArea`, not a rotated box.** GTK4 has no triangle, and a square with
`transform: rotate(45deg)` overflows its allocation into the bar. Four lines of cairo point it at
whichever edge the panel is on; its size and colour still come from CSS
(`.applet-popover__arrow`, `--sideways` for a vertical panel), and the fill reads
`gtk_widget_get_color`, so no colour is written in Rust.

**The catcher never computes where the bar ends — it takes `set_exclusive_zone(0)` and lets the
compositor place it.** This is the whole of its vertical positioning: no margin, no thickness, no
measurement of the bar.

Everything else was tried and is wrong. `set_exclusive_zone(-1)` plus a top margin of `config.size`
assumes two things that are both false. `Panel::set_thickness` calls `set_size_request`, a
**minimum**, so a bar whose applets need more room is taller than the configured number. And, worse,
the panel is not necessarily at the top of the output at all: anything else holding an exclusive
zone pushes it down. Measured on a session also running the previous generation — legacy bar at
logical `0..36`, this panel at `36..72` — a catcher margined by `36` put the popover at `y = 36`,
directly behind the panel's own bar. The 9px arrow was completely hidden and only the shadow, which
falls downward, escaped. That is a popover that looks like it has no arrow and sits in the wrong
place, and no arithmetic over `config.size` can fix it, because the missing number is the sum of
every *other* surface's exclusive zone.

`exclusive_zone(0)` means "reserve nothing, but respect what others reserved". The catcher's content
area is then exactly the free region under every bar, its origin lines up with the panel's own along
the anchor axis, and `room` in `placement()` is the usable extent rather than the whole output.

**A position change closes an open popover.** The anchor is a coordinate on one axis, and the applet
is the only thing that can recompute it for the other; re-placing a `Top` popover's x as a `Left`
popover's y puts it somewhere arbitrary. Closing is honest, and the next press reopens it correctly.

**Every panel position is a different layout.** The catcher takes the panel's `Position` and derives
the slot's orientation, which side the arrow sits on, which way it points, and which axis
`placement` works along. `Top` is verified on a live session; `Bottom` and `Left` were verified in a
nested compositor and `Right` not at all.

**Nothing asserts the state machine.** `placement` is a free function precisely so the arithmetic
can be tested headlessly, and the six tests beside it are the whole of the automated coverage.
Opening, the fade, dismissal and the side-change teardown all need a mapped layer surface, which a
test must not create on the user's session — they are checked by running the panel and reading the
screen, and they are not covered.

## Reconciliation settles every slot, on both paths

`reconcile_applets` has two: one for a config change that left the applet list alone, and one that
rebuilds it. A slot that survives a rebuild is moved across as it stands, so the rebuild path used
to append it without ever handing it the new configuration — an edit that added an applet *and*
changed another applet's settings applied only the first half, and the surviving applet kept the
settings it started with. Both paths now go through one `settle`, which orients the handle and
hands it its configuration; the runtime compares before writing, so settling a slot that was just
launched with that same configuration costs nothing.

Two loops doing almost the same thing is what let them drift, so the shape is the fix: there is one
place to change and no way to change half of it.

## Rules

An applet renders topics and sends commands. It never opens a D-Bus connection, never reaches a
backend directly, and holds no state that outlives its own widget.

UI state never waits on a round trip. A slider updates its own widget immediately and sends the
command; the topic event that follows is reconciliation. This is safe because topics are state
cells — the daemon's value always wins and the panel cannot drift.

Update properties on existing widgets. Rebuilding widget trees per event is the most likely source
of visible stutter.

A bar's identity is its position in the `panels` array paired with the monitor's connector name;
everything else — position, size, and the monitor object itself — is reconfigured in place. A
monitor GDK cannot name has no stable identity, so it gets no bar rather than one that reconcile
cannot find again. Repointing a mapped layer surface at another output remaps it, so `set_monitor`
is called only when the requested output actually changed.

A surface that must exist once per session — the notification popup stack — is owned by an elected
bar, not by every bar. An unbound bar never owns it.

CSS providers are installed once and reloaded in place; installing twice stacks every rule. Every
provider connects `parsing-error`, because GTK4's loaders return nothing and a malformed stylesheet
is otherwise silent.

A programmatic state change must not re-emit its signal, or the handler that sends the command
re-enters itself.

A dead daemon is a normal state: events stop arriving, the last value stays on screen, and
reconnection restores everything with no special handling. The panel does not dim or annotate a
value whose producer is gone. The connection is opened with `Client::open`, so a panel started
before `glimpsed` waits for it rather than failing; the task that watches the connection state is
what owns the client, because the connection stops when the last handle drops and no widget has a
topic to read yet.

Configuration is the `[panel]` table of the shared `config.toml`, plus `panel.css`. Tables owned
by other binaries are ignored, not validated. Schema in.
