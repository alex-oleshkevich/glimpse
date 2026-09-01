# glimpse-widgets

Shared GTK4 widgets: GObject subclasses, Blueprint templates and the CSS they expect.

Used by the panel and the lock screen.

## Layout

- `src/<widget>/` — one directory per widget, `mod.rs` plus `imp.rs` for the GObject boilerplate
- `blueprints/` — `.blp` templates, compiled by `build.rs` through `blueprint-compiler`
- `resources/widgets/` — the `.ui` files that compilation produces, bundled into
  `glimpse-panel.gresource` by `glib-build-tools`. Generated; never edited by hand.

Every template is bundled under the resource prefix `/me/aresa/GlimpseShell`, so a `.blp` is only
reachable once its `.ui` is listed in `resources/glimpse-panel.gresource.xml`. Adding a blueprint
means adding both the `build.rs` pair and the manifest entry.

`build.rs` only compiles; it does not lint, so `just lint` runs `blueprint-compiler lint` over every
template as well as clippy. A template whose accessible name lives on the composite widget rather
than on a child marks that child `accessible-role: presentation` — giving the child its own label
instead silences the same warning by making a screen reader announce the name twice.

## Indicators

`Indicator` is one visible chip in a bar: an icon, an optional label, an optional badge. It takes
values and emits nothing. Pointer and keyboard input belong to the group, not to the chip — an
applet is one clickable thing however many chips it happens to render, so a chip has no gesture
controller, no signals, and the `Generic` accessible role. The accessible name moved up with the
input: naming the chips would leave the one element assistive technology treats as a button
unnamed, so the group composes its name from what its chips show.

The icon is a single `Option<gio::Icon>` rather than one property per source. `gdk::Texture`
implements `gio::Icon`, so a themed name (`gio::ThemedIcon`), a file (`gio::FileIcon`) and a
StatusNotifierItem's raw ARGB pixmap all arrive through the same setter and the same
`Image::set_from_gicon`. Sniffing a string for a leading slash to tell a path from an icon name —
what the previous generation did — guesses wrong on a themed name containing one.

Every setter compares before it writes, `set_icon` through `Icon::equal`. That is what lets a caller
re-apply an entire `IndicatorSpec` on every update without any of it reaching GTK, and it is why the
group below can be careless about how often it applies.

Labels and badges are truncated to `LABEL_MAX_CHARS`, tooltips to `TOOLTIP_MAX_CHARS`, and all three
are set as plain text. There is deliberately no markup setter: tray titles and MPRIS metadata come
from other applications and are unbounded. A tooltip gets the longer budget because showing what a
truncated label cut off is the job it exists for.

`IndicatorSpec` holds a `gio::Icon` and so is not `Send`. The value crossing a channel from a tokio
task stays plain data — a name, a path, or pixel bytes — and becomes an icon on the GTK thread.

## IndicatorGroup

`IndicatorGroup` renders zero or more indicators for one owner, which is the shape a bar slot
actually needs: a tray shows one per running item, a privacy slot shows nothing at all while
nothing is recording.

The group is the interactive element: it is focusable, carries the `Button` accessible role, and
owns the click, scroll and key controllers, emitting `pressed(button)` and `scrolled(dx, dy)`.
Deciding what a press means belongs to whoever owns the group. Enter and Space emit `pressed` with
button 1, so keyboard activation arrives as an ordinary left click rather than as a separate path
every consumer has to handle.

The group is focusable and `:focus-visible` is styled, because a focusable widget with no visible
focus ring is worse than one that cannot be reached at all.

`set_items` takes the whole desired list and reconciles it **by position**: index _n_ of the new
list is applied to the widget already at index _n_, extras are created, and the tail is unparented.
An earlier version keyed on an `IndicatorSpec::id`, which existed only because each chip carried its
own signal closure capturing that id — reusing a widget under a different id would have reported
presses under the wrong one. With input owned by the group no closure captures anything, so the id
had no second job and was removed along with the duplicate-id skip it required. Position reuse keeps
a value change to a property write rather than a rebuilt subtree, which is all the id was buying.

Placement is one `Widget::insert_after` call, which both parents a new child and reorders an
existing one.

An empty group sets itself invisible rather than merely rendering nothing. A visible empty widget
still draws its own padding and still counts toward the enclosing section's spacing, which shows up
as a gap between its neighbours.

Orientation is settable because `Panel` flips between horizontal and vertical; spacing is not,
because nothing varies it.

## Calendar

A month grid with a year view behind it. `var/design/calendar.md` holds the layout reasoning and the
comparison against the approved mockup; what follows is what the code decides.

**The grid is always six weeks.** A month needs four to six, and a grid that tracked that would
resize the popover under the pointer that is scrolling it.

**Four measurements are tokens on `.calendar` itself** — `--gl-calendar-control` for the header
buttons, `--gl-calendar-cell` for a day or month, `--gl-calendar-radius` for both, and
`--gl-calendar-gap` between a number and its dots. Each was written out in two or three rules, and
they are the values anyone retuning the calendar reaches for first. A custom property inherits down
the widget tree, so `.calendar__day` reads one declared on `.calendar`; the rest of the calendar's
lengths appear once each and stay where they are used.

