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

**The applet builder receives `Ctx` and captures its exact service handle.** It seeds the applet
from `handle.snapshot()`, then `Ctx::watch(handle.subscribe())` turns later changes into `Woken`
inputs. `Ctx` owns the forwarding task, so removing the applet cancels its subscription without a
second lifetime mechanism. Applets contain no socket, topic, or JSON routing path.

**A panic stops one applet, not the panel.** `handle` and `indicators` run inside one
`catch_unwind`; a panic logs, drops the applet, stops its sources and empties its group. Unwinding
past a `&mut self` mid-mutation leaves state nobody can reason about.

**`indicators()` is a pull** called after every `handle`, feeding `set_items`, which compares before
writing. An empty vector hides the group — that is how an applet says it has nothing yet, never a
placeholder.

**Scroll reaches an applet as whole notches.** The group emits raw deltas and a touchpad sends many
small ones; the runtime accumulates per axis and drains in whole units, so a wheel detent is one
notch and ten `0.4` deltas are four. The accumulator belongs to the group, so an applet whose
indicators change mid-gesture does not lose the remainder.

**Pointer input names no indicator.** The whole group is one clickable target, so `Input::Pointer`
carries only the button or direction.

**Zone reconciliation is keyed by `(zone, name, kind)`.** `MonitorsChanged` and `ThemeChanged` both reach
`reconcile_panels`, so the guard comparing desired against current key sequences is what stops every
applet being rebuilt on every theme write. Changing a custom applet's `extends` replaces that one
runtime so its captured typed handle changes with it. A name with no implementation still occupies a `Slot`
with `handle: None`, which is what keeps the sequences comparable. An unresolvable *name* is a user
typo, logged at `warn`; a name resolving to an unimplemented kind is expected and logged at `debug`
— the shipped default names nineteen applets, so collapsing the two means nineteen warnings on an
untouched install.

There is deliberately no staleness, no `degraded`, no timer and no applet `Output`.

### Keyboard

The chip is the current layout's code (`US`). It hides when the compositor service reports fewer than two layouts. Scroll cycles; left click opens a list of layouts with the current one checked and the code on the right, not bold. `{code}` and `{name}` fill `tooltip-format`. The footer is only the `settings-command` row, when that pair is set.

The compositor owns the list; this applet only renders `keyboard.layouts` and sends `keyboard.switch_layout`.

### Notifications

The chip is a bell. It hides until `notifications.list` has arrived, then stays even when the list is empty so do-not-disturb is still reachable. `indicator-style` is `icon-only`, `icon-dot` (the default), or `icon-counter`; the dot and counter hide under do-not-disturb. The bell stays in the bar's normal color; a critical unread notification colors only the dot. Do-not-disturb swaps the icon and mutes attention. `{count}` fills `tooltip-format`.

The popover groups by `app_id`, newest first, and formats translated relative time in `render.rs`. Its switch is on when notifications are active and do-not-disturb is off; turning it off enables do-not-disturb. `NotificationStack` leaves groups of up to three as individual cards and collapses groups of four or more; the collapsed card is only a preview, so its per-notification controls are hidden, left click opens the stack and right click clears that application without focusing it. On an individual unread card, left click invokes the specification's `default` action when one was offered, asks the compositor to raise the sender when `app_pid` is known, and dismisses the notification; right click and the close button remove it without either action or focus. Named buttons send only their action and token. Read history has neither activation nor action buttons, and its close button removes that record. Dismiss, remove, clear-app, clear-all and do-not-disturb go through the matching commands. The icon cache is pruned against each new list so sender-controlled keys cannot accumulate for the panel's lifetime. A degraded notifications service is the trouble banner. The footer is only the `settings-command` row, when that pair is set.

## The tick

`ctx.interval(period)` is the only timer an applet gets, delivering `Input::Tick`.

**A tick lands on the boundary**, not on whenever the panel started: `until_boundary` takes the time
since the epoch modulo the period. Without it a `%H:%M` clock changes up to a minute late, which
reads as broken rather than late. Measured with the tick instrumented: every tick landed 0–1 ms past
the whole second. Exactly on a boundary the wait is a whole period, so nothing renders twice.

