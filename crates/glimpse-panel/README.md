# glimpse-panel

The panel: layer-shell bars, applets, popovers and notification popups.

Builds the binary named `glimpse`.

## Contents

- `main.rs` — GTK application, layer-shell setup, one bar per output across hotplug
- `app.rs` — one bar per (panel config × monitor), the shared `Client`, config and theme watches
- `components/panel.rs` — bar window, zones, applet reconciliation
- `applet/` — the applet framework: the trait, `Ctx`, the relm4 runtime
- `applets/` — one module per applet, plus the registration match
- `popups/` — notification popups, OSD _(pending)_

## Applets

An applet owns exactly one `IndicatorGroup`, which renders 0..N `Indicator`s, so every applet's view
is the same shape and only `Vec<IndicatorSpec>` varies. The trait is object-safe and the runtime
stores `Box<dyn Applet>`:

```rust
fn topics(&self) -> &'static [&'static str]   // declared; the runtime subscribes
fn start() -> Self
fn handle(&mut self, ctx: &Ctx, input: &Input)
fn indicators(&self) -> Vec<IndicatorSpec>
```

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

**Scroll reaches an applet as whole notches.** The `Indicator` emits raw deltas and a touchpad sends
many small ones; the runtime accumulates per (indicator, axis) and drains in whole units, so a wheel
detent is one notch and ten `0.4` deltas are four.

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