The selection ring is deliberately **not** a token: it is `2px` inside a `box-shadow`, and
`only_hairlines_and_borders_are_measured_in_pixels` allows `px` by property name. Hoisting it would
move that `2px` into a custom-property declaration the rule can no longer recognise as a border.

**The day cell is square**, 3.1rem — 45px at the default font, a little larger than the previous
implementation's 40px because a panel popover is read at arm's length and clicked in passing. It is centred in its column rather than filling it: the grid is homogeneous and each
column is wider than a cell, so a stretched button renders a square `min-width`/`min-height` as a
rectangle. Geometry is `rem` rather than `px` throughout, so a cell grows with the text inside
it; a fixed cell holding scaling text overflows at the first accessibility setting.

**Today is a fill and selected is an outline**, so a day that is both still reads as both, and
today-and-selected swaps the outline to `--gl-knob` against the accent. The previous implementation
drew today as an outline, which leaves nothing distinct for selection.

**`Today` appears only off the current month and reserves no width.** The controls are anchored to
the right edge, so inserting it grows the group leftward and the arrows never move. `Row`'s check
column needs reservation because a row is anchored left and the column precedes the label — opposite
anchor, opposite answer.

**Month names use `%OB`, not `%B`.** `%B` is the form a date is built from: in Polish, `września`
("the 1st of September") against the standalone `wrzesień`. A title wants the standalone one. English
does not distinguish them, which is exactly what makes this easy to ship broken — it was, and the
first render caught it.

**Three levels of emphasis, and they have to stay in that order**: a weekday in this month reads at
full strength, a weekend at `--gl-muted`, a day outside the month at `--gl-dim`. The first version
dimmed an out-of-month day twice — `--gl-faint` *and* `opacity: var(--gl-disabled)`, about 11% — and
put weekends below out-of-month days, so an in-month Saturday looked less present than a day
belonging to another month.

**On today, the dots drop their own colours.** A calendar's colour can land on top of itself —
a blue event on the blue accent fill is invisible, which the first render with sample events showed
immediately. There the dots take the cell's foreground instead, read from CSS at snapshot time so it
follows the theme rather than a copy of it. One day loses which-calendar information; the mockup made
the same trade for the same reason.

**Dots are drawn, not styled.** Up to three arbitrary `gdk::RGBA` per day cannot come from CSS
classes, so `Dots` is a small widget that snapshots rounded rectangles. Its `measure` reports the
same height whether or not the day has events, so a day gaining one does not resize the grid. Three
is a cap rather than a count: a fourth event adds no fourth dot.

**The arithmetic is not in the widget.** `grid.rs` turns a year and a month into 42 cells and steps
months across year boundaries — pure, tested headlessly, none of it needing a display.

**Scrolling is GTK's to accumulate, not ours.** `EventControllerScrollFlags::DISCRETE` emits only
whole-number deltas, so GTK has already turned a touchpad's fractional stream into steps. An
accumulator of our own sat on top of that for a while, carrying a remainder that was always zero.

**The Today button is `valign: center`.** Without it GTK stretches the button to the height of the
tallest control in the header row, and a pill that should hug its label becomes a slab.

**`select` compares before it writes, and that guard is load-bearing.** It emits `day-selected`, so
a handler that reacts by selecting — the obvious way to keep two views in step — drives the signal
round for ever without it. Removing the guard overflows the stack in the test suite rather than
failing an assertion. `clear_selection` is the other half; `selected` is an `Option` and nothing else
could return it to `None`.

**Weekdays are numbered as `glib::DateTime` numbers them**, Monday 1 through Sunday 7, everywhere:
`first-weekday`, `month_grid`'s argument, and the `weekday` helper.

**The current date is given, not read.** `set_today` exists so a test can stand on a month boundary
without waiting for midnight.

**The weekday letters come from January 2024**, whose 1st was a Monday, so day *n* of that month is
weekday *n* in `glib::DateTime`'s numbering. That is the whole trick: `%a` on those seven dates, cut
to two characters, gives the locale's own abbreviations without a table to translate — `Mo Tu We` in
English, `po wt śr` in Polish, and a single character where a locale abbreviates to one.

**`first-weekday` is a property and defaults to Monday.** Reading the locale's own first day needs
`nl_langinfo(_NL_TIME_FIRST_WEEKDAY)`, which no crate in the workspace exposes; until one does, the
caller sets it. The weekday *letters* are locale-correct already — they come from `%a`, truncated to
one character, so a non-Latin locale gets its own.

## Placeholder

What stands where content would be: off, empty, unavailable, busy. An icon, a heading, a
description, and an `error` flag.

**One widget for all four situations, and `error` only recolours the icon.** The approved states
matrix builds every column with the same `empty()` builder for exactly this reason — the shape a
user learns for "nothing here" is the one they read for "broken", so recognition costs nothing the
second time. A separate error widget would teach a second shape to say a neighbouring thing.

**The action is not in the block.** "Retry", "Scan again", "Network settings…" go in the shell's
footer. The block states the situation; the footer offers the way out. This is also what keeps the
block usable when there is no way out — a machine with no battery has nothing to offer.

