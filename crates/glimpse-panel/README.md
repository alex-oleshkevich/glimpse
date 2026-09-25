# glimpse-panel

The panel: layer-shell bars, applets and popovers. Builds the binary named `glimpse-panel`.

## Contents

- `main.rs` — GTK application, layer-shell setup, one bar per output across hotplug
- `app.rs` — one bar per (panel config × monitor), the local service graph, config and theme watches
- `services.rs` — the panel-local composition root and its explicit service dependencies
- `components/panel.rs` — bar window, zones, applet reconciliation
- `applet/` — the framework: the trait, `Ctx`, the relm4 runtime, the popover catcher, and
  `popover::launch`, which spawns an argv with no shell; `popover::run` is the `settings-command`
  every footer row offers, launched and logged on failure
- `applets/` — one module per applet plus the registration match; `agenda.rs` holds what the clock
  and next-event applets both need to say about a calendar entry

## Applets

Most applets own one `IndicatorGroup`, so their view is the same shape and only
`Vec<IndicatorSpec>` varies. The trait is object-safe; the runtime stores `Box<dyn Applet>`:

```rust
fn handle(&mut self, ctx: &Ctx, input: &Input)
fn view(&mut self, ctx: &Ctx) -> Option<gtk4::Widget>   // None: the runtime supplies the group
fn indicators(&self) -> Vec<IndicatorSpec>
```

An applet builder captures its own service handle from `Ctx`: it seeds from `handle.snapshot()`,
then `Ctx::watch(handle.subscribe())` turns later changes into `Woken` inputs, and `Ctx` owns the
forwarding task so removing the applet cancels the subscription. **A panic stops one applet, not
the panel**: `handle` and `indicators` run inside one `catch_unwind`, which logs, drops the applet,
stops its sources and empties its group — unwinding past a `&mut self` mid-mutation would leave
state nobody can reason about.

**`indicators()` is a pull**, called after every `handle` and fed to `set_items`, which compares
before writing; an empty vector is how an applet says it has nothing yet, never a placeholder.
Scroll reaches an applet as whole notches, accumulated per axis so a touchpad's small deltas do not
lose their remainder mid-gesture; pointer input names no indicator, so the whole group is one
clickable target.

**Zone reconciliation is keyed by `(zone, name, kind)`**, which is what stops every applet being
rebuilt on every theme write; a name with no implementation still occupies a `Slot` with
`handle: None`, keeping the key sequences comparable. There is deliberately no staleness, no
`degraded`, no timer and no applet `Output`.

## The tick

`ctx.interval(period)` is the only timer an applet gets, delivering `Input::Tick`. **A tick lands
on the boundary**, not on whenever the panel started: `until_boundary` takes the time since the
epoch modulo the period, without which a `%H:%M` clock changes up to a minute late. **A missed tick
is skipped, not burst** — tokio's default `Burst` would deliver every missed tick in one pass after
a long suspend; `Skip` keeps the phase, where `Delay` would restart from the stall.

**The clock derives its period rather than being configured**: `%H:%M` ticks once a minute,
`%H:%M:%S` once a second, decided by scanning for a specifier faster than a minute — reading actual
specifiers, since `%-S`'s padding modifier defeats `contains("%S")`. **A format string that cannot
render must not panic**: chrono's `DelayedFormat` `Display` returns an error for an unknown
specifier, and `to_string()` would turn that into a panic that `catch_unwind` converts into "applet
dead for the session"; it renders through `write!` instead and returns `None`.

## An applet may supply its own widget

`view()` returning `Some` replaces the group and `indicators()` is never called; the root is a
`gtk4::Box`, not the group, and such an applet receives no `Input::Pointer` — the widget owns its
own. **Orientation is handed to the applet, not applied behind its back**: reaching into the view's
own `BoxLayout` turns it sideways without telling it, so it cannot restyle for the new axis.
Signals are wired in `view`, called once, since a GTK callback outlives any `&Ctx`.

## The popover

`Applet::popover(&Seat)` builds the tree on open; the runtime drops it on close and nothing is
cached. An open popover still follows events through a `glib::WeakRef`, weak because a strong
reference would hold the tree alive past dismissal, and `open_popover` only *raises* — it leaves an
open popover alone, so a question arriving inside the fade is still shown.