**A missed tick is skipped, not burst.** tokio's default `Burst` would deliver 3600 ticks in one
pass of the main loop after an hour's suspend. `Skip` is also the only behaviour that keeps the
phase; `Delay` restarts the schedule from wherever the stall ended.

**Calling it again replaces the timer**, which is what makes it safe to ask for from `configure`.
A period of zero is refused and logged.

### The clock derives its period; it is not configured

`%H:%M` ticks once a minute and `%H:%M:%S` once a second, decided by scanning for a specifier faster
than a minute — `%S %T %X %r %c %+ %s %f`. A setting would be a second way to say what the format
already says, and the two could disagree.

Two are deliberate over-approximations: `%X` is the locale's own time and need not carry seconds (it
does under `pl_PL`, measured), and `%f` is sub-second, which a one-second tick cannot follow anyway.

The scan reads specifiers rather than substrings, which matters twice: `%-S` carries a padding
modifier, so `contains("%S")` misses it, and `%%S` is a literal percent, so `contains("%S")` matches
something that is not a specifier.

**A format string that cannot render must not panic.** chrono's `Display` for `DelayedFormat`
*returns an error* for an unknown specifier and `to_string()` turns that into a panic — which, under
`catch_unwind`, stops the applet for the session. It renders through `write!` instead and returns
`None`. Ticks log at `trace`, not `debug`.

**What no test covers:** that `configure` calls `ctx.interval`, and that a tick reaches
`indicators()`. Both need the relm4 runtime and a mapped panel; the arithmetic and format scanning
are covered headlessly.

## An applet may supply its own widget

`view()` returning `Some` replaces the group and `indicators()` is never called. The pager is the
first case: a click *per slot* over a list whose length changes.

**The root is a `gtk4::Box` carrying the `applet` class**, not the group — relm4's `init_root()`
takes no arguments, so the root cannot depend on an applet built later in `init()`.

**An applet that supplies a view receives no `Input::Pointer`.** The widget owns its pointer, which
is what lets the pager give each slot its own button and scroll axes.

**Orientation is handed to the applet, not applied behind its back.** Reaching into the view's own
`BoxLayout` turns the widget sideways without telling it, so it cannot restyle for the new axis.
Measured on the pager: a vertical bar stretched every dot across the column and lost the shape
saying which workspace is current, because the rule lengthening the active one is keyed on
`min-width`.

**Signals are wired in `view`, called once**, before the first `configure`. A GTK callback outlives
any `&Ctx`, so local applets capture a cloneable typed handle and start commands without blocking
GTK. Any settings a callback needs at click time live behind an `Rc<Cell<_>>` the applet updates in
`configure`.

**`ctx.output()` is the connector this bar is on**, `None` when the monitor has no name.

## The popover

`Applet::popover(&Seat)` builds the tree on open and the runtime drops it on close. Nothing is
cached. An open popover still follows events: the applet keeps a `glib::WeakRef` and pushes every
render into it — weak, because a strong reference would hold the tree alive past dismissal.

**An applet on the runtime's `IndicatorGroup` gets its popover opened for it.** `HostInput::Pressed`
delivers the press to the applet and then calls `show_popover` itself when the button was left, so
neither the clock nor the next-event applet calls `open_popover` or matches on the press.
`Opener::open_popover` is for the other shape: the pager wires the click inside `view()`, so the
press never reaches its `handle`.

### It is not a `Gtk.Popover`

`applet/catcher.rs` holds the container: a second layer surface anchored to all four edges of the
panel's monitor, covering everything the bars have not reserved, mapped only while a popover is up.
The reason is that **a `Gtk.Popover` on a layer surface cannot be dismissed by a click on another
application** — that dismissal is `xdg_popup.grab`, `autohide` is how GTK asks for it, and the grab
costs the keyboard. Measured: `KeyboardMode::OnDemand` plus `autohide(true)` leaves `focused-window`
at `None` for as long as the popover is open, which is the focus theft the previous generation was
reported for. Owning the surface buys outside-click dismissal, one popover at a time and an exit
animation, and costs hand-rolled placement and a drawn arrow.

One catcher per panel, shared by every applet on it, so **one popover at a time is structural**
rather than a rule someone enforces. `KeyboardMode::None`, so nothing is taken from the focused
window — the cost is that `Escape` dismisses nothing.

