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

**Blur is a region, not the surface.** The catcher covers the whole output, so `[appearance] blur`
hands `glimpse_widgets::blur` the body and the arrow, never the window; the bar hands it the `Panel`.

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
`Occasion`, the conversion off the wire, `when`, `row`, the join and open-event links and
`open_http`, so a new field lands in one place.
Twelve-hour detection and the two clock formats live in `glimpse-config`, reached as
`glimpse_config::clock(twelve)`.

- **The popover is always local time, even on a clock with a `timezone`.** That setting moves the
  bar label; a calendar is not somewhere else.
- **Several panels share one calendar range, so the last to ask wins** — no client identity.
- **A day past `truncated_from` says so instead of looking empty.**
- **The clock popover's hero is always today**, and the day list is titled after the selected day:
  Today, Tomorrow, Yesterday, otherwise a weekday with its date. Both go through GLib, so they follow
  `LC_TIME` like the grid. On today, events that are over fold behind "N earlier"; an event opens a
  card when it has a join link, an event link, a calendar name or an organizer.
- **The next-event applet has no empty state, which is why it is usually absent.** It exists to
  show one event's details, so nothing inside `within` means no chip, and with no chip there is no
  popover to render empty. `horizon` reaches further than `within` for the *Coming up* list alone,
  and that section hides rather than captioning nothing. **An event ending under an open popover
  closes it**, through `Opener::close_popover` — nothing in the runtime closes one when its group
  empties, and with no empty state there is nothing to fall back to, so the alternative is a
  finished meeting left on screen with a live *Join* row. All-day is off by default. *Join* opens
  `Occasion.meeting_url` and only ever a recognised video-conferencing link; *Open event* opens
  `Occasion.event_url`, the calendar's own `URL` property, whatever it points at. Either row hides
  when the feed carries nothing for it, and so do the facts, which hold only what the heading does
  not already say — no location, no length, and `Status` only when tentative; `Description` shows
  the feed's own text verbatim. A *Coming up* row opens the same card the calendar's day list does,
  from `agenda::links` and `agenda::facts`.
  There is no RSVP. It does not send `calendar.set_range`. **`Clock.hide_all_day`** drops an all-day
  entry from the calendar popover's day list and month markers alike — the next-event applet has
  its own, older `all_day` toggle for whether one may take the bar, and the two settings are not
  the same knob.
- **An overlap is named, never re-chosen.** The bar takes one event — running before upcoming, then
  earliest start, then earliest end — and a *Conflicts* fact names what runs over it, with
  `{conflicts}` for the tooltip. Two meetings at once is the user's problem to see, not the applet's
  to resolve, and a count on the label would shift the bar as the day moves.

**clipboard** — renders `ClipboardState` as two `$Section`s, pinned above recent, each a
`$ClipboardList` of `$SplitRow`s: the body copies, the chevron unfolds Pin and Forget under it.
**A row carries no timestamp, so the applet takes no tick** — a relative age is the only thing that
would need one, and a minute timer redrawing unchanged rows is the cost of a line nobody reads.
**Both lists are capped by `visible`**, pinned included — nothing in the panel scrolls, and a
history of pins would otherwise run off the output. `WaylandSelection` lives in `src/selection/` and
not in `glimpse-services`, which may bind no `wl_` object; it holds the one data-control connection
and is injected as `Arc<dyn Selection>` into both the clipboard and the color picker services. An
offer keeps that connection up even with the clipboard disabled, because a selection lives only as
long as the connection that set it. **An image is decoded through `thumbnail`, never
`Texture::from_bytes`** — the service caps an entry's bytes, which says nothing about its pixel
count, and a small file can decode to an enormous bitmap. A picture that will not decode falls back
to its icon and stays restorable. Textures are cached by entry id and pruned when the entry leaves.
**The row thumbnail's size is `max-width`/`max-height` in `.clipboard-list picture`, not a Rust
`size_request`.** A `size_request` is a floor, not a ceiling — a decode up to 48px still asked for
its full natural size in the row, widening it past a same-row icon; `max-width`/`max-height` in
`glimpse.css` is what actually bounds it, with a small `margin` so it does not sit flush against the
row's edge.

