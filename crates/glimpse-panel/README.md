# glimpse-panel

The panel: layer-shell bars, applets and popovers. Builds the binary named `glimpse-panel`.

## Contents

- `main.rs` — CLI, config load and the GTK application start
- `app.rs` — one bar per (panel config × monitor), the local service graph, config and theme watches;
  the read-only GTK `bars` action carries `a(uss)` entries of config index, connector and edge for
  every mapped bar, including empty ones
- `services.rs` — the panel-local composition root and its explicit service dependencies
- `config_alert.rs` — configuration problems as one notification under `<app id>.Config`
- `components/panel.rs` — bar window, zones, applet reconciliation
- `applet/` — the framework: the trait, `Ctx`, the relm4 runtime, the popover catcher, and
  `popover::launch`, which starts an argv in a transient user service with no shell, from the
  panel's environment or, when asked, from a clean one built with `env -i`, kept until its cgroup
  empties so whatever a launcher forks survives it, reports a start job ending other than
  `done` as an error, and falls back to a direct spawn when the user manager is unavailable; its
  D-Bus half runs on relm4's tokio runtime, since zbus finds no reactor on the GLib executor
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

**Zone reconciliation keys applets by `(zone, name, kind)`**. An attached desktop id enters its
zone only when no configured exec applet on the bar resolves to that id; that applet's slot
becomes the dev slot in place. A name with no implementation still occupies a `Slot`
with `handle: None`, keeping the key sequences comparable. There is deliberately no staleness, no
`degraded`, no timer, and a runtime's only outward channel is its `applet.<serial>` action.
**Anything on the session bus can activate that action exactly as a pointer can**: a click runs a
command applet and a middle click mutes audio; only the session actions still ask for a second click
inside their popover.

## The tick

`ctx.interval(period)` is the only timer an applet gets, delivering `Input::Tick`. **A tick lands
on the boundary**, not on whenever the panel started: `until_boundary` takes the time since the
epoch modulo the period, without which a `%H:%M` clock changes up to a minute late. **The phase is
re-derived from the wall clock before every sleep**: a counted interval runs on the monotonic clock,
which stops during suspend and ignores a clock step, and would hold its old phase for the rest of
the session.

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
maps, only while a popover is up, a bare layer surface over every output and the arrow and body on a
surface of their own, presented after the bare one so it stacks above it, buying outside-click
dismissal at the cost of hand-rolled placement and a drawn arrow. **A layer surface that paints
nothing is never mapped and takes no click**, so each bare surface holds an empty `DrawingArea`.
**The popover surface is the arrow and body plus a `GUTTER` for the shadow, never the output**: a
compositor blurs the whole surface that carries a blur region, again on any damage beneath it, so a
blurred surface over the output is a full-output blur every frame. It is anchored to the bar's edge
and placed along it by layer-shell margin, the gutter yielding where the output ends; like the bare
surface on its output it takes exclusive zone 0, so both share the coordinates `placement()` works
in. One catcher per panel means **one popover at a time is structural**, and `KeyboardMode::None`
means `Escape` dismisses nothing. A popover that asks for typing gives its surface
`KeyboardMode::Exclusive`, since on-demand focus arrives only with a click into the surface and keys
would reach the window behind; there `Escape` closes it. **Blur is the body and the arrow, not a
surface** — `[appearance] blur` blurs the popover surface, and `backdrop-filter` sits on the
body and the arrow inside it.

`Applet::anchor` names a widget in the view; `placement()` keeps the arrow centred on the pressed
item and the body on the output with a gutter that yields when the arrow needs the room instead.
**Placement waits for the window, not the slot**, since a layer surface has no size until the
compositor configures it: `open` waits on a tick callback for a real allocation, and that callback
stays, re-settling whenever the body's measurement changes. **The catcher takes
`set_exclusive_zone(0)`** and lets the compositor place it — margining by `config.size` would
assume `set_thickness` is a minimum. **A position or monitor change closes an open popover**, since the
anchor is one axis's coordinate and reusing it across orientations is arbitrary.

**The animation is `AdwTimedAnimation`, not a CSS transition** — a `transition: opacity` on the
same node did not animate, and an unmapped widget or `gtk-enable-animations: false` skips to the
end and emits `done` synchronously, keeping the state machine one path. **The shadow is in `px`**:
`box-shadow` with `rem` lengths renders nothing in GTK4, silently. The arrow is a
`Gtk.DrawingArea`, not a rotated box, since GTK4 has no triangle and a rotated square overflows its
allocation into the bar. The slot is a `Gtk.Grid` placing the arrow by cell and drawing it after the
body on every side: a box draws in layout order, so the body's shadow would cover a top or left arrow.

## The applets

**clock and next-event** — `agenda.rs` owns everything about a calendar event, so a new field lands
in one place. **The next-event applet has no empty state, which is why it is usually absent** —
nothing inside `within` means no chip, and an event ending under an open popover closes it through
`Opener::close_popover`, since nothing else would.