### Dismissal

A `GestureClick` on the catcher `pick`s the press and closes unless it landed inside the content.
Because the surface covers the output, a click anywhere else on the desktop is a press on *us*.

`open` takes the dismissal callback, so only the applet owning the current popover hears about it.
One listener per applet for the life of the panel was the earlier shape and leaked: an applet removed
by a config change left its closure — and its `Sender` — in the catcher forever.

The runtime asks `Catcher::holds` before acting. `shown` alone is not enough, because a replaced
applet still holds its handle until the queued `PopoverDismissed` reaches it, and in that window a
press on it would close *someone else's* popover.

### Placement

`Applet::anchor` names a widget inside the view; the runtime turns it into a centre coordinate with
`compute_bounds`. An applet naming nothing anchors to its whole box.

`placement()` is the whole arithmetic and is a free function so it can be asserted without a
display. It returns where the body starts and where the arrow sits inside it, and holds four
properties, one test each: the arrow's centre is the pressed item's centre; the body never leaves the
output; the body keeps a **gutter** from the output edge; the arrow never sits on the body's rounded
corner.

**The gutter yields to the arrow, and that ordering is the design.** A 418px popover anchored 28px
from the edge cannot both keep a gutter and put its arrow over the item. Clamping to zero looked
broken; a fixed gutter slid the arrow 17px off the item. So the gutter is `arrow`, shrunk to whatever
still lets the arrow reach — only an item closer to the edge than `arrow × 2.5` gives any up. Both
mutations fail a test.

One CSS length drives the arrow's size, its inset from the corner and the body's gutter, read back
from the measured arrow: they are proportional to each other visually, and none is written in Rust.

**Placement waits for the window, not for the slot.** A hidden layer surface has no room until the
compositor configures it again. The first open worked because everything started at zero; every open
after it did not, because closing hides the window while the slot keeps its previous allocation:

| | center | extent | room | slot | start |
| --- | --- | --- | --- | --- | --- |
| first open | 1537 | 418 | 3072 | 418 | 1328 |
| every reopen | 1537 | 418 | **0** | 418 | **0** |

The guard waits on `room()` and `settle` returns without touching a margin while it is zero. The
anchor was never involved — `center` was right both times, which is why this looked like an anchoring
bug and was not one. Reproducing it needs a mapped layer surface that has been hidden and shown
again, so nothing asserts it.

### The animation is `AdwTimedAnimation`, not a CSS transition

`opacity` on the slot, driven by `adw::TimedAnimation`. Measured in a nested niri:
`0 → 0.702 at 58ms → 1.000 at 158ms` opening, `0.961 → 0.157 at 50ms → 0.000 at 148ms` closing.

A CSS `transition: opacity` on the same node did not animate. Two properties of libadwaita's
animation are the reason not to go back: `done` is an exact clock, so the teardown timer and its
duplicated duration constant are gone; and an unmapped widget or `gtk-enable-animations: false`
makes `play()` skip to the end and emit `done` synchronously, which lets the state machine be one
path instead of two.

**A layer surface has no size and is not mapped until the compositor configures it.** An idle right
after `present()` measures `width=0`. `open` waits on a tick callback for a real allocation, settles,
then plays. This one fact caused three separate bug reports — popover at the screen edge, no arrow,
no animation — before it was found.

**The shadow is in `px`, and it has to be.** `box-shadow` with `rem` lengths renders **nothing** in
GTK4 — measured both ways — and fails silently.

**The arrow is a `Gtk.DrawingArea`, not a rotated box.** GTK4 has no triangle, and a square with
`transform: rotate(45deg)` overflows its allocation into the bar. Four lines of cairo point it at
whichever edge the panel is on; size and colour still come from CSS, and the fill reads
`gtk_widget_get_color`.

