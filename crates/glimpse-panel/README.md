# glimpse-panel

The panel: layer-shell bars, applets and popovers. Builds the binary named `glimpse-panel`.

## Contents

- `main.rs` — GTK application, layer-shell setup, one bar per output across hotplug
- `app.rs` — one bar per (panel config × monitor), the local service graph, config and theme watches
- `services.rs` — the panel-local composition root and its explicit service dependencies
- `components/panel.rs` — bar window, zones, applet reconciliation
- `applet/` — the framework: the trait, `Ctx`, the relm4 runtime, the popover catcher, and
  `popover::run`, which launches the `settings-command` every footer row offers
- `applets/` — one module per applet plus the registration match; `agenda.rs` holds what the clock
  and next-event applets both need to say about a calendar entry

## Applets

Most applets own one `IndicatorGroup`, so their view is the same shape and only
`Vec<IndicatorSpec>` varies. The trait is object-safe and the runtime stores `Box<dyn Applet>`:

```rust
fn handle(&mut self, ctx: &Ctx, input: &Input)
fn view(&mut self, ctx: &Ctx) -> Option<gtk4::Widget>   // None: the runtime supplies the group
fn indicators(&self) -> Vec<IndicatorSpec>
```

**The applet builder receives `Ctx` and captures its exact service handle.** It seeds from
`handle.snapshot()`, then `Ctx::watch(handle.subscribe())` turns later changes into `Woken` inputs.
`Ctx` owns the forwarding task, so removing the applet cancels its subscription without a second
lifetime mechanism. Applets contain no socket, topic or JSON routing path.

**A panic stops one applet, not the panel.** `handle` and `indicators` run inside one
`catch_unwind`; a panic logs, drops the applet, stops its sources and empties its group. Unwinding
past a `&mut self` mid-mutation leaves state nobody can reason about.

**`indicators()` is a pull** called after every `handle`, feeding `set_items`, which compares before
writing. An empty vector hides the group — that is how an applet says it has nothing yet, never a
placeholder.

**Scroll reaches an applet as whole notches.** The group emits raw deltas and a touchpad sends many
small ones; the runtime accumulates per axis and drains in whole units. The accumulator belongs to
the group, so an applet whose indicators change mid-gesture does not lose the remainder.

**Pointer input names no indicator.** The whole group is one clickable target.

**Zone reconciliation is keyed by `(zone, name, kind)`.** `MonitorsChanged` and `ThemeChanged` both
reach `reconcile_panels`, so the guard comparing desired against current key sequences is what stops
every applet being rebuilt on every theme write. A name with no implementation still occupies a
`Slot` with `handle: None`, which keeps the sequences comparable. An unresolvable *name* is a user
typo logged at `warn`; a name resolving to an unimplemented kind is expected and logged at `debug` —
the shipped default names nineteen applets, so collapsing the two means nineteen warnings on an
untouched install.

There is deliberately no staleness, no `degraded`, no timer and no applet `Output`.

## The tick

`ctx.interval(period)` is the only timer an applet gets, delivering `Input::Tick`.

**A tick lands on the boundary**, not on whenever the panel started: `until_boundary` takes the time
since the epoch modulo the period, without which a `%H:%M` clock changes up to a minute late. Exactly
on a boundary the wait is a whole period, so nothing renders twice.

**A missed tick is skipped, not burst.** tokio's default `Burst` delivers 3600 ticks in one pass of
the main loop after an hour's suspend, and `Skip` is the only behaviour keeping the phase — `Delay`
restarts from wherever the stall ended.

**Calling it again replaces the timer**, which is what makes it safe to ask for from `configure`. A
period of zero is refused and logged.

**The clock derives its period; it is not configured.** `%H:%M` ticks once a minute and `%H:%M:%S`
once a second, decided by scanning for a specifier faster than a minute; a setting would be a second
way to say what the format already says. The scan reads specifiers, not substrings: `%-S` carries a
padding modifier that `contains("%S")` misses, and `%%S` is a literal percent that it matches.