**A `Gtk.Popover` on a layer surface cannot be dismissed by a click elsewhere** — that dismissal is
`xdg_popup.grab`, and `autohide(true)` with `KeyboardMode::OnDemand` (how GTK asks for the grab)
leaves `focused-window` at `None` for as long as the popover is open. `applet/catcher.rs` instead
holds a second layer surface over the whole output, mapped only while a popover is up, buying
outside-click dismissal at the cost of hand-rolled placement and a drawn arrow. One catcher per
panel means **one popover at a time is structural**, and `KeyboardMode::None` means `Escape`
dismisses nothing. **Blur is a region, not the surface** — `[appearance] blur` hands
`glimpse_widgets::blur` the body and arrow, never the catcher's own window.

`Applet::anchor` names a widget in the view; `placement()` keeps the arrow centred on the pressed
item and the body on the output with a gutter that yields when the arrow needs the room instead.
**Placement waits for the window, not the slot**, since a layer surface has no size until the
compositor configures it: `open` waits on a tick callback for a real allocation, and that callback
stays, re-settling whenever the body's measurement changes. **The catcher takes
`set_exclusive_zone(0)`** and lets the compositor place it — margining by `config.size` would
assume `set_thickness` is a minimum. **A position change closes an open popover**, since the anchor
is one axis's coordinate and reusing it across orientations is arbitrary.

**The animation is `AdwTimedAnimation`, not a CSS transition** — a `transition: opacity` on the
same node did not animate, and an unmapped widget or `gtk-enable-animations: false` skips to the
end and emits `done` synchronously, keeping the state machine one path. **The shadow is in `px`**:
`box-shadow` with `rem` lengths renders nothing in GTK4, silently. The arrow is a
`Gtk.DrawingArea`, not a rotated box, since GTK4 has no triangle and a rotated square overflows its
allocation into the bar.

## The applets

**clock and next-event** — `agenda.rs` owns everything about a calendar event, so a new field lands
in one place. **The next-event applet has no empty state, which is why it is usually absent** —
nothing inside `within` means no chip, and an event ending under an open popover closes it through
`Opener::close_popover`, since nothing else would.

**clipboard** — copying anything closes the popover, since it was picked to be pasted.
**Images decode through `thumbnail`, never `Texture::from_bytes`** — a byte cap says nothing about
pixel count, and a small file can decode to an enormous bitmap.

**color-picker** — the service owns the palette and the clipboard copy; the applet holds none of
it, and failures are reported by notification, never a popover banner. **ruler** — shaped like
color-picker: right click runs `glimpse-ruler`, refused mid-measurement; the chip is always
icon-only, since a measurement has no swatch to substitute it with.

**places** — a share reads as its folder over `host · PROTOCOL`, parsed out of the GVFS mount name,
never the raw `smb-share:` string; bookmark and volume labels are capped through `render::cap`,
since that text comes from another application. **removable** — renders no chip with nothing
attached; capacity is a free-of-total string, never a percentage, and a volume with none left says
so in words rather than printing `0 B free`.

**kdeconnect** — renders no chip while `kdeconnectd` is not running. **An incoming pair request is
answered in its own notification, never the popover** — `kdeconnectd` posts Accept/Reject itself.
**weather** — several places is several applets, through `extends`. **Units come off the typed
provider snapshot, never the panel configuration**, so a units change cannot print °F over a
Celsius reading mid-flight.

**mpris** — **position is advanced here, not polled there**: MPRIS emits no change signal for
`Position`, so `position_us`/`position_at`/`rate` are interpolated locally. Volume stays with the
audio applet — the popover slider is only the current player's own `Volume`.

**keyboard** — the compositor owns the layout list; this applet only renders it and sends the
switch command. **pager** — groups workspaces under each display's name, shared with the privacy
applet through `applets::output_name`, never the connector.

**workspace-name** — shares `applets/workspace.rs` with the pager so the two never disagree about a
workspace. **The chip takes the new name before the compositor answers and drops it on any answer,
success included, re-reading the snapshot** — niri refuses a name another workspace already holds
and still replies `Ok`, so only the snapshot knows whether the rename happened.

**notifications** — the chip is kept even when the list is empty so do-not-disturb stays reachable;
**the icon cache is pruned against each new list**, so sender-controlled keys cannot accumulate.
**display** — the chip is `video-display-symbolic` regardless of output count, and **the glyph
deliberately does not track it**, since a chip that changes shape on hotplug reads as a different
applet appearing.