**The catcher takes `set_exclusive_zone(0)` and lets the compositor place it** — no margin, no
thickness, no measurement of the bar. `set_exclusive_zone(-1)` plus a top margin of `config.size`
assumes two false things: `Panel::set_thickness` calls `set_size_request`, a **minimum**, so a bar
whose applets need more room is taller than the configured number; and the panel is not necessarily
at the top of the output, because anything else holding an exclusive zone pushes it down. Measured
on a session also running the previous generation — legacy bar at `0..36`, this panel at `36..72` —
a catcher margined by `36` put the popover directly behind the panel's own bar, with the 9px arrow
completely hidden. No arithmetic over `config.size` can fix it, because the missing number is the sum
of every *other* surface's exclusive zone.

**A position change closes an open popover.** The anchor is a coordinate on one axis, and re-placing
a `Top` popover's x as a `Left` popover's y puts it somewhere arbitrary.

**Every panel position is a different layout** — orientation, arrow side, arrow direction and
placement axis all derive from `Position`. `Top` is verified live; `Bottom` and `Left` in a nested
compositor; `Right` not at all.

**Nothing asserts the state machine.** `placement`'s six tests are the whole of the automated
coverage; opening, the fade, dismissal and the side-change teardown need a mapped layer surface,
which a test must not create on the user's session.

## The calendar applets

`agenda.rs` owns everything about an event that is not a calendar: `Occasion`, the conversion off the
wire, `when`, and `row`, which turns one `Occasion` into the `glimpse_widgets::Event` both popovers
render — so a new field on that type is added in one place.

Twelve-hour detection and the two clock formats live in `glimpse-config`'s `environment.rs`, reached
as `glimpse_config::clock(twelve)`. An applet takes `twelve` from `config.regional.twelve_hour()` in
its own `configure`.

**`when` is a free function over `(now, day, event, clock)`** with one test per state, because an
event's time is not a value but a *sentence about now* — `now · ends 10:00`, `in 12 min · 1 h`,
`ended 12 min ago` — and which of the eleven applies changes while the popover is open. The ladder is
ordered so the first match wins, and three rungs fix things the previous generation got wrong: an
event that ended stays "ended 12 min ago" for an hour before falling back to "over"; one starting
within the minute reads "starting now" rather than "in 0 min"; and a timed event crossing midnight
names the day it ends rather than reporting a 36-hour duration.

**Events come from the shared `CalendarHandle` snapshot**, converted into `agenda::occasions`.
That conversion turns `DateTime<Utc>` local and the `color` hex into a `gdk::RGBA`, whose
failure costs that event its dot rather than the popover. Nothing caps the text again — the service
caps and flattens before publishing.

### The clock's popover

`CalendarPopover` owns the structure; the applet owns every string. It is rebuilt on every open and
dropped on close, so it holds no state between openings; what survives is on the applet. **The tick
re-renders an open popover**, because every relative string in it is a function of `now`.

**The popover is always local time, even on a clock with a `timezone`.** That setting moves the bar
label alone.

**The applet asks the calendar service for the months it is showing.** `ask_for_range` turns the shown month
into `[first of that month, first of the month after next)` and sends `calendar.set_range` from
`configure` and on every `Tick`/`Woken`, only when the range changed. The month step reaches it
because `CalendarPopover` re-emits `month-shown` and the applet wakes on it. Opening a popover
forgets the last range asked, so it reasserts the visible range after the local service has
recovered.

**Several panels share one service range, and the last to ask wins** — the command carries no client
identity. Each re-asserts on its own next step, so it self-corrects rather than sticking; bead
`glimpse-66sq`.

**A day past `truncated_from` says so instead of looking empty.** The calendar service caps the merged list at
512 events and reports the start of the first entry it dropped. The comparison is `>=`, because the
day the mark falls on is already missing entries.

**What no test covers:** that the click reaches the applet and the catcher shows the popover.
Verified by hand against a scratch configuration.

### The next-event applet

One indicator — a dot in the calendar's colour and the entry's title — and a popover holding that
entry, a countdown, and what follows it.

**Three windows, all in minutes, all through one `inside` predicate.** `within` decides whether the
applet is on the bar at all; `countdown` decides whether its label spells out how long is left;
`horizon` decides how far the popover's list reaches. `horizon` is read as `horizon.max(within)`,
because a bar wider than the list is a hero with an empty list under it. The list is capped at the
configured `upcoming` and again at 20.

`countdown` set below `within` is the two-stage reading: an entry appears quietly as a dot and a
title, then starts counting as it approaches. At the default they are equal, so anything on the bar
is also counting.