**Why not `AdwStatusPage`.** It is the GNOME pattern for this and libadwaita documents `.compact`
for "a sidebar or a popover", so it was measured before being rejected. In a 400px popover:

| | height | title |
| --- | --- | --- |
| `AdwStatusPage` | 302px | 19.9px ultrabold |
| `AdwStatusPage.compact` | 226px | 19.9px ultrabold |
| this widget | 74px | `--gl-text-body`, 600, muted |

`.compact` is still an application-page empty state — three times the height, and it fills a panel
popover on its own. Adopting it means overriding its font size, weight, colour, icon size and every
padding, which is everything it provides, while inheriting a widget free to change them. The
measurement is the argument; without it this would just be preference.

**It wraps rather than widens.** Both labels wrap with a capped `max-width-chars`, for the reason
`Row` learned the hard way: an uncapped label's natural width is its whole string, so a long message
grows the popover instead of flowing.

**Not covered here:** an error that arrives *with* content — a stale weather reading, a degraded
service — where the list still renders and a strip above it says so. That is `AdwBanner`'s pattern
(GNOME HIG: persistent states, not events, "precise factual statements") and the mockups' `.status`
row. It is a second widget and is not built.

## Row

The list item every popover is made of: a Wi-Fi network, a Bluetooth device, a power profile, an
audio output.

```
[ check ] [ lead ] [ title    ]  ←space→  [ trail ]
                   [ subtitle ]
```

**It navigates, it does not expand.** A trailing chevron pushes a page; nothing in this widget
reveals content in place. Two reasons, and the second is the one that decides it: a popover's height
is capped by the work area, so expanding row 15 of 20 grows it past the fold and hides the thing
just revealed — and the content being revealed is not small. A Wi-Fi detail is four facts, two
settings and two buttons. That is a page. `var/design/row.md` records the evidence from the approved
mockups, which reached the same answer.

Expanding is right only when the revealed content is one or two rows *and* the list cannot grow —
an audio output revealing its volume slider. Build the expander when such a case appears.

**A slot ignores a widget it is already holding.** `fill_slot` compares against `first_child`
before it unparents anything, which matters because `EventList` and `WorldClock` re-apply every
slot on every render: without it, a list of ten rows unparents and reparents twenty widgets each
time a minute ticks.

**`icon-name` and `value` are properties; `lead` and `trail` stay slots.** Both are the same
argument that gave `Hero` its properties: without them a `.blp` can name the type and nothing else,
so every leading icon and every trailing fact has to be a hand-built child. Counted across the two
worked popover examples: 71 lead icons at four lines each and 45 value labels at twelve, which is
27% of 1975 lines of blueprint. They are separate widgets from the slots rather than fillings of
them, so `set_lead` and `set_icon_name` never fight over the same box, and a row can carry a value
*and* a chevron — which the network example needs and a single trailing slot cannot express.

**`lead` and `trail` take any widget** and the row never learns what it was given: a signal icon, a
lock, a spinner, the word `connecting`, `72%`, a chevron. One mechanism instead of a property per
kind of trailing thing. There is deliberately no second trailing slot for a value — no composition
in the approved mockups uses both, and two slots that mean "the right side" is the ambiguity `Hero`
already avoided.

**`selectable` and `selected` are separate**, which is the whole point of the check column. A
selectable row reserves 14px *before* anything is selected, so selecting one does not shift every
label in the list — the same rule as `ui.md`'s "data changes must not shift layout". A row that is
not selectable omits the column and starts at its lead. Both spellings appear in the approved
battery popover: Power mode reserves the column, Devices below it does not.

**A subtitle is what makes a row two lines.** Setting one adds `.row--two` and its taller metrics;
nothing else has to be told, and nothing can disagree.

**It is a `Gtk.Button`,** so activation, keyboard, focus and `:hover` / `:active` /
`:focus-visible` are GTK's rather than ours — `.row.hover` in the design mockups was a stand-in for
a state we now get for free. `activatable: false` drops `can-target` and `can-focus` together, so a
read-only fact row on a detail page neither lights up under the pointer nor stops the keyboard on
its way past. It is not made insensitive, which would dim it and say something untrue.

**Both labels cap their natural width** (`max-width-chars` 24 and 34, the numbers the approved
mockups used). `ellipsize` alone does not bound a row: it lowers the label's *minimum* width and
leaves the natural width at the full string, so an overlong SSID does not ellipsize — it widens the
popover around it. Measured: 447px natural for one 59-character title, 216px with the cap. The row
asserts this directly, by requiring that a 120-character title ask for no more width than a
40-character one.

**Sizes are rule-scoped tokens.** `--gl-row-height`, `--gl-row-padding` and `--gl-row-radius` are
declared in `.row` itself rather than in `:root`, because they are this rule's own measurements and
nothing else reads them. `:root` stays the shared vocabulary. `every_glimpse_token_the_stylesheet_reads_is_declared`
accepts both, since what it exists to catch is a token that is declared *nowhere* — GTK renders that
as a transparent surface and reports it nowhere.