**color-picker** — renders the `color_picker` service: a left click opens the palette, a right
click runs `glimpse-picker`. The service owns the palette and the clipboard copy; the applet holds
none of it. The chip is the latest pick as a `Swatch` in the indicator's extension slot, or the
picker icon before the first. A row copies in the configured format, read when it is pressed rather
than when the popover opened, and its chevron unfolds all six notations, each copying itself. A
failed pick or copy is reported by notification. While a pick is open the chip carries
`color-picker--picking` and a right click does nothing. A row carries no time, so the applet takes
no tick.

**places** — watches the `places` service handle alone. A place, a bookmark or a network share opens
through `gio::AppInfo::launch_default_for_uri`, off the main loop.

- **`tooltip-format` takes `{bookmarks}` and `{places}`.** `{drives}` moved to the removable applet
  when the two split, so it now renders as itself here.
- **Bookmark labels are capped the same way a tray title is** — `render::cap` runs
  `glimpse_utils::clean` before one reaches a row, because a `gtk-3.0/bookmarks` entry comes from
  another application.
- **Bookmarks end in an overflow row**, on the same footing as bluetooth's
  `more_paired`/`more_nearby`: expanding is `PopoverHandle` state, not a scroll.

**removable** — watches the `removable` service handle alone, and renders no chip at all when no
drive is attached, which is the common case. A mounted volume's card opens it through
`gio::AppInfo::launch_default_for_uri`; an unmounted one mounts from its row, and mount, eject
and unmount all report a failure the way every applet does, through `spawn_reported` and a
notification.

- **`tooltip-format` takes `{drives}` and `{volumes}`.**
- **Volume labels are capped the same way a tray title is** — a filesystem label is another
  application's string.
- **Capacity is a free-of-total string, never a percentage.** `render::capacity_text` reads
  `glimpse_utils::size::bytes` on both sides of "free of", and a volume with none left says so in
  words rather than printing `0 B free`, in amber. The filesystem, mount point and read-only access
  are facts in the card, never the subtitle.
- **Drives end in an overflow row**, expanding as `PopoverHandle` state rather than a scroll.

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
- **The bar shows only the current track label**, falling back to the player's name when that
  label is empty.
- **The optimistic value goes into `self.players`, not beside it**, so `dress` has one source.
- **`aimed` is a shared cell holding the current player's id**, which row signals carry too.
- **The other players section appears only when enabled and another player exists.**
- **An icon is a name the theme actually has**, checked with `IconTheme::has_icon`: `DesktopEntry`
  first, then the bus-name suffix whole and a segment at a time.

**keyboard** — the chip is the current layout's code, hidden under two layouts. The compositor owns
the list; this applet only renders it and sends the switch command.

**workspace-name** — the chip is the name of the workspace active on this bar's output, or its
index when it has none, falling back to the focused workspace when the output is unknown. It shares
`applets/workspace.rs` with the pager — `workspace_token` and the tooltip tokens — so the two never
disagree about a workspace. The
popover is one entry and asks for the keyboard with `Opener::typing` while it is up; Enter renames,
an empty entry clears the name, and Esc closes. The chip takes the new name before the compositor
answers and drops it on **any** answer, success included, re-reading the snapshot: niri refuses a
name another workspace already holds and still replies `Ok`, so only the snapshot knows whether the
rename happened. Scroll steps `compositor.focus_workspace` `Next`/`Prev`, the same as the pager's
own scroll in `PagerMode::Workspaces` — a chip this small has no strip to step over instead.

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

- **Devices is one list, the device in use first; Nearby devices is the same disclosure as Other
  networks** — no header until something is found, closed while anything is paired, open by
  itself when nothing is, and never an empty list or a spinner. A device in use is one row that opens its card, where Disconnect lives; a paired
  one connects on its body; a nearby one pairs on its body and has no card. A card closes when the
  action taken from it succeeds, through `collapse`. The two expanded flags are `Rc` cells the
  popover's closures write and `Input::Woken` reads back. `unmap` stops a scan unconditionally: any
  gate on published state loses a held one started in the last round trip. **An overflow row
  toggles to *Show fewer***: nothing scrolls, so one that only expands pushes the switch off the
  output.
- **A card holds only what a person acts on** — Disconnect, battery, codec, Connect automatically,
  Forget. A pairing that will not survive a restart is the row's own amber subtitle.
- **A pairing prompt is a page; `raised` keys its auto-open on the device id**, since BlueZ
  escalates mid-flow and a boolean would re-open one just dismissed. The dialog clears its entry on
  a change of **device** rather than of name, because BlueZ re-asks as a name resolves.