**The label reads `Design review in 12 min`, or `ends in 25 min` once it has started.** The suffix is
appended *after* the title is cut, so a long summary cannot eat the time. `Countdown` carries the
number, the unit and whether the event is running, and words itself twice from that — `in 12 min`
beside the title, `12` over `min` in the popover's `Readout` — rather than the label reformatting a
string the popover already built. An all-day entry has no minute to count and gets no suffix.

A day boundary was the obvious alternative and is the wrong shape: "today only" empties the list at
exactly the hour tomorrow's first meeting starts mattering. `horizon = 1440` recovers a full day.

**The window is why the applet is usually absent.** Anything further out than `within` leaves
`indicators()` empty and the group hides itself. An event that has already *started* stays regardless
of how long ago, so `within = 0` shows only what is under way and a meeting you are sitting in never
falls off.

**An all-day entry is demoted, and by default excluded.** Ordering by start alone would let a week of
leave outrank every meeting inside it. The sort key is `(max(start, now), end)`, which puts a running
meeting ahead of one that has not begun.

**A multi-day entry is counted from the day the reader is on.** `shown_day` clamps `now` into
`[start, end]` — today while running, its first day while still ahead — and `when` turns that into
"day 2 of 3". Anchoring to `event.start` made every row read "day 1" for the whole trip. A one-day
entry gets no counter.

**The bar label is cut with an ellipsis, not silently.** `Indicator`'s label carries `ellipsize: end`
and GTK shortens at rendered width — but only for a string that still overflows. A hard cut at 24
characters arrives already fitting, so GTK draws nothing and `Design review with the p` reads as the
title. `render::label` appends the mark when it cut.

**`indicators()` returns a cached vector.** The runtime pulls it after every `handle`, every
`configure` and once per scroll notch, so the scan runs in `refresh` where `&mut self` is available.
Choosing inside the pull put a 512-entry scan plus string building on the scroll path.

**`window()` saturates rather than borrowing the calendar service's limit.** The clamp exists so arithmetic on
a nonsense value cannot overflow, so it says that: anything that fits becomes itself, anything else
becomes `TimeDelta::MAX`, and `edge` saturates to the end of representable time rather than
collapsing to `now` — a window nobody could mean should include everything, not nothing.

**`tooltip-format` has its own tokens**: `{summary}`, `{detail}`, `{when}`. The substitution is a
single pass rather than chained `String::replace`, because a chain would substitute a `{when}` that
arrived *inside* an event's own title. Unset means no tooltip.

**It does not send `calendar.set_range`.** Two applets asking for different windows would overwrite
each other every tick — `glimpse-66sq` made continuous. The calendar service's default window already reaches
further than this applet looks.

**It always follows the locale for its clock.** `hour-format` is on the clock applet's table and is
deliberately not copied: two places to set one preference is worse than one place that does not cover
everything. Bead `glimpse-9dax`.

**What no test covers:** `dress` and `indicator` — the first needs a realised widget, the second a
`Ctx`, and both are wiring rather than decision. Everything they decide with is a free function in
`render.rs` with its own test. Verified by hand against a scratch calendar service: with `within = 60` a
meeting forty minutes out took the bar over one six hours out and one two days out; with
`within = 1` the applet disappeared; a config edit reached it without a restart. The click that opens
the popover is unverified — `ydotoold` is not running here, and the clock's popover has the same gap.

## The weather applet

One chip per place: an icon, a temperature and an optional tooltip. `render.rs` holds everything
that turns a `PlaceWeather` into strings — icons, condition wording, the nowcast, day and hour
labels, facts, drawer pages — so it is an ordinary `#[test]` with no display and no running services, and only
the wiring needs GTK.

**Several places is several applets.** `[applets.weather-home]` with `extends = "weather"` and its
own `[applets.weather-home.place]` is a second instance; the service holds no places of its own and
each applet leases the one it shows. Nothing new was needed for this — `extends` already did it.

`place` accepts `here`, fixed `latlon`, or a named `location` such as
`[applets.weather.place]` with `at = "location"` and `name = "Vilnius, LT"`. The weather service owns
GeoClue, forward and reverse resolution, and the canonical city/country result; the panel only sends
the typed request and displays that result. An explicit applet `label` still overrides it.