**`.row` must reset `font-weight`.** libadwaita styles bare `button` with `font-weight: bold`, and
weight inherits into any label placed inside one. Left alone, every row renders bold — and since the
grammar distinguishes a selected row with `font-weight: 600`, every row would read as selected. A
`Gtk.Button` is not a neutral container: it arrives carrying padding, min-height, radius and weight
that a custom design has to undo on purpose.

## Section, EventList and WorldClock

The agenda under the calendar, split where the mockups split it.

```
 Section        Today                                    3
               ─────────────────────────────────────────────
 EventList      ●  Team standup                      09:30
                   Daily · Google Meet
                ●  Marta's birthday                      —
                   All day
                   4 more events                         ›
```

**`Section` is not event-specific, and that is deliberate.** `menu.py:45`'s `heading(text, state)`
renders **Today** in the calendar mockup and **World clock**, **Tray**, **Devices** and **Networks**
elsewhere. Naming the shell `EventListShell` would mean writing it again, slightly differently, the
next four times a popover needs a titled group — the drift `PopoverShell` exists to prevent.

**`empty` is set by the caller, not detected.** `Section` cannot ask an arbitrary content child
whether it has anything in it. An explicit flag also lets a caller show the placeholder while
content is merely *stale*, which is the syncing state the mockup renders: a placeholder saying
"Showing what was cached at 20:41" over a grid that still holds data.

**The count hides with the content.** A count of zero beside an empty state says the same thing
twice, so `set_empty(true)` hides it whatever `count` holds — hidden, not forgotten, so restoring
content restores it.

**Visibility toggle, not a `Gtk.Stack`.** A stack sizes to its largest page, so a placeholder would
reserve its height under a four-row agenda and the popover would never shrink. `Calendar` uses a
stack for month/year because there both pages *want* the same size.

**Event rows are `Row`.** `row2(summary, sub, icon, time=)` in the mockup is title + subtitle + lead
+ trailing label, down to the same 24/34 `max-width-chars` pair. An `EventRow` would fork the hover,
focus and activation of a widget that already has them.

**`when` arrives formatted; a `Zone` does not.** The two widgets take opposite kinds of input, and
the rule behind it is: *derive in the widget when formatting destroys the derivation.* A caller
handing `WorldClock` the string `"00:47"` has already thrown away the fact that it is tomorrow
there, and would have to recompute it to pass that too — at which point the widget is a
`Gtk.Label`. An event's start time carries no such hidden fact; whether it reads `09:30`, `—`,
`in 20 min` or spans midnight is applet policy read off config.

**The lead is a colour dot, not an icon.** The mockup repeats `appointment-soon-symbolic` on every
row, which spends the lead column saying "this is an event" ten times in a list of events. One dot
in the calendar's colour says *which* calendar, and matches the dots under that date in the month
grid. The column appears when *any* shown event carries a colour, so summaries still line up when
only some do. `Dots` moved out of `calendar/` for this and grew `set_max` and `set_size`: the
calendar reserves three 4px dots, an event draws one at three times that. Both are device pixels and
so do not follow text scaling — the one place in the crate that is true, because the dots are
snapshot-drawn rather than styled.

**Overflow belongs to the list.** Only `EventList` knows how many events it was handed against how
many it drew, so `"4 more events"` is computed once rather than at four call sites. It is a `Row`
with `.row--quiet`, it emits `overflow` rather than deciding what "open the rest" means, and
`max_rows == 0` means no cap.

**It navigates; it does not expand.** Lifting the cap in place fails twice, and both failures are
one click away: the list grows past the bottom of the screen, and there is no way back — the row
that would collapse it is the row that just disappeared. This is `Row`'s rule (`var/design/row.md`)
applied to a list rather than to one item, and the popover's height cap is what decides it. The
signal exists so the applet can push a page or open the calendar application; the preview has
nowhere to navigate to, which is why clicking it there does nothing, exactly as `Open calendar` in
the same footer does nothing.

**`EventList` defaults to inert.** A hover highlight is a promise that clicking does something, and
until a caller connects `activated` and says `set_activatable(true)`, an event row takes neither the
pointer nor the focus. **The overflow row is exempt**, because it is a control rather than an event:
it exists only because the caller capped the list, clicking it is the entire reason it is there, and
gating it on a flag that describes event rows made it inert in exactly the case that put it on
screen. `WorldClock` rows stay targetable, because taking the
pointer is what raises a tooltip — `Europe/Berlin · CEST (UTC+02:00)`, the zone the label actually
resolved to and the offset that makes the time checkable — but they paint no hover or active state
and are not tab stops. A tooltip is the whole of what the row offers, so the highlight and the focus
ring would both promise more than the click can keep. **`set_activatable(false)` cannot be used
here**: it drops `can-target`, and a widget that is not a pointer target never gets a tooltip
either.

**The lead is the zone's own icon, or day/night.** A `Zone` may carry `icon_name`; without one the
row falls back to `weather-clear-symbolic` when the local hour there is 07:00–19:00,
`weather-clear-night-symbolic` otherwise, and to nothing at all when the zone did not resolve. The
fallback answers the question the list is consulted for — *can I call them now* — which the digits
do not, and it is the only thing worth the lead column on a row whose label is a city. The hour
threshold is a hint, not astronomy: real sunrise and sunset need coordinates, and `glimpse-sunset`
already computes them, so this upgrades once a zone carries a location.