- **The chip and its tooltip take `attention` while a question waits**, or a dismissed popover was
  the only thing that knew it was asked. **`BondBroken` is stated on the row, not notified**: the
  bond is gone, so *Connect* fails until the device is paired again.
- **Only the two prompts needing an entry reach `App`**, narrowed by `render::typed`, so one the
  popover draws never trips `close_popovers`. Those that do close every popover, then title, size
  and **show** the host before `present`.
- **The popover being open is the scan and the visibility**, set on map, cleared on unmap, **never
  re-asserted between** — a wake that re-asked fights the timeout that just lapsed. There is no
  switch for either: a computer is visible exactly while someone is looking at its Bluetooth, and
  `chip` reads `state.held()`, so a popover's own scan never lights the bar.

**network** — the chip is the connection's own icon and **nothing else**: no SSID and no percentage,
both of which belong to the tooltip. No managed device renders nothing.

- **A VPN is a second chip, never an overlay**, since two overlays do not compose. **Metered is
  marked on the connected row**; the tooltip naming it is configuration and the chip never has it.
- **The password is asked for before the join, on a page of the popover**, because NetworkManager
  drops the working connection the moment activation is requested. **A request NetworkManager raises
  survives the popover being shut**; one the user began does not.
- **Ethernet, VPN, Wi-Fi, Other networks, then the hidden-network row.** Wi-Fi is the network in
  use, first whatever its strength, then every saved one in range; a saved network out of range is
  not listed. **Other networks is a disclosure whose header is the toggle**, closed while anything
  known is in range and open by itself when nothing is; an open list is capped at
  `visible-networks` and ends in a row that shows the rest. A stranger is one line, saying only
  *Open* or *Enterprise*, since the padlock already says secured. Strength is banded at render from the
  raw value, which the tooltip prints exactly. A failed command is a notification.
- **A connection in use is one row that opens its card; Disconnect lives only there**, so the row a
  person clicks most never drops the connection. Every other row joins on its body. Only a
  connection in use and a saved network have a card, and it repeats nothing the row already says.
- **A wired row is a device, not a profile**, so it routes to `connect_device`; sent to
  `connect_access_point` it is silently not found.

**audio** — needs a PulseAudio-protocol server, `pipewire-pulse` or PulseAudio proper, and renders
nothing without one.

- **Each direction is its fader, then one row naming the device in use, whose card lists every
  device; an app's whole row opens its card**, a fader and where it plays per direction. There is
  no readout and no device overflow: the header follows the output instead. A switch of the default
  device or a `move_app` closes its card on success, through `collapse`; volume and mute leave it
  open. The widget owns which card is open, so the applet passes every app's detail on each dress.
- **Muted is the muted glyph in the warning colour, everywhere** — each fader, the header, and an
  app row's trail — never a greyed icon or the word *Muted*. A capture fader and an app that only
  records carry the microphone's glyphs, not the speaker's.
- **A master fader carries no device id of its own**, so its `level-changed`/`level-toggled`
  resolve the current default device from a fresh `AudioHandle::snapshot()` at the moment the
  signal fires, never from a value captured when the popover was built.
- **A middle click toggles the default output's mute**, through the same `Input::Pointer` arm
  scroll uses — left click stays the runtime's popover toggle and is never matched here.
- **The Applications section hides entirely with nothing playing**, the same as Output and Input
  with no devices; it no longer stays open on a "Nothing is playing" placeholder, because unlike a
  quiet Bluetooth adapter or an empty privacy popover, silence here is the common case, not a
  reassurance worth a permanent header.
- **List caps are constants in `indicator.rs`, not configuration** — `AppletKind::Audio {}` carries
  no settings yet, unlike bluetooth's `devices`/`nearby`.

**brightness** — the chip renders as long as either a backlight source or a reachable night light
exists; an empty source list on its own is an ordinary desktop, not an error, and only the absence
of both hides the chip.

- **The current display resolves in three rungs**: the display source whose connector is the
  focused output, then the single source carrying no connector at all (the internal panel), then
  the first display source — `render::current_display` is the whole ladder and is what
  `BrightnessPopover::set_sources` is handed with that source first.
- **A display source whose output is disabled is filtered out before any of that ladder runs**,
  through `render::is_powered` against `OutputInfo.enabled`. A source with no matching output at
  all is kept — an unknown power state must not hide a real fader.