**The lease renews on a minute's tick against the provider's thirty-minute `LEASE`**, which is thirty
renewals of margin, so a panel that misses a tick or two never drops its place. This applet is that
constant's first consumer, and the pair is read together: if the tick slows, `LEASE` moves with it.
The typed handle call is fire-and-forget from GTK, which is right — a refused renewal (the
eight-place cap) is nothing the applet could act on, and the next tick retries.

**A fixed place still missing a whole tick after the provider took the name gets a warning chip.**
Rendering nothing is the deliberate answer for a place with no reading yet, and it must stay that
way: `here` may legitimately never resolve, and a place that has simply not been fetched is missing
for well under a tick. But a provider that holds the name, reports itself available and serves none
of the place we asked for is a different thing, and must not make the applet vanish from the bar.
`note_unserved` runs on the tick rather than on the snapshot, which is what keeps the ordinary
startup gap from flashing a warning.

**Units come off the typed provider snapshot, never off the panel configuration.**
`WeatherStatus.units` describes the numbers being rendered, so a units change cannot print °F over
a Celsius reading while the updated snapshot is in flight.

`[regional] units` is therefore read by `glimpse-weather` and by nothing in the panel.

The hour format has no such hazard — nothing round-trips to render a time — and it arrives the way
every other setting does: `AppletConfig.regional`, which `glimpse-config` copies onto each applet at
load. An applet writes `config.regional.twelve_hour()` in `configure` and that is the whole
mechanism. It is deliberately **not** on `Ctx` and not a parameter of `AppletHandle::launch`: a
setting the applet can read off the configuration it is already handed does not need a second route,
and a bare `bool` threaded through five signatures was the first attempt at this.

**The list starts at tomorrow and the strip at the next hour.** Today and the hour standing are
already the hero; a row and a column repeating them are the second telling, and each costs a slot
the strip and the list exist to look ahead with. The applet's `days` default is therefore one lower
than the service's `forecast-days`, which counts today — 7 shown needs 8 fetched, and that is why
the service default is 8.

**An alert takes the chip's icon and its colour.** The bar has room for one thing, and a warning
standing outranks the condition it is standing in; the temperature stays, so the chip does not stop
being a weather chip. `render::worst` picks the most severe alert rather than the first listed, so a
minor one cannot hide a severe one behind it.

**Sleet borrows snow's glyph.** The Adwaita icon theme ships no sleet symbolic, and naming one that
does not exist renders as a broken image rather than as nothing. The wording is its own — the word
is what tells you it is not snow.

**A weekday name is formatted through `LC_TIME`, not looked up in the message catalog.** A date
belongs to the locale, and `init_translations` has already called `setlocale`. Polish returns `wto`
and Russian `Вт` — lowercase in Polish, which is that language being correct rather than a bug.
`Tomorrow` is a real msgid because it is a word, not a date.

**No value yet is an empty `Vec`.** A fresh panel shows nothing rather than a placeholder, and the
group hides itself. A `here` place with no fix and a place the provider answered for with no current
reading collapse to the same absence, which is what the empty return is for.

**The icon is cached by name.** `indicators()` is a pull the runtime makes after every input, so a
`gio::ThemedIcon::new` per call is exactly the waste the applet rules name. Keying the cache on the
icon *name* rather than on `(Condition, is_day)` is what lets the alert icon share it.

## The mpris applet

One chip for the player the MPRIS service marked `current`, and a popover holding that player in full with
the rest listed beneath it. Which player is current, and which are hidden, are the service's
decisions in `[mpris]`; `[applets.mpris]` is only how the bar renders the one it is handed.

**Position is advanced here, not polled there.** MPRIS emits no change signal for `Position`, so the
service state carries `position_us`, the instant it was read, and `rate`, and `render::position` extrapolates
from those. `ctx.interval` is the timer — a second while something is playing, a minute otherwise,
asked for again on every update because asking replaces the timer rather than adding one.