**A format string that cannot render must not panic.** chrono's `Display` for `DelayedFormat`
*returns an error* for an unknown specifier and `to_string()` turns that into a panic that, under
`catch_unwind`, stops the applet for the session. It renders through `write!` and returns `None`.

## An applet may supply its own widget

`view()` returning `Some` replaces the group and `indicators()` is never called. The pager is the
first case: a click *per slot* over a list whose length changes.

- **The root is a `gtk4::Box`, not the group**, `init_root()` taking no arguments; and **an applet
  supplying a view receives no `Input::Pointer`**, the widget owning its own.
- **Orientation is handed to the applet, not applied behind its back.** Reaching into the view's own
  `BoxLayout` turns the widget sideways without telling it, so it cannot restyle for the new axis:
  on the pager a vertical bar stretched every dot, the active-dot rule keying on `min-width`.
- **Signals are wired in `view`, called once.** A GTK callback outlives any `&Ctx`, so applets
  capture a typed handle and put what a callback needs at click time behind an `Rc` cell.

## The popover

`Applet::popover(&Seat)` builds the tree on open and the runtime drops it on close; nothing is
cached. An open popover still follows events: the applet keeps a `glib::WeakRef` and pushes every
render into it, weak because a strong reference would hold the tree alive past dismissal.