- **The chosen source is pinned for as long as the popover stays open.** `popover()` resolves the
  ladder once and holds the id; `dress` keeps feeding that id first even if focus moves to another
  output before release, because `BrightnessPopover::report_primary_changed` reads the key at emit
  time, not at press time, and a focus change mid-press would otherwise write to the wrong display.
- **The switch reads `schedule != "off"`, never `active`.** `active` is `temperature != DAY`, false
  every daylight hour under Automatic, so a switch bound to it would read off while the night light
  works. Turning it on sends `SetSchedule` with the snapshot's own `configured` mode rather than a
  hardcoded `automatic`.
- **`BrightnessPopover` already remembers the night light's last-good values across `None`** and
  only greys them; the applet feeds `night_light.current` straight through on every wake rather than
  holding a second copy. The *chip*, one layer up, latches its own "night light has ever been seen"
  bit instead, because `current` also drops to `None` for the duration of a provider restart and a
  backlight-less machine would otherwise lose its only chip along with it.
- **The temperature rail sends live, on `moved` as well as `changed`.** A colour has no readout but
  the screen itself, so a grab-release-wait cycle is not acceptable there the way it is for a
  percentage; `Fader::set_value` is already a no-op while held, so the service's own echo cannot
  fight the drag. Both signals route through the same `render::Coalescer`: one call in flight,
  everything that arrives while it is busy collapses to the latest value, and the call that follows
  always carries that value rather than every value in between. The primary and device faders still
  send only on `changed` — `BrightnessPopover` wires no `moved` for them yet.
- **`scroll-step` is a percent in config and native units on the wire, validated to 1..=100** so it
  can neither leave the wheel silently dead nor move several times the display's own range in one
  notch. `render::native_step` converts and rounds away from zero.

**display** — the chip is `video-display-symbolic` whatever the output count, and empty with none.
**The glyph deliberately does not track the count**: a chip that changes shape when a monitor is
plugged in reads as a different applet appearing, and the count is already in the popover. The applet only maps `CompositorOutputs` into
`glimpse_widgets::Display` and wires `enable-requested` to `compositor.set_output_enabled` and
`blanked` to `compositor.power_off_monitors` — never the other way around, since the first removes an
output from the layout and the second is DPMS and wakes on input. The last-enabled-output lock and
its readable subtitle are `DisplayList`'s own; the service refuses the command underneath it too.

**idle** — the priority table behind the hero subtitle is documented by `render.rs`'s own test names,
not restated here. The chip is never hidden while the daemon answers, unlike a notifier: it is the
only way to reach the five hold presets, so it must stay reachable even with nothing to report. The
switch is the indefinite hold, so no preset repeats it, and glimpse's own hold is never a row under
"Kept awake by". **The daemon keeps no end time**, so the applet remembers the one it asked for and
the hero and hold row read "Awake until 15:40" in the configured clock; a restart forgets it.
`render::icon` carries that distinction instead — **`view-conceal-symbolic` while idle is allowed,
`view-reveal-symbolic` while something holds the session awake**. The two states are one glyph
family on purpose: an open eye against a struck-through one reads as one thing changing, where the
alarm-clock-becoming-a-pause-button it replaced read as two unrelated applets.

- **A hold's id never reaches the applet directly** — `Hold()` is fire-and-forget, so
  `manual_hold_ids` derives the whole set from the next state by the daemon's own manual-hold
  literal. A derivation, never a remembered set: the applet keeps no id the provider has stopped
  reporting, and a second preset pressed while holding is ours rather than a stranger.
  **The adoption shape includes `can_release`**: any session-bus client can forge the same
  `who`/`why` strings via `logind.Inhibit()`, but the daemon never marks that record releasable, so
  it is never adopted and the toggle never sends a `Release()` the daemon would silently refuse. A
  restart still forgets the panel's own holds and renders them as plain rows.
- **`why` is capped tighter in `row_status` than the daemon's own 240 characters**, so a verbose
   reason cannot push a row's `(Flatpak via portal)`/`(systemd-inhibit · pid N)` marker off the end.

**battery** — the chip is `DisplayDevice`; internals, facts and the charge-limit switch come from
the first present `BAT*` object, because the composite omits them. Extra packs after that join the
device list. `indicator-style` is icon-only by default; `label-format` substitutes `{percentage}`, `{state}` and
`{remaining}`, and a format resolving to nothing drops the label rather than a bare separator.
UPower's `IconName` is the chip icon; the level ladder is only its fallback. A failed profile or
charge-limit command is a notification.
- **The hero says when, not what**: time left while draining, "Full at {time}" while charging
  (`[regional]` decides the clock), and "Held at N%" when an enabled charge limit is why it is not
  charging. It takes the chip's severity.