**`label-format` substitutes by name.** `.replace` per placeholder, never `format!` into the msgid,
because a translator has to be free to reorder them. A placeholder with nothing behind it renders as
nothing, which leaves a format like `{artist} — {title}` reading as ` — ` for a video with no
artist — so `render::trimmed` returns `None` for a label carrying no alphanumeric character at all.

**A label that renders to nothing leaves an icon-only chip, not an absent applet.** A stream with no
metadata is still the thing making the noise, and removing the indicator takes the popover off the
bar with it; `label-format = ""` is therefore also how you ask for an icon-only chip. `tooltip-format`
reads the same placeholder vocabulary, as it does on the clock, weather and next-event applets.

Substitution is a chain of `.replace` calls, which is what `pager/label.rs`, `weather/render.rs` and
`next_event/render.rs` all do, and it means a value substituted early can contain a placeholder
substituted later — a player whose `Identity` is literally `{title}` gets its title rendered twice.
The consequence is duplicated metadata in a label and nothing else: there is no markup and no escape
here. This is the fourth copy of that mechanism and the first whose values are chosen by another
application, which is the argument for one left-to-right resolver in `applet/` rather than against
touching it — the reason it is still four copies is that three of them are outside this change, not
that four is the right number.

**The cap counts characters.** Track titles are chosen by whatever is playing and are unbounded, and
a byte slice through a multi-byte title panics. `Indicator` already truncates at `LABEL_MAX_CHARS`,
so this is a second, configurable cap in front of a backstop rather than the only thing standing
between a hostile title and the bar.

**`Transport` holds no state it is not given**, so pressing shuffle or repeat says only that the
button was pressed — the next value has to be computed from what is currently true. A signal closure
cannot reach `&mut self`, so the press is pushed onto `pressed` and `opener.wake()` brings it back
as `Input::Woken`, where `press` has the model in hand.

**The optimistic value goes into `self.players`, not beside it.** `dress` writes the transport from
that model on every refresh, so a value written anywhere else is overwritten by the next wake with
whatever the service last published — which looks like the button springing back. Writing it into the
model moves the button now and lets the next typed MPRIS snapshot reconcile it, which is what the
"UI state never waits on a round trip" rule in `AGENTS.md` asks for. It also makes two presses
inside one round trip advance twice instead of sending the same value again.

**The scrubber and the footer read `aimed` instead**, a shared cell holding the current player's id.
They need no value computed against the model — a seek position and "raise this" are complete on
their own — so they call the captured `MprisHandle` asynchronously.

**Row signals carry the player's id.** `PlayerList` reports which player was clicked rather than
which position, and reads that key back at the moment the row fires, because rows are reused in
place. The applet therefore keeps no parallel list to resolve a position against, and there are no
two orderings to hold in step.

**`show-others = false` hides the section rather than emptying it.** An empty `Section` shows its
placeholder, so passing an empty slice would answer a viewer who turned the list off with "Nothing
else playing" — `MprisPopover::set_others` takes an `Option` for exactly that reason.

**The footer raises the current player** rather than opening settings — it is what the popover's
design puts there, and `CanRaise` false hides the row. This applet therefore does not read
`config.common.settings()`, which every other applet with a popover does.

**Artwork goes through `glimpse_widgets::artwork`,** which crops it square so `Gtk.Image` has
nothing to letterbox, and is cached against its path so a track decodes once rather than once per
tick. It is decoded only while the popover is alive to show it — `refresh` returns after the chip
when `shown` no longer upgrades, so a bar with nothing open reads no files at all.

**Elapsed time is formatted by `glimpse_widgets::clock`,** the same function the popover's
`Scrubber` uses. Two spellings of one position disagree by a second on the track the viewer is
looking at, which is the whole reason that function is public rather than `pub(crate)`.

**An icon is a name the theme actually has.** `DesktopEntry` first, then the bus-name suffix whole
and a segment at a time — `chromium.instance4181` carries the application's name in front of a
number nobody ships an icon for. Each candidate is checked with `IconTheme::has_icon`, because an
unresolvable name renders as a broken-image glyph, which reads worse than the category icon it falls
back to. `gio::DesktopAppInfo` is not bound in gtk-rs's `gio`, so a desktop entry whose `Icon=`
differs from its file name is not followed; that is the one case this misses.