**`Zone::note` and `Zone::icon_name` are a second line and a glyph the widget knows nothing about.**
Weather is what they were added for — `18° · Light rain` under a showers icon — but the widget never
learns that: it takes a string and an icon name, and shares the second line with the day note,
`Tomorrow · 9° · Clear`, rather than taking a third. A third line is what stops a clock list being
glanceable. **The two travel together on purpose.** A zone that knows its weather draws it, because
a sun sitting above the words "light rain" is a contradiction the row states about itself — and it
is what the first version rendered.

The icon carries no colour of its own. Tinting daylight amber was decoration with no evidence behind
it, and it turns a daytime rain glyph into something that reads as a warning; the mockup's
`.row image { color: var(--muted) }` says the same thing. Shape carries the meaning, and GNOME's
weather icon set already has `-night` variants for a caller that wants to encode both.

**All rows are one height.** `.world-clock .row` and `.row--two` share a `min-height`, so a zone that
crosses midnight and gains `Tomorrow` does not grow and shove everything under it. For a clock that
is the strongest form of `ui.md`'s "data changes must not shift layout": the change happens while
the user is looking at it.

**A second line appears only when the date differs.** Same-day zones stay one line, so a list of
European cities has no subtitles and the block is four rows rather than eight. The comparison is
`(year, day_of_year)` against the instant the caller passed, *in that instant's own timezone* — so
pass a local `DateTime`, not a UTC one, or every row is compared against UTC's date.

**A zone that does not resolve reads `—`.** `g_time_zone_new_identifier` returns NULL for an
unknown identifier, which is why `glib`'s `v2_68` feature is enabled in the workspace: the older
`g_time_zone_new` silently returns UTC, and a clock that is confidently wrong is worse than one that
says it does not know. The tooltip still names the identifier, because that is the diagnostic.

**No timer.** `set_now` is the caller's tick. A popover that is shut still owning a source that
re-renders four labels a minute is exactly what `ui.md`'s widget-boundary rule exists to prevent,
and the applet has to own the tick anyway to know when the popover is visible.

Both right-hand columns take `--gl-muted`, not the mockup's two different greys. Within one popover
an event's time and a clock's time are the same kind of thing in the same column, and two weights of
grey read as a distinction that is not there.

**Times use `font-variant-numeric: tabular-nums`, and it is load-bearing.** Measured at 20px Adwaita
Sans: `11:11` / `20:41` / `09:30` request 39 / 51 / 56 px proportionally and 58 / 58 / 58 tabular. A
right-aligned time column without it moves by up to 17px depending on which digits the clock happens
to be showing — and unlike most jitter this one is animated. libadwaita ships a `.numeric` class
doing the same thing and applies it to `calendar` itself.

**`is_visible()` is not `get_visible()`.** The first is `gtk_widget_is_visible`, true only when every
ancestor is visible too; the second is the widget's own flag. Every getter that reports a value by
asking whether its label is showing must use `get_visible()`, or a `Row` inside a `Section` marked
empty reports `title() == None` for a title it is holding. `Section` hiding its content box is what
made this reachable; the same defect was already latent in `Row`, `Hero` and `Placeholder`.

**Neither list shares a base class with the other.** What they share is four lines of
clear-and-append; what differs is every slot. A `RowList` with two implementations is the ceremony
the finishing pass exists to cut.

The sync banner — the mockup's `status()` strip, "Last synced 4 h ago · Retry" *above* content that
is still shown — is not built. That is an error arriving **with** data, which `Placeholder`
deliberately does not cover, and it needs an action-signal design.

## Notice

An error, a warning or a fact that arrives **with** content that still works — which is exactly what
`Placeholder` refuses to be. Four designs asked for it before it existed: a weather alert, a weather
nowcast, a captive portal's "sign in required", and a pairing confirmation, plus the approved
mockup's `Last synced 4 h ago · Retry` strip.

**Severity is one state, not a set of flags.** `info` / `warning` / `error` as a `glib::Enum`, each
adding at most one class, so `.notice--warning` and `.notice--error` can never both be on. Two
booleans would have allowed a notice to be both.

**Not clickable by default.** A notice usually only states something, and a hover highlight is a
promise that clicking does something — `EventList`'s rule applied again. `activatable: true` takes
the pointer and the focus *and* reveals the chevron, so the affordance and the behaviour cannot
disagree.

**The nowcast and the alert are the same widget.** They differ in provenance — one is issued by a
weather service with an expiry and a colour code, the other is derived from radar and lives for
minutes — but not in shape. `severity` is the whole of the difference.

## Readout

The large number in a hero slot: a temperature, a battery percentage, a volume. `value` and `unit`
are separate labels so the unit can be smaller and dimmer than the figure, and so a value with no
unit reserves no width for one. Both sit on a shared baseline.

## RangeBar

A `(low, high)` segment drawn on a `(minimum, maximum)` track — a day's temperature against the
week's, and equally a battery range, a disk-usage span or a volume window.