**An applet on the runtime's `IndicatorGroup` gets its popover opened for it.** One owning its view
asks by name: `Opener::toggle_popover` is that press (the pager's click), `open_popover` only
*raises* — it opens one closed or still fading shut and leaves an open one alone, so a question
arriving inside the 150ms fade is still shown. Bluetooth raises a pairing prompt nothing was open for.

### It is not a `Gtk.Popover`

`applet/catcher.rs` holds a second layer surface anchored to all four edges of the panel's monitor,
mapped only while a popover is up. **A `Gtk.Popover` on a layer surface cannot be dismissed by a
click on another application** — that dismissal is `xdg_popup.grab`, `autohide` is how GTK asks for
it, and the grab costs the keyboard: `KeyboardMode::OnDemand` plus `autohide(true)` leaves
`focused-window` at `None` for as long as the popover is open. Owning the surface buys outside-click
dismissal, one popover at a time and an exit animation, and costs hand-rolled placement and a drawn
arrow.

One catcher per panel, shared by every applet on it, so **one popover at a time is structural**.
`KeyboardMode::None`, so nothing is taken from the focused window and `Escape` dismisses nothing.

**`open` takes the dismissal callback**, so only the applet owning the current popover hears about
it, and one removed by a config change leaves no closure and no `Sender` behind. The runtime asks
`Catcher::holds` first: a replaced applet holds its handle until `PopoverDismissed` is delivered.

### Placement

`Applet::anchor` names a widget inside the view; the runtime turns it into a centre coordinate.
`placement()` is the whole arithmetic and is a free function, asserted without a display: the
arrow's centre is the pressed item's centre, the body stays on the output keeping a gutter from its
edge, and the arrow never sits on a rounded corner.

- **The gutter yields to the arrow.** A popover near the edge cannot both keep a gutter and put its
  arrow over the item, so the gutter shrinks. One CSS length drives arrow size, inset and gutter.
- **Placement waits for the window, not for the slot.** A layer surface has no size until the
  compositor configures it, so an idle after `present()` measures `width=0`, and a reopen reads
  `room()` as zero while the slot keeps its old allocation — which looked like an anchoring bug.
  `settle` touches no margin while room is zero; `open` waits on a tick callback for a real
  allocation, settles, then plays. **The callback then stays**, re-settling whenever the body's
  measurement changes — a drawer opening inside one otherwise walks the detail page off the edge.
- **The catcher takes `set_exclusive_zone(0)` and lets the compositor place it** — no margin, no
  measurement of the bar. Margining by `config.size` assumes two false things: `set_thickness` is a
  **minimum**, and anything else holding an exclusive zone pushes the panel down.
- **A position change closes an open popover.** The anchor is one axis's coordinate, so re-placing a
  `Top` popover's x as a `Left` popover's y is arbitrary; orientation and axis come from `Position`.

### The animation is `AdwTimedAnimation`, not a CSS transition

`opacity` on the slot, driven by `adw::TimedAnimation`; a CSS `transition: opacity` on the same node
did not animate. `done` is an exact clock, so no duration constant is duplicated, and an unmapped
widget or `gtk-enable-animations: false` skips `play()` to the end and emits `done` synchronously,
which lets the state machine be one path.

**The shadow is in `px`**: `box-shadow` with `rem` lengths renders **nothing** in GTK4, silently.

**The arrow is a `Gtk.DrawingArea`, not a rotated box.** GTK4 has no triangle, and a square with
`transform: rotate(45deg)` overflows its allocation into the bar. Size and color come from CSS and
the fill reads `gtk_widget_get_color`.

## The applets

**clock and next-event** — `agenda.rs` owns everything about an event that is not a calendar:
`Occasion`, the conversion off the wire, `when`, and `row`, so a new field lands in one place.
Twelve-hour detection and the two clock formats live in `glimpse-config`, reached as
`glimpse_config::clock(twelve)`.

- **The popover is always local time, even on a clock with a `timezone`.** That setting moves the
  bar label; a calendar is not somewhere else.
- **Several panels share one calendar range, so the last to ask wins** — no client identity.
- **A day past `truncated_from` says so instead of looking empty.**
- **The next-event window is why the applet is usually absent.** Anything further out than `within`
  leaves nothing on the bar. An all-day entry is demoted and excluded by default, or a week of them
  would bury the meeting in ten minutes; a multi-day entry counts from the day the reader is on.
- **It does not send `calendar.set_range`** — the clock owns the month being shown.

**weather** — several places is several applets, through `extends`. The lease renews on a minute's
tick against the provider's thirty-minute lease.

- **A fixed place still missing a whole tick after the provider took the name gets a warning chip.**
  Rendering nothing stays the answer for a place with no reading yet; a provider that holds the
  name, reports itself available and serves none of what we asked for is a different thing.
  `note_unserved` runs on the tick, which keeps the startup gap from flashing a warning.
- **Units come off the typed provider snapshot, never off the panel configuration**, so a units
  change cannot print °F over a Celsius reading while the updated snapshot is in flight.
- **The list starts at tomorrow and the strip at the next hour** — today and the hour standing are
  already the hero.
- **An alert takes the chip's icon and its colour.** The bar has room for one thing.
- **A weekday name is formatted through `LC_TIME`, not looked up in the message catalog.**
- **The icon is cached by name**, because `indicators()` is a pull after every input.

**mpris** — **position is advanced here, not polled there**: MPRIS emits no change signal for
`Position`, so the payload's `position_us`/`position_at`/`rate` are interpolated locally.

- **`label-format` substitutes by name** — `.replace` per placeholder, never `format!` into the
  msgid. **The cap counts characters**, because track titles are chosen by whatever is playing.
- **A label that renders to nothing leaves an icon-only chip, not an absent applet.**
- **The optimistic value goes into `self.players`, not beside it**, so `dress` has one source.
- **`aimed` is a shared cell holding the current player's id**, which row signals carry too.
- **`show-others = false` hides the section**, because an empty `Section` shows its placeholder.
- **An icon is a name the theme actually has**, checked with `IconTheme::has_icon`: `DesktopEntry`
  first, then the bus-name suffix whole and a segment at a time.

**keyboard** — the chip is the current layout's code, hidden under two layouts. The compositor owns
the list; this applet only renders it and sends the switch command.

**notifications** — the chip is a bell, hidden until the list has arrived and kept afterwards even
when empty so do-not-disturb stays reachable. A collapsed stack card is a preview, so its
per-notification controls are hidden: left click opens the stack, right click clears that
application without focusing it. On an individual unread card, left click invokes the specification's
`default` action when one was offered, asks the compositor to raise the sender when `app_pid` is
known, and dismisses; right click and the close button remove it without either. Read history has
neither activation nor action buttons. The icon cache is pruned against each new list, so
sender-controlled keys cannot accumulate for the panel's lifetime.

**tray** — the second applet supplying its own view, for the pager's reason: a click per chip over a
list whose length changes. It renders every registered item and decides *which*, because that is a
bar's preference — `hide` and `pin` match the item's own `Id`, which survives a restart where its bus
name does not. `Passive` sorts toward the chevron rather than vanishing; `max-visible` of `0` means
no overflow, never hide-everything. A tray icon is rendered as the application gave it.

**The icon ladder is six steps and every one was a bug somewhere.** An absolute path that *exists*
wins; one that has gone falls through, because a missing file is not a missing icon. Only an absolute
path is a path — a themed name may contain a slash. The item's own `IconThemePath` is probed as a
literal file (`base/name`, then `.png`, `.svg`, `.xpm`, `.ico`) before the icon theme is consulted,
and a directory that is not there is skipped at `debug`, never `warn`: a Flatpak application names
`/app/share/icons`, real in its sandbox and absent here. Paths added to the process-wide `IconTheme`
are deduped and capped at 16. Pixels are last; nothing at all gets `image-missing-symbolic`.

**A dbusmenu separator is an *item*; a `GMenu` separator is a section boundary.** The transform is a
split, and leading, trailing and doubled separators must leave no empty sections. `visible: false` is
not built at all — building it disabled still shows what the application asked to hide. A checkmark
is a boolean-stateful `SimpleAction`, a radio group a string-stateful one; a section is the group.

**`com.canonical.dbusmenu.Status` is a second `Status`, on the menu object** — `notice` is the calm
counterpart to `NeedsAttention`, both can be true, and attention wins.

**`a(iiay)` is ARGB32 in network byte order, which is byte order A,R,G,B — precisely GDK's
`A8r8g8b8`.** No swizzle, no PNG round-trip. Premultiplication is unspecified by the protocol, so a
dark halo is the symptom of guessing wrong rather than something to tune. Textures cache on a content
hash, so an application rewriting its icon per message is free after the first. `connect_changed` on
the icon theme and `notify::scale-factor` are connected **once, in the applet**, not per item.

**bluetooth** — the chip is the adapter's power state and **nothing else**: an icon, never a device
name or a count; what is connected belongs to the tooltip. No adapter renders **nothing**, so a
machine with no radio carries no dead chip.

- **Selection and the two expanded flags are `Rc` cells the popover's closures write and
  `Input::Woken` reads back**, since a signal closure has no `&mut self`. `unmap` stops a scan
  unconditionally: any gate on published state loses a held one started in the last round trip.
  **An overflow row toggles to *Show fewer***: nothing scrolls, so one that only expands pushes the
  switches off the output.
- **A pairing prompt is a page; `raised` keys its auto-open on the device id**, since BlueZ
  escalates mid-flow and a boolean would re-open one just dismissed. The dialog clears its entry on
  a change of **device** rather than of name, because BlueZ re-asks as a name resolves.
- **The chip and its tooltip take `attention` while a question waits**, or a dismissed popover was
  the only thing that knew it was asked. **`BondBroken` is stated on the row, not notified**: the
  bond is gone, so *Connect* fails until the device is paired again.
- **Only the two prompts needing an entry reach `App`**, narrowed by `render::typed`, so one the
  popover draws never trips `close_popovers`. Those that do close every popover, then title, size
  and **show** the host before `present`.
- **Two switch rows own discovery and visibility**, set on open, cleared on unmap, **never
  re-asserted between** — a wake that re-asked fights the timeout that just lapsed. Both go
  insensitive while the radio is off, and `chip` reads `state.held()`, so a popover's own scan never
  lights the bar.

**network** — the chip is the connection's own icon and **nothing else**: no SSID and no percentage,
both of which belong to the tooltip. No managed device renders nothing.

- **A VPN is a second chip, never an overlay**, since two overlays do not compose. **Metered is
  marked on the connected row**; the tooltip naming it is configuration and the chip never has it.
- **The password is asked for before the join, on a page of the popover**, because NetworkManager
  drops the working connection the moment activation is requested. **A request NetworkManager raises
  survives the popover being shut**; one the user began does not.
- **Strength is banded at render from the raw value**, which the tooltip prints exactly, and **the
  connected network is placed first whatever its strength**. A failed command is a notification.
- **A wired row is a device, not a profile**, so it routes to `connect_device`; sent to
  `connect_access_point` it is silently not found. Its card names speed and address, and **an
  unplugged cable activates nothing**.

**audio** — needs a PulseAudio-protocol server, `pipewire-pulse` or PulseAudio proper, and renders
nothing without one.

- **Selection and the three expanded flags are `Rc` cells the popover's closures write and
  `Input::Woken` reads back**, the same shape as bluetooth's. A held app id is filtered against
  current state on every dress, since a stream's app is the common thing to vanish, not the rare
  one. **`move_app` closes the detail on success; every volume, mute and default-device change
  leaves it open.**
- **A master fader carries no device id of its own**, so its `level-changed`/`level-toggled`
  resolve the current default device from a fresh `AudioHandle::snapshot()` at the moment the
  signal fires, never from a value captured when the popover was built.
- **List caps are constants in `indicator.rs`, not configuration** — `AppletKind::Audio {}` carries
  no settings yet, unlike bluetooth's `devices`/`nearby`.

**idle** — the priority table behind the hero subtitle is documented by `render.rs`'s own test names,
not restated here. The chip is never hidden while the daemon answers, unlike a notifier: it is the
only way to reach the six fixed hold presets, so it must stay reachable even with nothing to report.
`render::icon` carries that distinction instead.

- **A hold's id never reaches the applet directly** — `Hold()` is fire-and-forget like every other
  command, so `newly_adopted_holds` watches the next state for fresh records shaped like the daemon's
  own manual-hold literal and adopts every one it finds into a set, not a single id, so a second
  preset pressed while already holding is recognised as ours rather than rendered as a stranger.
  **The adoption shape includes `can_release`**: any session-bus client can forge the same
  `who`/`why` strings via `logind.Inhibit()`, but the daemon never marks that record releasable, so
  it is never adopted and the toggle never sends a `Release()` the daemon would silently refuse. A
  restart still forgets the panel's own holds and renders them as plain rows.
- **`why` is capped tighter in `row_status` than the daemon's own 240 characters**, so a verbose
  reason cannot push a row's `(Flatpak via portal)`/`(systemd-inhibit · pid N)` marker off the end.

## Losing the session bus kills the process, and nothing here can change that

A panel whose session bus dies terminates with exit 143 (SIGTERM) and leaves **nothing at all** in
the log. That is not this crate's doing — every GTK application on the machine behaves the same way.
Do not re-derive it; all three obvious guesses were checked. The `closed` signal on the connection
`g_bus_get_sync` returns never fires, `set_exit_on_close(false)` changes nothing, and GLib prints no
message of its own. The process is gone before anything in `run` could speak.

**`Restart=on-failure` deliberately does not cover it.** systemd's `on-failure` excludes SIGTERM, so
the unit does not come back — right in both cases that reach it: a session bus that died is a
session that is ending, and an ordinary `systemctl stop` is the same signal.

## Rules

An applet renders typed service snapshots and sends typed commands through its injected handle. It
never opens a D-Bus connection, never reaches a backend directly, and holds no state that outlives
its own widget.

UI state never waits on a round trip: update the widget optimistically and let the service event
reconcile it.

**One substituter renders every `{token}` format.** `applets/tokens.rs::render` walks the template
once, resolving each token through a closure the applet supplies; the applet decides its own token
names. Chained `String::replace` is the wrong shape — each replacement runs over the previous one's
output. Every value interpolated is compositor- or calendar-supplied text; the clock's
`tooltip_format` is strftime, not this.