**clipboard** — copying anything closes the popover, since it was picked to be pasted.
**Images decode through `thumbnail`, never `Texture::from_bytes`** — a byte cap says nothing about
pixel count, and a small file can decode to an enormous bitmap.
A decoded image is also written, private, under `runtime_dir()/clipboard/<id>/` and deleted with
its entry: a drop hands a file manager a URI, and a drag's
content must exist when it starts. Decoding and writing run on `gio::spawn_blocking`.
`WaylandSelection` adapts `glimpse_wayland::selection` to the service's `Selection` trait. It
opens a data-control watch only while the service has an events subscriber and passes read demand
before reading an offer, so a disabled clipboard service opens no connection or reads no secret.

**places** — a share reads as its folder over `host · PROTOCOL`, parsed out of the GVFS mount name,
never the raw `smb-share:` string; bookmark and volume labels are capped through `glimpse_utils::clean`,
since that text comes from another application. **removable** — renders no chip with no
visible device; an empty card reader is not one. **A device is one row, and its main volume is the
largest, the first on a tie.** Space is the short "N free" form, never a percentage, and a main
volume with none left says so in words rather than printing `0 B free`. A refused eject reads "In use
by an app"; the overflow cap counts devices.

**weather** — several places is several applets, through `extends`. **Units come off the typed
provider snapshot, never the panel configuration**, so a units change cannot print °F over a
Celsius reading mid-flight.

**mpris** — **position is advanced here, not polled there**: MPRIS emits no change signal for
`Position`, so `position_us`/`position_at`/`rate` are interpolated locally. Volume stays with the
audio applet; the popover has no volume control.

**keyboard** — the compositor owns the layout list; this applet only renders it and sends the
switch command. **pager** — groups workspaces under each display's name, shared with the privacy
applet through `applets::output_name`, never the connector.

**workspace-name** — shares `applets/workspace.rs` with the pager so the two never disagree about a
workspace. **The chip takes the new name before the compositor answers and drops it on any answer,
success included, re-reading the snapshot** — niri refuses a name another workspace already holds
and still replies `Ok`, so only the snapshot knows whether the rename happened.

**notifications** — the chip is kept even when the list is empty so do-not-disturb stays reachable;
**the icon cache is pruned against each new list**, so sender-controlled keys cannot accumulate.
**display** — one chip and popover for brightness, night light and connected outputs. The chip is
`display-brightness-symbolic` while a backlight source or reachable night light exists, otherwise
`video-display-symbolic` while there are outputs, with no chip when neither exists. **The glyph
does not track output count**, since changing shape on hotplug reads as a different applet.
**The current display resolves in three rungs**: the focused output's source, then the one with no
connector (the internal panel), then the first. **The switch reads `schedule != "off"`, never
`active`** — `active` is `temperature != DAY`, false every daylight hour under Automatic.

**tray** — the second applet supplying its own view, for the pager's reason. **The icon ladder is
six steps**: an absolute path that exists wins over a themed name, the item's own `IconThemePath`
is probed before the icon theme, and pixels are last. **A dbusmenu separator is an item; a `GMenu`
separator is a section boundary**, so the transform must leave no empty sections. **`a(iiay)` is
ARGB32 in network byte order** — GDK's `A8r8g8b8` exactly; a dark halo means premultiplication was
guessed wrong.

**bluetooth** — the chip is the adapter's power state alone. **`unmap` stops a scan
unconditionally**, since any gate on published state would lose a scan started in the last round
trip. Only the Forget confirmation lives in the popover. `BondBroken` is stated on the row, never notified.

**network** — the chip is the connection's own icon alone. **Every network row opens a drawer with Connect or Disconnect first.** A wired row routes to
`connect_device` — sent to `connect_access_point` it is silently not found.

**audio** — needs a PulseAudio-protocol server and renders nothing without one. **Muted is the
muted glyph in the warning colour everywhere**, never a greyed icon or the word *Muted*. **A master
fader carries no device id of its own** — its controls resolve the current default device from a
fresh snapshot at signal time, never a value captured when the popover was built.

**idle** — the chip stays reachable while the daemon answers, so its presets remain accessible.
**The provider owns each manual hold's deadline in `Idle1.HoldExpiries`, keyed by inhibitor ID.**
**Preset replacement releases only manual holds in the current snapshot**, excluding the new hold.

**power** — charge comes from `DisplayDevice`, with a profile or AC icon on desktops; internals
and the charge-limit switch come from the first present `BAT*` object, since the composite omits them.

**command** — a user-defined chip running one argv per gesture with no shell, from the panel's
environment plus its `env`, or from only the session variables and its `env` when `inherit-env` is
off; a program that cannot start is throttled to one notification per interval. **session** — power
actions confirm in a parentless dialog after the popover closes, with the action as the default response; lock runs immediately.

**privacy** — a use with no application (location, a screencopy cast) is titled by the resource.
**A `PipeWire` cast's card offers only *Stop sharing*** — nothing else opens, since a camera cannot
be handed back to the process holding it. **A cast takes `IndicatorSpec.class` instead of
severity**, so `glimpse.css` can paint the record glyph in the danger colour while the timer keeps
the bar's foreground.

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