**It is drawn, not styled**, because the geometry depends on data the stylesheet cannot see. It
takes its colour from CSS `color` and derives the track from that at 22% alpha, so a caller restyles
it the ordinary way; only the 4px thickness and the 24/96px width hints are literals, which is the
same limitation `Dots` carries and for the same reason.

**A high below its low is clamped, not drawn backwards**, and the clamp happens before the
compare-before-write guard — otherwise a clamped range never short-circuits, because the guard would
be comparing the requested value against the stored one.

Its `range()` and `scale()` getters exist so a snapshot-drawn widget has a test seam at all; there is
no other way to observe it without rendering pixels.

## FactList

`&[Fact]` — a label and a value — rendered as non-activatable `Row`s. It is the detail pane of every
popover: 45 hand-written fact rows existed across the network and bluetooth examples before it.

## ForecastStrip and ForecastList

`ForecastStrip` is the hourly columns, `ForecastList` the daily rows with a `RangeBar` in each trail.

**The list owns the scale.** `scale()` is the span of every day it holds, and `render` passes the
same pair to every bar, so two rows cannot be measured against different spans. A caller cannot get
this wrong because it is never asked.

**`ForecastHour` and `ForecastDay` are templates, and `ForecastStrip`/`ForecastList` are not.** A
template earns itself when the structure is static: an hour column is always time, icon,
temperature, and a day row is always precipitation, low, bar, high. How *many* of them there are is
data, so the two containers build their children at runtime with no template of their own — exactly
the `IndicatorGroup` builds `Indicator` shape.

**`ForecastDay` subclasses `Row`**, which needs three things that are easy to get wrong. `Row` must
be `IsSubclassable` (a `RowImpl` marker trait), the subclass's `[trail]` children route through
`Row`'s own `Buildable` — which works because the parent's template children are bound before the
subclass's are added — and `Row`'s property setters are **inherent methods on the wrapper, not a
trait**, so a subclass reaches them with `upcast_ref::<Row>()` rather than calling them directly.

**Row's lead icon is `lead-icon`, not `icon-name`.** `Gtk.Button` already owns an `icon-name`
property that replaces the button's child. A subclass calling `set_icon_name` therefore resolves the
*parent's* setter and destroys the row's template — which is exactly what happened the first time
`ForecastDay` was written. Shadowing a parent property with a different meaning is a trap that
recurs for every future subclass, so the property was renamed rather than documented around.
`Hero`, `Notice`, `Placeholder` and `ForecastHour` keep `icon-name`: they extend `Gtk.Widget`, which
owns no such property.

**There is no hour item and no day item widget beyond those two.** An hour column is three labels with no state and
no life outside the strip; a day row is already `Row`. A group/item split earns itself when the item
has state, signals, or standalone use — `Indicator` is separate from `IndicatorGroup` because the
panel places one on its own. Neither of these does.

**A zero chance of rain shows nothing**, and so does an unknown one: `Option<u32>` distinguishes
"no data" from "0%", and both render as an empty column rather than a `0%` that means neither.

**Temperatures are formatted here**, rounded with a `°` suffix, because `RangeBar` needs the numbers
and a caller passing strings would have thrown them away. There is deliberately no unit setter yet —
it belongs in the change that adds the °C/°F configuration, not before it.

**A runtime-built label handed to `set_text` must start `visible: false`.** `set_text` derives
visibility from the text and returns early when the text is unchanged, so a label created visible
with empty text is inconsistent from birth and never gets hidden. Blueprint children declare
`visible: false` and so are fine; every label built in Rust has to say the same thing.

## PopoverShell and Hero

`PopoverShell` is the frame every applet popover sits in: an optional hero, one content child, an
optional footer, and a `Gtk.Separator` between each pair. A section and its hairline are shown and
hidden together — hiding the section alone leaves a line floating against nothing, which is the one
mistake this widget exists to make impossible.

**The shell takes any widget as its hero.** It does not require a `Hero`, and does not know whether
it was given one. That is what lets an applet with something specific to show — a battery gauge, a
now-playing thumbnail — compose its own header without an escape hatch bolted onto the standard one.
`Hero` is the common case, not the required one.

`Hero` is `[ icon ] [ title / subtitle ] ←space→ [ slot ]`. The slot takes any widget, usually a
`Gtk.Switch`. There is deliberately no `set_toggle` convenience beside it: the previous generation
had both a `toggle: Option<bool>` field and a generic `trailing` slot, which is two ways to put a
switch on the right and no rule saying which.

The hero replaces the earlier design's split title row and lede. One header concept, not three.

Content is a single child and the footer is append/clear, and the asymmetry is deliberate: the shell
owns the footer's box, so it owns its orientation and spacing, while content's layout belongs to
whoever built it. A second `set_content` unparents the first.

**Both widgets are declarable.** `PopoverShell` and `Hero` implement `Gtk.Buildable`, so a popover
can be written in Blueprint rather than assembled in Rust:

