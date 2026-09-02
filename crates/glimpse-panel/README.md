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
