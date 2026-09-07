# glimpse-panel

The panel: layer-shell bars, applets, popovers and notification popups.

Builds the binary named `glimpse`.

## Contents

- `main.rs` — GTK application, layer-shell setup, one bar per output across hotplug
- `app.rs` — one bar per (panel config × monitor), the shared `Client`, config and theme watches
- `components/panel.rs` — bar window, zones, applet reconciliation
- `applet/` — the applet framework: the trait, `Ctx`, the relm4 runtime, and `popover::run`, which
  launches the `settings-command` every popover's footer row offers
- `applets/` — one module per applet, plus the registration match; `pager/` renders workspaces or
  windows into its own `Pager` widget rather than into indicators, and `agenda.rs` holds what the
  clock and next-event applets both need to say about a calendar entry
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

## The tick

`ctx.interval(period)` is the only timer an applet gets. It delivers `Input::Tick`, and the clock is
what brought it in — `indicators()` is a pull the runtime makes after every `handle`, so an applet
whose value is the wall clock had nothing to make it run and rendered whatever the time was when the
panel started.

**A tick lands on the boundary, not on whenever the panel happened to start.** `until_boundary`
takes the time since the Unix epoch modulo the period, and `interval_at` starts there, so a
minute-long period fires at `:00` rather than 12.4s into every minute. Without it a clock reading
`%H:%M` would change up to a minute after the minute did, which reads as a broken clock rather than
a late one. Measured live with the tick instrumented: every tick landed 0–1 ms past the whole
second. Exactly on a boundary the wait is a whole period rather than zero, so nothing renders the
same instant twice.

**A missed tick is skipped, not burst.** tokio's default is
`MissedTickBehavior::Burst`, which fires one tick for every period that went by while the timer was
not polled — so resuming from an hour's suspend would deliver 3600 ticks in a single pass of the
main loop, each one a full re-render. `Skip` is also the only behaviour that keeps the phase: it
snaps back onto the original schedule rather than restarting it from wherever the stall ended, which
is what `Delay` does.

**Calling it again replaces the timer rather than adding one.** `Ctx` holds it in its own slot,
separate from the subscription guards, because `configure` runs on every configuration change and an
applet that asked for a tick each time would otherwise accumulate one per reload. Dropping the guard
aborts the task, and `shutdown` counts it alongside the subscriptions.

**A period of zero is refused** and logged, rather than spinning the main loop.

### The clock derives its period; it is not configured

`%H:%M` ticks once a minute and `%H:%M:%S` once a second, worked out by scanning the format for a
specifier that moves faster than a minute — `%S %T %X %r %c %+ %s %f`. A setting would be a second
way to say what the format already says, and the two could disagree.

Two of those are deliberate over-approximations, because ticking too often is a wasted render and
ticking too seldom is a wrong number on screen. `%X` is the locale's own time and need not carry
seconds — it does under `pl_PL`, measured — and `%f` is sub-second, which a one-second tick cannot
follow anyway; it is in the set so that a format built on it moves at all.

The scan reads specifiers rather than searching for substrings, which matters twice: `%-S` carries a
padding modifier between the `%` and the `S`, so `contains("%S")` misses it, and `%%S` is a literal
percent followed by an `S`, so `contains("%S")` matches something that is not a specifier at all.
Both are pinned by tests.

**A format string that cannot render is not allowed to panic.** chrono's `Display` for
`DelayedFormat` *returns an error* for an unknown specifier, and `to_string()` turns that into a
panic — which, under the runtime's `catch_unwind`, stops the applet permanently. So a `%Q` in
`label-format` would have killed the clock for the session. It renders through `write!` into a
`String` instead, returns `None` on failure, and the applet returns an empty `Vec` so the group
hides itself. The warning is logged once per configuration change, not once per tick.

Ticks are logged at `trace`, not `debug`: a second-long period would otherwise put a line a second
into a `--log debug` run, and the one thing worth seeing at `debug` — that the timer started, and
with what period — is already logged once by `interval`.

**What no test covers:** that `configure` actually calls `ctx.interval`, and that a tick reaches
`indicators()`. Both need the relm4 runtime and a mapped panel. The arithmetic and the format
scanning are covered headlessly; the wiring was verified by running the panel against a scratch
configuration and watching the bar advance, and by instrumenting the tick to measure its alignment.

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

### Placement waits for the window, not for the slot

`settle` needs the room the surface has, and a hidden layer surface has none until the compositor
configures it again. The first open worked because everything started at zero and the tick callback
waited for `slot.width() > 0`; every open after it did not, because closing hides the window while
the slot keeps its previous allocation. So the guard passed on stale data, `placement` clamped
`start` into `0..=0`, and the popover jumped to the panel's leading edge.

Measured across two opens, with only one input different:

| | center | extent | room | slot | start |
| --- | --- | --- | --- | --- | --- |
| first open | 1537 | 418 | 3072 | 418 | 1328 |
| every reopen | 1537 | 418 | **0** | 418 | **0** |