Resolved names and their `gio::Icon`s are held for **every** player, not only the one on the bar, for
the reason `Weather` holds its own: `indicators` is pulled after every input, so walking the
candidate list once per render is exactly the waste the applet rules name — and the popover's list
would otherwise be exempt from a rule the chip beside it obeys. The chip, the hero and the rows all
read the same held name, so none of them can disagree about a player's icon.

The cache is keyed on the desktop entry as well as the id. `DesktopEntry` is read fresh on every
property change and a player may answer it late, so an id-only key would cache the fallback icon on
the first read and keep it for that player's whole life.

## Reconciliation settles every slot, on both paths

`reconcile_applets` has two: one for a config change that left the applet list alone, and one that
rebuilds it. A slot surviving a rebuild is moved across as it stands, so the rebuild path used to
append it without handing it the new configuration — an edit that added an applet *and* changed
another's settings applied only the first half. Both paths now go through one `settle`; the runtime
compares before writing, so settling a just-launched slot costs nothing.

## Translated wording

`agenda::when`, `agenda::span` and `next_event::Countdown` write the sentences a person actually
reads — "All day", "now · ends in 12 min", "in 3 h" — so each is a `gettext` call with named
`{placeholders}` filled by `.replace`, not a `format!`. A translator has to be able to move the
number, and in several languages the unit precedes it.

`Countdown` carries `unit` and `readout_unit` as owned `String`s because both are translated at
construction; `readout()` still hands out `(&str, &str)`, so no caller changed.

With no catalog loaded, `gettext` returns its own msgid. Every existing assertion on this wording
keeps passing unchanged, which is why these functions needed no test edits.

## Losing the session bus kills the process, and nothing here can change that

A panel whose session bus dies terminates with exit 143 (SIGTERM) and leaves **nothing at all** in
the log. That is not this crate's doing and cannot be fixed here — every GTK application on the
machine behaves the same way.

Do not re-derive this; all three obvious guesses were checked. The `closed` signal on the connection
`g_bus_get_sync` returns never fires, so there is no point at which Rust code could log the reason.
`set_exit_on_close(false)` on that connection changes nothing. GLib prints no message of its own on
the way out. The process is gone before anything in `run` could speak.

**`Restart=on-failure` deliberately does not cover it.** systemd's `on-failure` excludes SIGTERM, so
the unit does not come back — which is right in both cases that reach it: a session bus that died is
a session that is ending, and an ordinary `systemctl stop` is the same signal. There is no case
where restarting after a SIGTERM is wanted, so the policy stays.

## Rules

An applet renders typed service snapshots and sends typed commands through its injected handle. It
never opens a D-Bus connection, never reaches a backend directly, and holds no state that outlives
its own widget.

UI state never waits on a round trip. A slider updates its widget immediately and sends the command;
the watch update that follows is reconciliation, and the service's value always wins.

Update properties on existing widgets. Rebuilding trees per event is the most likely source of
visible stutter.

A bar's identity is its position in the `panels` array paired with the monitor's connector name;
everything else is reconfigured in place. A monitor GDK cannot name gets no bar rather than one
reconcile cannot find again. Repointing a mapped layer surface at another output remaps it, so
`set_monitor` is called only when the output actually changed.

Transient notification surfaces belong to the independent `glimpse-notifications` process, so panel
reconciliation and monitor hotplug cannot interrupt popup delivery.

CSS providers are installed once and reloaded in place; installing twice stacks every rule. Every
provider connects `parsing-error`, because GTK4's loaders return nothing.

A programmatic state change must not re-emit its signal, or the handler that sends the command
re-enters itself.

Each panel-local service owns its last snapshot and availability state. A backend disappearance
stops updates or degrades that service without blocking GTK; backend-specific recovery stays in the
existing service implementation. Notification and weather applets consume typed provider handles
owned once at panel scope, so every applet shares one session-bus connection and owner follower.

The normal application ID is unique, so a second `glimpse-panel` activates the existing process
instead of duplicating backend subscriptions and polling. `GLIMPSE_PANEL_APP_ID` deliberately opts a
development run into a separate application identity.

Configuration is the `[panel]` table of the shared `config.toml`, plus `panel.css`. Tables owned by
other binaries are ignored, not validated.