```
$PopoverShell {
  [hero]
  $Hero { title: "Wi-Fi"; subtitle: "Tenda_4A21F0"; icon-name: "network-wireless-symbolic";
    [slot] Gtk.Switch { active: true; } }

  Gtk.Box { }        // no annotation: the content child

  [footer]
  Gtk.Button { label: "Network Settings"; }
}
```

`Hero`'s `title`, `subtitle` and `icon-name` are properties routed through the same setters Rust
calls, so the character cap applies to a value that arrives from a `.blp` exactly as it does to one
that arrives from a topic. `icon-name` is the declarative spelling of `set_icon`, not a second piece
of state — it reads back out of the same `gio::Icon`.

`add_child` ignores the widget's own template children, guarded by `try_get().is_none()`. This is
not defensive coding: `init_template` adds those children through `Gtk.Buildable` itself, so an
unguarded override routes `hero_box` into `content_box` and panics on an unbound `TemplateChild`
before the widget finishes being constructed.

**The shell paints its own surface** — `--gl-surface`, `--gl-surface-fg`, `--gl-radius`, all three
of which sat declared and unread until it did. It draws no shadow: inside a `Gtk.Popover` the
`contents` node already draws one, and a second is visible. Whatever ends up hosting the shell must
therefore either be transparent or agree with `--gl-radius`, because two rounded surfaces of
different radii stacked on each other show the mismatch at every corner.

**The shell does not scroll.** Capping height against the monitor belongs to whatever hosts it,
because only that knows the anchor's work area. Keeping it out is what lets the shell be built in a
test with no display behind it.

Title and subtitle are capped at `TEXT_MAX_CHARS` and set as plain text. A hero title carries
network SSIDs and MPRIS metadata, which are unbounded and come from other applications.

## Stylesheets

`Styles` owns the CSS providers for one process. `install()` registers them on the display **once**,
and `load()` replaces their content in place — installing twice stacks every rule. Three providers,
in cascade order:

| Priority | Source | Holds |
| --- | --- | --- |
| `APPLICATION` | `styles/glimpse.css`, via `include_str!` | the token vocabulary and every component rule |
| `USER` | the theme's sheet for this surface | token redefinitions |
| `USER + 1` | the user's own `styles.css` | the last word |

The built-in is compiled in rather than installed, because `load()` points the theme provider at
**one** path: selecting a theme named `nord` loads `nord/panel.css` *instead of* the default's, not
on top of it. A component rule living in a theme is a rule the first second theme deletes. It is
`include_str!` rather than a gresource because only `glimpse-panel` calls `register_resources()`,
and the lock screen and wallpaper need the same stylesheet.

The shipped `adwaita` theme is therefore **empty**, and that is the test: if the panel renders
correctly with three zero-byte sheets, the built-in stands alone.

Each provider connects `parsing-error`, because GTK4's loaders return nothing and a malformed
stylesheet is otherwise indistinguishable from a selector that does not match. Theme sheets load by
path rather than by string, so a relative `@import` resolves against the importing file's directory.
A sheet that does not resolve loads the empty string, which clears its provider.

**`parsing-error` does not see a bad token.** Measured on GTK 4.22: a `var(--gl-sruface)` naming
nothing, or an `alpha()` given a percentage, produces a `Gtk-WARNING` on stderr and never fires the
signal. The surface renders transparent and nothing in the log says why. Two things guard against
it — every `var()` in the built-in carries a fallback, and `theme::tests` lints the vocabulary.

`Styles` takes resolved paths rather than a theme name — locating them is `glimpse-config`'s job, and
keeping it that way is what lets this crate stay free of the configuration schema.

The built-in is compiled in, so editing `styles/glimpse.css` needs a rebuild. The hot loop for
working on a rule is `GLIMPSE_THEMES_DIR=data/themes`, which points the theme provider at the
repository's own pack and reloads it on every save; a rule written there overrides the built-in but
cannot delete one, so the result is transcribed back into `styles/glimpse.css` when it settles.

### The token vocabulary

Twenty-six tokens, all `--gl-` prefixed, declared once in the `:root` block of `styles/glimpse.css`.
Three tiers, and a rule may only read the tier directly below it: libadwaita's tokens → `--gl-*` →
component rules. A component rule that names `--accent-bg-color` or a literal colour is a test
failure, not a style choice.

| Group | Tokens |
| --- | --- |
| surfaces | `panel` `panel-fg` `surface` `surface-fg` `border` `shadow` |
| text ramp | `muted` `dim` `faint` |
| accent | `accent` `accent-fg` `accent-text` `accent-soft` |
| state | `hover` `active` `control` `knob` `scrim` |
| semantic | `danger-text` `danger-soft` `warning-text` |
| type | `text-caption` `text-body` `text-title` |
| other | `radius` `duration` `ease` `font-family` `disabled` |

## The type scale

Three sizes, all `rem`, and **no rule may write a font size in `px`** — `no_rule_sets_a_pixel_font_size`
fails the build on one.

| token | | role |
| --- | --- | --- |
| `--gl-text-caption` | 0.85rem | subtitles, badges, secondary facts |
| `--gl-text-body` | 1rem | row titles, panel labels — the default |
| `--gl-text-title` | 1.2rem | a hero's title, a section heading |