The guard now waits on `room()` — the window's own extent along the axis that matters — and `settle`
returns without touching a margin when the room is still zero, so nothing can place a popover
against a size it has not been told yet. The anchor was never involved; `center` was right both
times, which is why this looked like an anchoring bug and was not one.

Nothing asserts it: reproducing it needs a mapped layer surface that has been hidden and shown
again. `placement` already answers `(0, 0)` for an unmeasured surface and is tested for it — the
defect was calling it at all.

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

## The clock's popover

`CalendarPopover` owns the structure; the applet owns every string in it. The split falls where it
does because an event's time is not a value but a *sentence about now* — `now · ends 10:00`,
`in 12 min · 1 h`, `ended 12 min ago` — and which of the eleven sentences applies changes while the
popover is open.

**`when` is a free function over `(now, day, event, clock)`**, and lives in `applets/agenda.rs`
rather than under `clock/`, because the next-event applet reads the same sentences. That module
owns everything about an event that is not a calendar: the `Occasion` type, the conversion off the
wire, the twelve-hour detection, the two clock formats, and `row`, which turns one `Occasion` into
the `glimpse_widgets::Event` both popovers render — so a new field on that type is added in one
place. `clock/popover.rs` keeps what only a month grid needs. It has one test per state, because that
is the whole of the logic and none of it needs a widget. The ladder is ordered so the first match
wins, and three of its rungs exist to fix things the previous generation got wrong: an event that
ended stays "ended 12 min ago" for an hour before falling back to "over"; an event starting within
the minute reads "starting now" rather than "in 0 min"; and a timed event crossing midnight names
the day it ends rather than reporting a 36-hour duration.

**The popover is rebuilt on every open and dropped on close**, which the catcher requires, so it
holds no state between openings. What survives is on the applet: the selected day and the events,
both behind an `Rc` so the `day-selected` closure and the tick can each render without borrowing
`&mut self` — the same shape the pager uses for its settings.

**The tick re-renders an open popover.** It has to: every relative string in it is a function of
`now`. `handle` upgrades a `WeakRef` to the shown popover and does nothing when it has been dropped.

**`TWELVE` is `%-I:%M %p`, not `%l:%M %p`.** `%l` is space-padded, so every twelve-hour time this
crate composes carried a leading space into the middle of a sentence — ` 3:30 PM · 1 h`. The `-`
flag suppresses the pad at the source instead of each caller trimming the result.

**The popover is always local time, even on a clock with a `timezone`.** That setting moves the bar
label alone. A second clock showing Tokyo should not also claim your calendar is in Tokyo, and the
events it lists are local by definition.

**Events come from `calendar.events`.** `topics()` names it, the runtime subscribes at build time,
and `Input::Topic` decodes the payload into `agenda::occasions`. The conversion is where the wire
type stops: `DateTime<Utc>` becomes local, and the `color` hex string becomes a `gdk::RGBA` through
`RGBA::parse`, whose failure costs that event its dot rather than the popover. Nothing here caps the
text again — the service caps and flattens before publishing, and a second cap would be an untested
copy of a rule that already has one.

**The applet asks the daemon for the months it is showing.** `ask_for_range` turns the calendar's
shown month into `[first of that month, first of the month after next)` and sends
`calendar.set_range`; it runs from `configure`, and again on every `Tick` and `Woken`, sending only
when the range actually changed. The month step reaches it because `CalendarPopover` re-emits the
calendar's `month-shown` and the applet wakes on it, the same route `day-selected` already took —
an applet has no `ctx` inside `popover()`, so waking is how a widget signal turns into a command.
With no popover open the range falls back to the current month, so a panel left running across a
month boundary re-asks on the next tick. Opening a popover forgets the last range asked, which is
what re-asserts it after a daemon restart: nothing tells an applet the daemon went away — a dead
daemon just stops sending — so a panel that only asked on change would keep browsing a month the
new daemon had never been told about. A fixed window in the daemon is what made December render
empty for a weekly meeting that was certainly there.

**Several panels share one range, and the last one to ask wins.** The command carries no client
identity and `calendar.events` is one topic, so two clock applets browsing different months
overwrite each other. Each re-asserts on its own next step, so it self-corrects rather than sticking
— bead `glimpse-66sq`.

**A day past `truncated_from` says so instead of looking empty.** The daemon caps the merged list at
512 events and reports the start of the first entry it dropped; `truncated` compares the shown day
against that instant in local time, and `set_day_truncated` swaps the placeholder. Without it a busy
calendar renders a silent empty month, which reads as a panel bug rather than as a limit. The
comparison is `>=`, because the day the mark falls on is already missing entries.

**An applet on the runtime's `IndicatorGroup` gets its popover opened for it.** `HostInput::Pressed`
delivers `Input::Pointer(Pointer::Press(..))` to the applet and then calls `show_popover` itself when
the button was left, so neither the clock nor the next-event applet calls `open_popover` and neither
needs to match on the press at all. `Opener::open_popover` is for the other shape: the pager supplies
its own widget through `view()` and wires the click there, so the press never reaches its `handle`.
This was the reverse once, and a clock whose `handle` only matched `Input::Tick` did nothing when
clicked — the runtime absorbing it is what fixed that class of bug rather than each applet
remembering.