- **The charge limit is a control in the column**, never behind the health card. The health row
  shows UPower's `Capacity` percentage as its value, amber below 80%; its card holds what nothing else shows — energy,
  design capacity, cycles, voltage, technology, model, vendor — never the charge or time the hero
  already carries.
- A device row's subtitle is its charging state, since the icon already names the kind; it reads
  amber at 20% or less while not charging. Performance reads amber only when it is held back.

**command** — a user-defined chip: `icon` (a theme name or an absolute image path) and/or `label`,
and one argv per gesture (`on-click`, `on-middle-click`, `on-right-click`, `on-scroll-up|down|left|
right`), run once per scroll notch with no shell. A program that cannot start is one notification
per program per five seconds; its exit status is not watched. `popover::launch` hands the child an
activation token and drops a `LANGUAGE` only `[regional]` set.

**session** — icon-only, and the hero names the session type beside how long the user has been
signed in. Power actions confirm on the app host after the popover closes; lock and session switch
run immediately. Confirmation copy is formatted at click from the current snapshot,
not from the one that opened the popover. Inhibitors are named only when they apply to that action;
open windows are counted for log out, restart and shut down, never described as unsaved work.
Updates appear only while PackageKit owns its name, as a status row, never a count.

**privacy** — one row per application, titled by its name and icon, with what it uses in the
subtitle (`Camera · Microphone · Sharing Dell U2723QE`); a use with no application — location, or a
screencopy cast — is titled by the resource. A shared output is named by its display label from the
compositor, never its connector. A row holding a `PipeWire` cast opens a card whose *Stop sharing*
ends every session in it; nothing else opens, because a camera cannot be handed back to the process
holding it. `Mute microphone` mutes the default input through the audio service and shows only
while something records, and a row reads `muted` beside Microphone while it is. The popover closes
when nothing is in use, so it has no empty state. A chip
carries `Severity::Warning`, because a chip only exists
while something is watching or listening; there is no calm state to distinguish it from. The screen cast is the
exception: it takes `IndicatorSpec.class` instead, so `glimpse.css` can paint the record glyph in
the danger colour while the timer beside it keeps the bar's foreground — severity colours icon and
label together and cannot express that. A cast swaps the resource icon for `media-record-symbolic`
and labels the chip with how long it has been running,
which is the one privacy reading a user acts on. The clock counts from the **oldest** visible cast:
several casts collapse onto one chip, and the screen has been shared continuously since the first
began. `Usage.since` survives a refresh, so the count does not restart when the service re-reads its
sources. The applet paces itself — a second while casting, a minute otherwise — and a `since` in the
future, from a clock that jumped backwards, reads as `00:00` rather than panicking.

## Losing the session bus kills the process, and nothing here can change that

A panel whose session bus dies terminates with exit 143 (SIGTERM) and leaves **nothing at all** in
the log. That is not this crate's doing — every GTK application on the machine behaves the same way.
Do not re-derive it: the `closed` signal on the connection `g_bus_get_sync` returns never fires,
`set_exit_on_close(false)` changes nothing, and GLib prints no message of its own. The process is
gone before anything in `run` could speak.

**`Restart=on-failure` deliberately does not cover it.** systemd's `on-failure` excludes SIGTERM, so
the unit does not come back — right in both cases that reach it: a session bus that died is a
session that is ending, and an ordinary `systemctl stop` is the same signal.

## Rules

An applet renders typed service snapshots and sends typed commands through its injected handle. It
never opens a D-Bus connection, never reaches a backend directly, and holds no state that outlives
its own widget.

UI state never waits on a round trip: update the widget optimistically and let the service event
reconcile it. **A command that fails wakes its own applet**, because a mirror service publishes
nothing when a rejected command leaves its state byte-identical — the optimistic flip would then
stand until something unrelated moved. The wake re-dresses from the snapshot, which is what puts the
row back.

**One substituter renders every `{token}` format.** `applets/tokens.rs::render` walks the template
once, resolving each token through a closure the applet supplies; the applet decides its own token
names. Chained `String::replace` is the wrong shape — each replacement runs over the previous one's
output. Every value interpolated is compositor- or calendar-supplied text; the clock's
`tooltip_format` is strftime, not this.