**Lengths follow the same rule.** Padding, margins, `min-width`, `min-height`, `border-radius` and
`-gtk-icon-size` are `rem`, so a box grows with the type it holds. `px` is kept for a hairline, a
border, an outline and a `999px` pill — things that are not proportional to text. Measured: a
two-line row specified in `px` reaches 83px at 200% text scaling against 148px for the same row in
`rem`. It does not clip, because a `min-height` is a minimum and GTK grows the box; what it loses is
the proportion, ending up as doubled type inside untouched 8px padding.

The base is the user's own font, because `rem` resolves against the root, and the root's font is
`gtk-font-name`. Nothing sets a base size; body text in a shell popover *is* the system UI size, and
choosing a different one second-guesses a preference the user already stated.

`px` was measured and rejected: at 200% text scaling GTK moves the root from 14.67px to 29.33px and
a `font-size: 14px` label does not move at all, so the shell would keep shell-sized text on a
desktop that had doubled. `em` scales correctly but **compounds** — 1.2em inside 1.5em is 1.8× — so a
size would depend on where the widget happened to sit. `rem` scales and does not compound.

A theme changes the scale by redefining the three tokens; there is no separate scale factor, because
the tokens already are one.

Eighteen derive from libadwaita, so the light/dark flip and the system accent cost nothing: setting
the scheme on `AdwStyleManager` moves every one of them at once, which is why there is no
`--dark-*` mirror and no `@media (prefers-color-scheme)` on a colour anywhere.

Three are literal. `--gl-knob` is white in both schemes by design, `--gl-scrim` sits over a wallpaper
rather than over an Adwaita surface, and `--gl-shadow` **cannot** be derived: `alpha()` multiplies
rather than replaces, and `--shade-color` is already translucent at 0.07, so `alpha(shade, 0.55)`
yields 0.04 and no visible shadow.

That same multiplication is why every token derived from `--gl-surface-fg` resolves lower in light
than in dark — Adwaita's light foreground carries 80%. `--gl-muted` is 0.44 light and 0.55 dark.
This matches what `.dimmed` does in every Adwaita application; **do not compensate for it.**

`--gl-control` reads `alpha(var(--gl-border), 1.5)` rather than a re-derived constant. Written as
`alpha(var(--gl-surface-fg), 0.15)` it rendered pixel-identical to `--gl-border`, because
`--border-color` is itself 0.12 / 0.15. The ratio form keeps the design's 1.5× separation between a
hairline and a switch track, and inherits libadwaita's high-contrast bump on `--border-color`.

### The panel and indicator rules

Ported from the previous generation's `themes/base.css` rather than invented, so the bar reads the
same: 6px of horizontal panel padding, a `0 1px 2px` shadow rather than a hard hairline, pill
indicators at `4px 6px` in a 22px box, semibold, `line-height: 1`, and a badge that is accent at 18%
carrying the panel's own foreground rather than a solid accent chip.

Four things were deliberately not carried across:

- **Thickness is `[[panels]] size`, not CSS.** The old sheet set `min-height` on the panel, and
  `Panel::set_thickness` calls `set_size_request`, which is also a *minimum* — GTK takes the larger
  of the two, so a stylesheet floor silently overrides any smaller configured size. Measured: with
  the rule present, `size = 28` still rendered 36px. Nothing in this sheet sets panel thickness.
- **The icon is not dimmed.** `--gl-muted` on `.indicator__icon` was a departure from the old bar,
  where an indicator's icon, label and badge all inherited the panel foreground at full strength.
- **`:active` keeps `--gl-active`.** The old sheet gave `:hover` and `:active` the same background
  and left `--indicator-active-bg` declared but unread, so a press looked exactly like a hover.
- **Font size is inherited.** The old bar's `11pt` was the system UI size restated; naming a size
  here would override font scaling instead, which `.claude/rules/ui.md` forbids. `tabular-nums` sits
  on `.panel` so a clock, a counter and a badge all get it from one declaration.

`.panel label` is gone with them: `font-family` and `font-weight` inherit, so it only restated
`.panel`.

Spacing, type sizes and inner radii are **not** tokens. They are literals in the rule that reads
them, because the design's rhythm is hand-tuned at 1px resolution — `3px`, `7px`, `9px` and `11px`
all appear in load-bearing places, and the button and row radii differ by exactly one pixel. A
spacing scale would not preserve that rhythm, it would replace it with a rounder one. `--gl-radius`
is the single exception: it rounds a surface, and nothing else moves.

## Rules

A widget moves here as soon as a second binary needs it. Preventing copy-paste between the panel and
the lock screen is the entire reason this crate exists.

Widgets take values and emit signals. They do not know about topics, sockets or the daemon, which is
what lets one be built in a test with a literal value and no daemon behind it.

Every widget assertion lives in one `#[ignore]`d test function. GTK binds to whichever thread calls
`gtk4::init()`, so a second test function constructing widgets on cargo's other test threads is a
race rather than a second test. `#[ignore]` is what keeps `just test` green without a display;
`just test-compositor` is the recipe that runs it. The test registers the gresource itself, because
a template resolves its resource at class-init and only the binaries get that from `main.rs`.
