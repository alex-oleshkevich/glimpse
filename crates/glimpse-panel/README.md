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
since the epoch modulo the period. Without it a `%H:%M` clock changes up to a minute late, which
reads as broken rather than late. Exactly on a boundary the wait is a whole period, so nothing
renders twice.

**A missed tick is skipped, not burst.** tokio's default `Burst` would deliver 3600 ticks in one
pass of the main loop after an hour's suspend. `Skip` is also the only behaviour that keeps the
phase; `Delay` restarts the schedule from wherever the stall ended.

**Calling it again replaces the timer**, which is what makes it safe to ask for from `configure`. A
period of zero is refused and logged.

**The clock derives its period; it is not configured.** `%H:%M` ticks once a minute and `%H:%M:%S`
once a second, decided by scanning for a specifier faster than a minute. A setting would be a second
way to say what the format already says, and the two could disagree. The scan reads specifiers
rather than substrings, which matters twice: `%-S` carries a padding modifier so `contains("%S")`
misses it, and `%%S` is a literal percent so `contains("%S")` matches a non-specifier.

**A format string that cannot render must not panic.** chrono's `Display` for `DelayedFormat`
*returns an error* for an unknown specifier and `to_string()` turns that into a panic — which, under
`catch_unwind`, stops the applet for the session. It renders through `write!` and returns `None`.

## An applet may supply its own widget

`view()` returning `Some` replaces the group and `indicators()` is never called. The pager is the
first case: a click *per slot* over a list whose length changes.

- **The root is a `gtk4::Box`, not the group** — `init_root()` takes no arguments.
- **An applet supplying a view receives no `Input::Pointer`** — the widget owns its pointer.
- **Orientation is handed to the applet, not applied behind its back.** Reaching into the view's own
  `BoxLayout` turns the widget sideways without telling it, so it cannot restyle for the new axis —
  on the pager a vertical bar stretched every dot, the active-dot rule being keyed on `min-width`.
- **Signals are wired in `view`, called once.** A GTK callback outlives any `&Ctx`, so applets
  capture a typed handle and put what a callback needs at click time behind an `Rc` cell.

## The popover

`Applet::popover(&Seat)` builds the tree on open and the runtime drops it on close. Nothing is
cached. An open popover still follows events: the applet keeps a `glib::WeakRef` and pushes every
render into it — weak, because a strong reference would hold the tree alive past dismissal.

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
`placement()` is the whole arithmetic and is a free function so it can be asserted without a
display. Four properties, one test each: the arrow's centre is the pressed item's centre; the body
stays on the output, keeping a gutter from its edge; the arrow never sits on a rounded corner.

- **The gutter yields to the arrow, and that ordering is the design.** A popover near the edge
  cannot both keep a gutter and put its arrow over the item, so the gutter shrinks to whatever still
  lets the arrow reach. One CSS length drives arrow size, inset and gutter; none is written in Rust.
- **Placement waits for the window, not for the slot.** A layer surface has no size until the
  compositor configures it, so an idle after `present()` measures `width=0`, and a reopen reads
  `room()` as zero while the slot keeps its old allocation — which looked like an anchoring bug.
  `settle` touches no margin while room is zero; `open` waits on a tick callback for a real
  allocation, settles, then plays. **The callback then stays**, re-settling whenever the body's
  measurement changes — a drawer opening inside a popover otherwise grows against the margin
  computed for the narrow body and walks the detail page off the output edge.
- **The catcher takes `set_exclusive_zone(0)` and lets the compositor place it** — no margin, no
  measurement of the bar. Margining by `config.size` assumes two false things: `set_thickness` is a
  **minimum**, so a bar whose applets need more room is taller, and anything else holding an
  exclusive zone pushes the panel down. The missing number is the sum of every *other* zone.
- **A position change closes an open popover.** The anchor is a coordinate on one axis, so
  re-placing a `Top` popover's x as a `Left` popover's y puts it somewhere arbitrary; orientation,
  arrow side and placement axis all derive from `Position`.

### The animation is `AdwTimedAnimation`, not a CSS transition

`opacity` on the slot, driven by `adw::TimedAnimation`; a CSS `transition: opacity` on the same node
did not animate. Two properties are the reason not to go back: `done` is an exact clock, so no
duplicated duration constant; and an unmapped widget or `gtk-enable-animations: false` makes
`play()` skip to the end and emit `done` synchronously, which lets the state machine be one path.

**The shadow is in `px`, and it has to be.** `box-shadow` with `rem` lengths renders **nothing** in
GTK4 and fails silently.

**The arrow is a `Gtk.DrawingArea`, not a rotated box.** GTK4 has no triangle, and a square with
`transform: rotate(45deg)` overflows its allocation into the bar. Size and colour still come from
CSS, and the fill reads `gtk_widget_get_color`.

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
bar's preference — `hide` and `pin` match the item's own `Id`, which survives an application restart
where its bus name does not. `Passive` is the item asking to be put away, so it sorts toward the
chevron rather than vanishing; `max-visible` of `0` means no overflow, never hide-everything. A tray
icon is the application's choice, rendered as given: the symbolic-icon rule stops at our own chips.
This applet is why `"tray"` in the shipped right zone finally renders something.

**The icon ladder is six steps and every one was a bug somewhere.** An absolute path that *exists*
wins; one that has gone falls through, because a missing file is not a missing icon. Only an absolute
path is a path — a themed name may contain a slash. The item's own `IconThemePath` is probed as a
literal file (`base/name`, then `.png`, `.svg`, `.xpm`, `.ico`) before the icon theme is consulted,
and a directory that is not there is skipped at `debug`, never `warn`: a Flatpak application names
`/app/share/icons`, real in its sandbox and absent here. Search paths added to the process-wide
`IconTheme` are deduped and capped at 16. Pixels are last, and only when there is no name. Nothing at
all gets `image-missing-symbolic`, because a blank chip reads as broken.

**A dbusmenu separator is an *item*; a `GMenu` separator is a section boundary.** The transform is a
split, and leading, trailing and doubled separators must leave no empty sections. `visible: false` is
not built at all — building it disabled still shows what the application asked to hide. A checkmark
is a boolean-stateful `SimpleAction`, a radio group a string-stateful one with a per-item target;
dbusmenu never says which items form a group, so a section is the group.

**`com.canonical.dbusmenu.Status` is a second `Status`, on the menu object.** `notice` is the calm
counterpart to `NeedsAttention`; both can be true and attention wins.

**`a(iiay)` is ARGB32 in network byte order, which is byte order A,R,G,B — precisely GDK's
`A8r8g8b8`.** No swizzle, no PNG round-trip. Premultiplication is unspecified by the protocol, so a
dark halo is the symptom of guessing wrong rather than something to tune. Textures cache on a content
hash, which makes an application rewriting its icon per message free after the first. `connect_changed`
on the icon theme and `notify::scale-factor` are connected **once, in the applet**, not per item;
only name-based icons need re-resolving.

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
- **Only the two prompts needing an entry reach `App`**, narrowed by `render::typed` in the watch,
  so one the popover draws never trips `close_popovers`. Those that do close every popover first,
  then title, size and **show** the host before `present`.
- **Two switch rows own discovery and visibility**, set on open, cleared on unmap, **never
  re-asserted between** — a wake that re-asked fights the timeout that just lapsed. Both go
  insensitive while the radio is off, and `chip`/`hero`/`tooltip` read `state.held()`, so a scan the
  popover started never lights the bar.

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