**What no test covers:** that the click reaches the applet and the catcher shows the popover. It was
verified by hand — the panel run against a scratch configuration, the indicator clicked, and the
result read off the screen and out of the debug log.

## The next-event applet

One indicator — a dot in the calendar's colour and the entry's title, truncated at 24 characters —
and a popover holding that entry, a countdown, and what follows it. It subscribes to the same
`calendar.events` topic as the clock and reads the same `agenda` module, so the two never disagree
about what an event's time says.

**The bar is held to a window, and the window is why the applet is usually absent.** `within` is
minutes before an event starts; anything further out leaves `indicators()` returning an empty
vector, and `IndicatorGroup` then hides itself. A meeting two days away has nothing useful to tell
a bar, and showing it anyway was the first thing to go wrong. An event that has already *started*
stays regardless of how long ago that was, so `within = 0` shows only what is under way, and a
meeting you are sitting in never falls off. `window()` clamps the configured value to 400 days —
the widest range the daemon will expand — so no arithmetic on it can overflow.

**An all-day entry is demoted, and by default excluded.** Ordering by start alone would let a week
of leave outrank every meeting inside it, because its start is earliest and it never ends; `next`
therefore looks for a timed event first and only falls back to an all-day one when `all-day = true`.
The sort key is `(max(start, now), end)`, which is what puts a running meeting ahead of one that
has not begun.

**It does not send `calendar.set_range`.** The clock does, and the command carries no client
identity — two applets asking for different windows would overwrite each other on every tick, which
is the standing `glimpse-66sq` problem made continuous rather than transient. The daemon's default
window already reaches further forward than this applet looks, so there is nothing to ask for.

**The bar label is cut with an ellipsis, not silently.** `Indicator`'s own label carries
`ellipsize: end`, so GTK shortens it at whatever width the bar allows — but only for a string that
still overflows. A hard cut at 24 characters arrives already fitting, GTK draws no ellipsis, and
`Design review with the p` reads as the title rather than as a truncation. `render::label` therefore
appends the mark itself when it cut anything.

**`indicators()` returns a cached vector.** The runtime pulls it after every `handle`, every
`configure` and once per scroll notch, so the scan that picks the next event runs in `refresh` —
where `&mut self` is available — and `indicators` only clones the result. Choosing inside the pull
put a 512-entry scan plus a `when()` and a `tooltip()` on the scroll path, which is the one place an
applet can stall a frame. `refresh` also re-dresses an open popover, so the two never disagree about
which event they are showing.

**`window()` saturates instead of borrowing the daemon's limit.** An earlier cut clamped the
configured minutes to 400 days on the grounds that the daemon expands no further, which stated a
protocol invariant as a panel literal and coupled two numbers that are not the same number. The
clamp here exists only so arithmetic on a nonsense value cannot overflow, so it says that: anything
that fits becomes itself, anything that does not becomes `TimeDelta::MAX`, and `edge` saturates to
the end of representable time rather than collapsing to `now` — a window nobody could mean should
include everything, not nothing.

**`tooltip-format` has its own tokens**: `{summary}`, `{detail}` and `{when}`. The substitution is a
single pass rather than chained `String::replace` calls, because a summary comes out of a `.ics`
file the user did not write — a chain would substitute a `{when}` that arrived *inside* an event's
own title. Unset means no tooltip, which is what `Common` documents.

**A multi-day entry is counted from the day the reader is on, not from its own start.** One entry
covering three days is one entry, so the hero and its row have to say *which* day of it this is.
`shown_day` clamps `now` into `[start, end]` — today while it is running, its first day while it is
still ahead — and `when` turns that into "day 2 of 3". Anchoring it to `event.start` instead, which
is what the first cut did, made every row read "day 1" for the whole trip. A one-day entry gets no
counter: "day 1 of 1" is a number that says nothing.

**There are two windows, and the list gets the wider one.** The bar answers "is something about to
happen" and is held to `within`; the popover answers "what does the rest of this look like" and
reaches `horizon`, twelve hours by default. Both are minutes and both go through the same `inside`
predicate, so there is one definition of what "in the window" means and one place the clamp lives.

A day boundary was the obvious alternative and is the wrong shape: "today only" empties the list at
exactly the hour tomorrow's first meeting starts mattering, so a glance at 22:00 shows nothing while
an 08:00 standup is the thing you wanted. A rolling horizon has no such cliff, and `horizon = 1440`
recovers a full day for anyone who wants one. The list is capped at the configured `upcoming` and
again at 20, because a configured length nobody could mean is still a popover taller than the
screen.

`horizon` is read as `horizon.max(within)`. A document may set the bar wider than the list, and the
result is a hero showing an event with an empty list under it — incoherent rather than merely
unusual, so the narrower of the two is raised instead of rendered.

**The next-event applet always follows the locale for its clock.** `hour-format` is a setting on the
clock applet's own table, and it is deliberately not copied here: two places to set one preference is
worse than one place that does not cover everything. Moving it up to `Common` is the fix when a
second applet needs it — bead `glimpse-9dax`.

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