**tray** — the second applet supplying its own view, for the pager's reason. **The icon ladder is
six steps**: an absolute path that exists wins over a themed name, the item's own `IconThemePath`
is probed before the icon theme, and pixels are last. **A dbusmenu separator is an item; a `GMenu`
separator is a section boundary**, so the transform must leave no empty sections. **`a(iiay)` is
ARGB32 in network byte order** — GDK's `A8r8g8b8` exactly; a dark halo means premultiplication was
guessed wrong.

**bluetooth** — the chip is the adapter's power state alone. **`unmap` stops a scan
unconditionally**, since any gate on published state would lose a scan started in the last round
trip. **A pairing prompt is a page whose `raised` keys auto-open on the device id**, not a boolean,
because BlueZ escalates mid-flow. `BondBroken` is stated on the row, never notified.

**network** — the chip is the connection's own icon alone. **The password is asked for before the
join**, because NetworkManager drops the working connection the moment activation is requested.
**A connection in use is one row that opens its card, and Disconnect lives only there.** A wired
row routes to `connect_device` — sent to `connect_access_point` it is silently not found.

**audio** — needs a PulseAudio-protocol server and renders nothing without one. **Muted is the
muted glyph in the warning colour everywhere**, never a greyed icon or the word *Muted*. **A master
fader carries no device id of its own** — its controls resolve the current default device from a
fresh snapshot at signal time, never a value captured when the popover was built.

**brightness** — the chip renders as long as either a backlight source or a reachable night light
exists. **The current display resolves in three rungs**: the focused output's source, then the one
with no connector (the internal panel), then the first. **The switch reads `schedule != "off"`,
never `active`** — `active` is `temperature != DAY`, false every daylight hour under Automatic.

**idle** — the chip stays reachable while the daemon answers, since it is the only way to reach the
hold presets. **The daemon keeps no end time, so the applet remembers the one it asked for**; a
restart forgets it. **A hold's id never reaches the applet directly** — `manual_hold_ids` derives
the set from the daemon's own literal each time, so it never adopts an id the provider has stopped
reporting.

**battery** — the chip is `DisplayDevice`; internals and the charge-limit switch come from the
first present `BAT*` object, since the composite omits them. **The hero says when, not what**: time
left while draining, "Full at {time}" while charging, "Held at N%" under an enabled charge limit.

**command** — a user-defined chip running one argv per gesture with no shell; a program that
cannot start is throttled to one notification per interval. **session** — power actions confirm on
the app host after the popover closes, with the action as the default response; lock runs immediately.

**privacy** — a use with no application (location, a screencopy cast) is titled by the resource.
**A `PipeWire` cast's card offers only *Stop sharing*** — nothing else opens, since a camera cannot
be handed back to the process holding it. **A cast takes `IndicatorSpec.class` instead of
severity**, so `glimpse.css` can paint the record glyph in the danger colour while the timer keeps
the bar's foreground.

**system-monitor** — opt-in: its service samples nothing unless the applet is actually *placed* on
a panel zone, not merely present in the config. CPU and network read `None` on the first sample
since enabling, since a rate needs a delta the first sample cannot have.

## Losing the session bus kills the process, and nothing here can change that

A panel whose session bus dies terminates with `SIGTERM` and leaves nothing in the log — every GTK
application on the machine behaves the same way. The `closed` signal on the connection
`g_bus_get_sync` returns never fires and GLib prints no message of its own; the process is gone
before anything in `run` could speak. **`Restart=on-failure` deliberately does not cover it**:
systemd's `on-failure` excludes SIGTERM, so the unit does not come back — right in both cases that
reach it, since a session bus that died is a session that is ending and an ordinary `systemctl stop`
is the same signal.

## Rules

An applet renders typed service snapshots and sends typed commands through its injected handle. It
never opens a D-Bus connection, never reaches a backend directly, and holds no state that outlives
its own widget. UI state never waits on a round trip: update the widget optimistically and let the
service event reconcile it. **A command that fails wakes its own applet**, because a mirror service
publishes nothing when a rejected command leaves its state byte-identical — the optimistic flip
would otherwise stand until something unrelated moved.

**One substituter renders every `{token}` format.** `applets/tokens.rs::render` walks the template
once, resolving each token through a closure the applet supplies — chained `String::replace` is the
wrong shape, since each replacement would run over the previous one's output.
