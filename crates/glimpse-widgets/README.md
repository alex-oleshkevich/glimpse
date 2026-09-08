# glimpse-widgets

Shared GTK4 widgets: GObject subclasses, Blueprint templates and the CSS they expect. Used by the
panel and the lock screen.

## Layout

- `src/<widget>/` — one directory per widget, `mod.rs` plus `imp.rs`
- `blueprints/` — `.blp` templates, compiled by `build.rs` through `blueprint-compiler`
- `resources/widgets/` — the generated `.ui` files, bundled into `glimpse-widgets.gresource`

Adding a template means three edits: the `build.rs` pair, the `gresource.xml` entry, and the module
in `lib.rs`. The resource prefix is `/me/aresa/GlimpseShell`.

**Every type a template names must be bound as a `TemplateChild`, even one Rust never reads.**
Binding registers the GType before `init_template` resolves the class by name; without it `Builder`
reports `Invalid object type 'PopoverShell'` and the constructor panics. The crate's GTK test cannot
catch this — it builds every widget in one function, so earlier assertions have already registered
the types by the time a popover is constructed. It fails the moment a binary builds one alone.

`build.rs` only compiles; `just lint` runs `blueprint-compiler lint` separately. A child whose
accessible name lives on the composite widget takes `accessible-role: presentation` — giving it its
own label silences the same warning by making a screen reader announce the name twice.

## Recurring rules

These decide the same way in many widgets; the sections below assume them.

- **Every setter compares before it writes.** A caller can re-apply a whole spec every update
  without any of it reaching GTK. `gio::Icon` compares with `Icon::equal`.
- **A hover highlight is a promise that clicking does something.** A widget that only displays takes
  neither the pointer nor the focus.
- **`get_visible()`, not `is_visible()`.** The second walks ancestors, so a `Row` inside a `Section`
  marked empty reports `title() == None` for a title it holds.
- **`activatable: false` drops `can-target`, and a non-target passes the pointer to nothing** — not
  its children, and not the tooltip machinery. A row whose trail is a control stays activatable.
- **Both labels cap their natural width.** `ellipsize` lowers a label's *minimum* width and leaves
  its natural width at the full string, so an overlong SSID widens the popover rather than
  ellipsizing. Measured: 447px natural for a 59-character title, 216px capped.
- **Untrusted text is capped and set as plain text.** No markup setter anywhere. Tray titles, MPRIS
  metadata and SSIDs are unbounded and come from other applications.
- **A runtime-built label handed to `set_text` must start `visible: false`.** `set_text` derives
  visibility from the text and returns early when unchanged, so a visible empty label never hides.

## Indicator and IndicatorGroup

`Indicator` is one chip: an optional dot, icon, label and badge. It emits nothing — input belongs to
the group, because an applet is one clickable thing however many chips it renders. The chip takes
the `Generic` role and the accessible name moves up with the input.

The icon is one `Option<gio::Icon>`. `gdk::Texture` implements it, so a themed name, a file and a
StatusNotifierItem's ARGB pixmap all arrive through one setter. Sniffing a string for a leading
slash — the previous generation's approach — guesses wrong on a themed name containing one.

`IndicatorSpec` holds a `gio::Icon` and so is not `Send`: a value crossing from a tokio task stays
plain data and becomes an icon on the GTK thread.

`IndicatorGroup` is the interactive element — focusable, `Button` role, owning the click, scroll and
key controllers, emitting `pressed(button)` and `scrolled(dx, dy)`. Enter and Space emit `pressed`
with button 1, so keyboard activation arrives as an ordinary left click.

`set_items` reconciles **by position**: index *n* is applied to the widget at index *n*, extras are
created, the tail is unparented. An earlier version keyed on a spec id that existed only because
each chip carried a closure capturing it; with input owned by the group, no closure captures
anything. Placement is one `insert_after`, which both parents and reorders.

An empty group sets itself invisible — a visible empty widget still draws padding and counts toward
the enclosing spacing, which reads as a gap between its neighbours.

## Pager

A strip of one `PagerItem` per slot. It is the first indicator that is not an `IndicatorGroup`: a
group takes one click for the whole row over a fixed list, and a pager needs a click per slot over a
list whose length changes.

`Slot` carries `id`, `label`, `tooltip` and three independent states — `focus`, `occupied`,
`urgent`. `Focus` is `Here`/`Elsewhere`/`None`, replacing an `active`/`inactive` pair that both meant
"current workspace" and differed only in whether its output was focused. Occupancy and urgency stay
flags, so an urgent current workspace draws both. `Shape` is `Dots` or `Labels` and is the only thing
deciding label visibility.

**An item takes no click of its own.** It was a `Gtk.Button` per slot, but `GtkButton` restricts its
gesture to the primary button — measured through `observe_controllers` — which is the button every
applet's popover opens on. The rule that every applet opens on primary click has no exceptions, so
the per-slot action gave way; acting on one workspace happens in the popover. Dropping the button
also removed `:hover` rules that out-specified `--here` and `--urgent`.

The strip exposes `anchor()`, the item a press landed on, for the popover's arrow. Resolution is a
bounds test rather than `Widget::pick`: `pick` needs a *mapped* widget, a bounds test only an
allocated one, and that difference makes it assertable without a window. The reference is weak, so a
departing item stops being an anchor. A press between items anchors nothing.

Scrolling emits `stepped(horizontal, forward)`, not a raw delta; picking the dominant axis is
arithmetic and lives in a free `step(dx, dy)`. An equal diagonal resolves to vertical. **Vertical
steps whatever the strip is showing, horizontal steps the other dimension** — so a wheel always
moves along the slots in front of you, which is the only axis a plain mouse produces.

**One token drives both dimensions of the labels shape.** GTK4's `min-width`/`min-height` bound the
*content* box and padding is added outside it, so `min-width: 1.6rem` with `padding: 0 0.35rem`
measured 40 × 26 — an ellipse. The padding moved onto `.pager-item__label`, inside the content box.
The item's margin is symmetric because `measure()` includes it, and an asymmetric one makes the axes
incomparable.

**`PagerItem` deliberately has no `dispose`.** Its template's root child is a bound
`TemplateChild<Gtk.Label>` and `gtk4::Button` already unparents its own child; `dispose_template()`
on top unparents it twice. Measured, one critical per instance finalized:

| template root child | `ParentType` | `dispose_template()` | criticals |
| --- | --- | --- | --- |
| named (`Gtk.Label label`) | `Gtk.Button` | yes | one per instance |
| named | `Gtk.Button` | no | none |
| unnamed wrapper `Gtk.Box` | `Gtk.Button` | yes | none |
| named (`Hero`, `Readout`) | `gtk4::Widget` | yes | none |

*Naming* the root child triggers it, not the child's type. `Row` and `Notice` are also `Gtk.Button`
templates calling `dispose_template()` and are correct, because both wrap contents in an **unnamed**
box. A `gtk4::Widget` subclass owns no child and always needs the call.

**A slot's `tooltip` is its accessible name.** In the dots shape the label is hidden, and GTK derives
nothing from `tooltip-text` on its own. A slot with neither is an unnamed button.

**A vertical strip is a different shape, not a rotation.** `set_orientation` moves the layout, swaps
the alignment and toggles `pager--vertical`, all three load-bearing: left at `halign: Fill` every dot
stretches into a bar of its own, and the active item lengthens *along* the strip — `min-width`
horizontally, `min-height` vertically. Both were seen on a real `position = "left"` panel. It is
called from `constructed`, so the starting and running alignment come from one place.

**Hover must not outrank the state it sits on.** `.pager-item:hover` is more specific than
`.pager-item--here` and wins whatever the file order, so hovering the current workspace makes it
render as not-current and hovering an urgent one hides the urgency. Both hover rules exclude those
two states with `:not()`, which this GTK honours chained — measured `min-width` 90 against 10.
`--elsewhere` is deliberately not excluded: it already paints what hover would.

## Calendar

A month grid with a year view behind it. `var/design/calendar.md` holds the layout reasoning.

**The grid is always six weeks.** A month needs four to six, and a grid that tracked that would
resize the popover under the pointer scrolling it.

**Four measurements are tokens on `.calendar` itself** — `--gl-calendar-control`, `--gl-calendar-cell`,
`--gl-calendar-radius`, `--gl-calendar-gap`. Each appeared in two or three rules and they are what
anyone retuning the calendar reaches for first. The selection ring is deliberately *not* a token: it
is `2px` inside a `box-shadow`, and `only_hairlines_and_borders_are_measured_in_pixels` allows `px`
by property name — hoisting it moves that `2px` where the rule cannot recognise it as a border.

**The day cell is square**, 3.1rem, centred in its column rather than filling it: the grid is
homogeneous and each column is wider than a cell, so a stretched button renders a square as a
rectangle. `rem` throughout, so a cell grows with the text; a fixed cell holding scaling text
overflows at the first accessibility setting.

**Today is a fill, selected is an outline**, so a day that is both reads as both; today-and-selected
swaps the outline to `--gl-knob`. Drawing today as an outline leaves nothing distinct for selection.

**`Today` appears only off the current month and reserves no width.** The controls are anchored
right, so inserting it grows the group leftward and the arrows never move. `Row`'s check column needs
reservation because it is anchored left and precedes the label — opposite anchor, opposite answer.

**Month names use `%OB`, not `%B`.** `%B` is the form a date is built from — Polish `września` ("the
1st of September") against standalone `wrzesień`. English does not distinguish them, which is what
makes it easy to ship broken.

**Three levels of emphasis, in this order**: a weekday at full strength, a weekend at `--gl-muted`,
a day outside the month at `--gl-dim`. The first version dimmed out-of-month days twice and put
weekends below them, so an in-month Saturday looked less present than another month's day.

**On today, the dots drop their own colours** and take the cell's foreground, read from CSS at
snapshot time. A calendar's colour can land on top of itself — a blue event on the blue accent fill
is invisible. One day loses which-calendar information.

**Dots are drawn, not styled.** Up to three arbitrary `gdk::RGBA` per day cannot come from CSS
classes. `measure` reports the same height with or without events, so a day gaining one does not
resize the grid. Three is a cap, not a count.

**The arithmetic is not in the widget.** `grid.rs` turns a year and month into 42 cells and steps
across year boundaries — pure and tested headlessly.

**Scrolling is GTK's to accumulate.** `EventControllerScrollFlags::DISCRETE` emits whole-number
deltas only; an accumulator of our own carried a remainder that was always zero.

**The Today button is `valign: center`**, or GTK stretches it to the tallest control in the row.

**`select` compares before it writes, and that guard is load-bearing.** It emits `day-selected`, so
a handler that reacts by selecting drives the signal round for ever without it — removing the guard
overflows the stack rather than failing an assertion.

**Weekdays are numbered as `glib::DateTime` numbers them**, Monday 1 through Sunday 7, everywhere.
The weekday letters come from January 2024, whose 1st was a Monday, so day *n* is weekday *n*: `%a`
on those seven dates gives the locale's own abbreviations with no table to translate.

**`first-weekday` is a property defaulting to Monday.** Reading the locale's own first day needs
`nl_langinfo(_NL_TIME_FIRST_WEEKDAY)`, which no crate in the workspace exposes.

**The current date is given, not read** — `set_today` lets a test stand on a month boundary.

## Placeholder

What stands where content would be: off, empty, unavailable, busy. Icon, heading, description, and
an `error` flag that only recolours the icon — the shape a user learns for "nothing here" is the one
they read for "broken", so recognition costs nothing the second time.

**The action is not in the block.** "Retry", "Network settings…" go in the shell's footer. The block
states the situation; the footer offers the way out — which keeps the block usable when there is
none.

**Why not `AdwStatusPage`.** Measured in a 400px popover:

| | height | title |
| --- | --- | --- |
| `AdwStatusPage` | 302px | 19.9px ultrabold |
| `AdwStatusPage.compact` | 226px | 19.9px ultrabold |
| this widget | 74px | `--gl-text-body`, 600, muted |

`.compact` is still an application-page empty state and fills a panel popover on its own. Adopting
it means overriding its font size, weight, colour, icon size and every padding — everything it
provides — while inheriting a widget free to change them.

**It wraps rather than widens.** Both labels wrap with a capped `max-width-chars`.

**Not covered here:** an error arriving *with* content. That is `Notice`.

## Row

The list item every popover is made of.

```
[ check ] [ lead ] [ title    ]  ←space→  [ trail ]
                   [ subtitle ]
```

**It navigates, it does not expand.** A popover's height is capped by the work area, so expanding
row 15 of 20 grows it past the fold and hides the thing just revealed — and the content is not small
(a Wi-Fi detail is four facts, two settings, two buttons; that is a page). `var/design/row.md`
records the evidence. Expanding is right only when the revealed content is one or two rows *and* the
list cannot grow — an audio output revealing its volume slider.

**`icon-name` and `value` are properties; `lead` and `trail` stay slots.** Without properties a
`.blp` can name the type and nothing else. Counted across two worked popover examples: 71 lead icons
at four lines each and 45 value labels at twelve — 27% of 1975 blueprint lines. They are separate
widgets from the slots, so `set_lead` and `set_icon_name` never fight over one box and a row can
carry a value *and* a chevron.

**`lead` and `trail` take any widget** and the row never learns what it was given. There is
deliberately no second trailing slot: no approved composition uses both, and two slots meaning "the
right side" is the ambiguity `Hero` already avoids.

**A slot ignores a widget it already holds.** `fill_slot` compares against `first_child` before
unparenting, which matters because `EventList` and `WorldClock` re-apply every slot on every render.

**`selectable` and `selected` are separate.** A selectable row reserves 14px *before* anything is
selected, so selecting one does not shift every label in the list. A non-selectable row omits the
column. Both spellings appear in the approved battery popover.

**A subtitle is what makes a row two lines** — setting one adds `.row--two` and its metrics.

**It is a `Gtk.Button`**, so activation, keyboard, focus and the pointer states are GTK's.
`activatable: false` drops `can-target` and `can-focus` together; it is not made insensitive, which
would dim it and say something untrue.

**`.row` must reset `font-weight`.** libadwaita styles bare `button` bold and weight inherits into
any label inside one, so every row would render bold — and since the grammar distinguishes a
selected row with `font-weight: 600`, every row would read as selected. A `Gtk.Button` arrives
carrying padding, min-height, radius and weight a custom design has to undo on purpose.

**Sizes are rule-scoped tokens.** `--gl-row-height`, `--gl-row-padding`, `--gl-row-radius` are
declared in `.row` itself; `:root` stays the shared vocabulary. The lint accepts both, because what
it catches is a token declared *nowhere* — GTK renders that transparent and reports it nowhere.

## SplitRow

A `Row` and a trailing button divided by a hairline. The body is the primary action; the button is
the way in.

**One click cannot mean two things.** A display row that both turned the output off and opened its
detail page had three targets competing for one gesture. The hairline is the promise that the halves
differ.

**It wraps a `Row`, it does not subclass one.** Properties forward to the inner row and `[lead]` /
`[trail]` land inside it through `Gtk.Buildable`. Subclassing would put the button inside the row's
own box, where `Row` would have to know about it.

**Two signals, neither named `clicked`** — `activated` is the body, `details` is the button.

**There is no `activatable` property**; a body that should do nothing is a `FactList` row with a
chevron. The hairline is a `Gtk.Separator`, not a `border-left`, because the pixel lint allows
`border:` and a bare `1px` but not `border-left: 1px solid …`.

**Not covered by a test:** that a press on a trailing switch reaches the switch rather than the row.
Nested-button isolation is GTK's and needs a display and a synthetic pointer.

## Section, EventList and WorldClock

```
 Section        Today                                    3
               ─────────────────────────────────────────────
 EventList      ●  Team standup                      09:30
                   Daily · Google Meet
                   4 more events                         ›
```

**`Section` is not event-specific.** The same heading renders **Today**, **World clock**, **Tray**,
**Devices** and **Networks**. Naming it `EventListShell` would mean writing it again the next four
times.

**`empty` is set by the caller, not detected.** `Section` cannot ask an arbitrary content child
whether it holds anything, and an explicit flag also lets a caller show the placeholder while
content is merely *stale*. The count hides with the content — hidden, not forgotten.

**Visibility toggle, not a `Gtk.Stack`.** A stack sizes to its largest page, so a placeholder would
reserve its height under a four-row agenda. `Calendar` uses a stack for month/year because there
both pages *want* the same size.

**Event rows are `Row`.** An `EventRow` would fork the hover, focus and activation of a widget that
already has them.

**`when` arrives formatted; a `Zone` does not.** The rule: *derive in the widget when formatting
destroys the derivation.* A caller handing `WorldClock` the string `"00:47"` has thrown away the fact
that it is tomorrow there. An event's start time carries no such hidden fact.

**The lead is a colour dot, not an icon.** Repeating `appointment-soon-symbolic` spends the lead
column saying "this is an event" ten times in a list of events; one dot says *which* calendar and
matches the month grid. The column appears when *any* shown event carries a colour, so summaries
still line up when only some do. `Dots` grew `set_max`/`set_size` for this: the calendar reserves
three 4px dots, an event draws one at three times that. Both are device pixels — the one place in
the crate that is true, because the dots are snapshot-drawn.

**Overflow belongs to the list.** Only `EventList` knows how many events it was handed against how
many it drew. It emits `overflow` rather than deciding what "open the rest" means; `max_rows == 0`
means no cap. It navigates rather than expanding, for `Row`'s reason applied to a list.

**`EventList` defaults to inert**, but **the overflow row is exempt** — it is a control, clicking it
is the entire reason it is there, and gating it on the flag made it inert in exactly the case that
put it on screen. `WorldClock` rows stay targetable because taking the pointer is what raises a
tooltip, but paint no hover state and are not tab stops.

**`EventList` answers the tooltip, not the row.** A summary is capped at 120 characters by the
daemon and ellipsized again by width, so hovering should recover it — but a non-activatable row is
skipped by picking, and GTK finds tooltips by picking. The list takes `has-tooltip` itself and maps
the pointer's `y` onto row allocations, which also works when rows *are* activatable because the
lookup walks up from the picked widget. Text goes through `Tooltip::set_text`, never `set_markup`.

**The lead is the zone's own icon, or day/night.** Without an `icon_name` the row falls back to
`weather-clear-symbolic` for 07:00–19:00 local there, the night variant otherwise, and nothing when
the zone did not resolve. It answers the question the list is consulted for — *can I call them now*
— which the digits do not. The hour threshold is a hint; `glimpse-sunset` already computes real
sunrise, so this upgrades once a zone carries a location.

**`Zone::note` and `Zone::icon_name` travel together on purpose.** The widget never learns they are
weather; it takes a string and an icon name and shares the second line with the day note rather than
taking a third. A sun above the words "light rain" is a contradiction the row states about itself —
and is what the first version rendered. The icon carries no colour: tinting daylight amber turns a
daytime rain glyph into something that reads as a warning.

**All rows are one height** — `.world-clock .row` and `.row--two` share a `min-height`, so a zone
that crosses midnight and gains `Tomorrow` does not shove everything under it while the user watches.

**A second line appears only when the date differs.** The comparison is `(year, day_of_year)` against
the caller's instant *in that instant's own timezone* — pass a local `DateTime`, not a UTC one.

**A zone that does not resolve reads `—`.** `g_time_zone_new_identifier` returns NULL for an unknown
identifier, which is why glib's `v2_68` feature is enabled: the older `g_time_zone_new` silently
returns UTC, and a clock confidently wrong is worse than one saying it does not know.

**No timer.** `set_now` is the caller's tick.

**Times use `tabular-nums`, and it is load-bearing.** Measured at 20px Adwaita Sans: `11:11` /
`20:41` / `09:30` request 39 / 51 / 56 px proportionally and 58 / 58 / 58 tabular — up to 17px of
jitter, animated.

Both right-hand columns take `--gl-muted`, not two different greys: within one popover an event's
time and a clock's time are the same kind of thing in the same column.

**Neither list shares a base class.** What they share is four lines of clear-and-append; what differs
is every slot.

## Notice

An error, warning or fact arriving **with** content that still works — what `Placeholder` refuses to
be. Four designs asked for it first: a weather alert, a nowcast, a captive portal, a pairing
confirmation.

**Severity is one state, not a set of flags** — `info`/`warning`/`error` as a `glib::Enum`, each
adding at most one class, so two can never both be on.

**Not clickable by default.** `activatable: true` takes the pointer and focus *and* reveals the
chevron, so affordance and behaviour cannot disagree.

## Readout, RangeBar, FactList, ChoiceList

**`Readout`** — the large number in a hero slot. `value` and `unit` are separate labels sharing a
baseline, so the unit can be smaller and a value with no unit reserves no width.

**`RangeBar`** — a `(low, high)` segment on a `(minimum, maximum)` track. Drawn, not styled, because
the geometry depends on data the stylesheet cannot see; it takes CSS `color` and derives the track at
22% alpha. A high below its low is clamped, and **the clamp happens before the compare-before-write
guard** — otherwise a clamped range never short-circuits. `range()` and `scale()` exist so a
snapshot-drawn widget has a test seam at all.

**`FactList`** — `&[Fact]` as non-activatable `Row`s. 45 hand-written fact rows existed before it.

**`ChoiceList`** — `&[Choice]` plus one `selected` index; `FactList`'s read-only counterpart.
The check moves on click, before anyone handles it, and `set_selected` reconciles — re-asserting the
old value after a rejected click puts it back. Selection is positional, so `set_choices` drops the
index whenever the data differs (index 0 named the headphones a moment ago). An unchanged list
short-circuits first. Nothing is chosen until something says so; the first row is not a claim the
widget is entitled to make.

## ForecastStrip and ForecastList

`ForecastStrip` is hourly columns, `ForecastList` daily rows with a `RangeBar` in each trail.

**The list owns the scale.** `scale()` is the span of every day it holds and `render` passes the same
pair to every bar, so two rows cannot be measured against different spans.

**The items are templates; the containers are not.** A template earns itself when the structure is
static. How many there are is data, so the containers build children at runtime — the same shape as
`IndicatorGroup` building `Indicator`.

**`ForecastDay` subclasses `Row`**, which needs three things: `Row` must be `IsSubclassable` (a
`RowImpl` marker), the subclass's `[trail]` children route through `Row`'s own `Buildable` (the
parent's template children are bound before the subclass's are added), and `Row`'s property setters
are **inherent methods on the wrapper, not a trait**, so a subclass reaches them via
`upcast_ref::<Row>()`.

**Row's lead icon is `lead-icon`, not `icon-name`.** `Gtk.Button` already owns an `icon-name` that
replaces the button's child, so a subclass calling `set_icon_name` resolves the *parent's* setter and
destroys the row's template — which is what happened the first time `ForecastDay` was written.
`Hero`, `Notice`, `Placeholder` and `ForecastHour` keep `icon-name`; they extend `Gtk.Widget`, which
owns no such property.

**A zero chance of rain shows nothing, and so does an unknown one.** `Option<u32>` distinguishes them
and both render empty rather than a `0%` meaning neither.

**Temperatures are formatted here** because `RangeBar` needs the numbers and a caller passing strings
would have thrown them away. No unit setter yet — it belongs in the change that adds °C/°F.

## NowPlaying, Scrubber, Transport and PlayerList

`NowPlaying` is one player in full — artwork, application, title, artist, album, a `Scrubber` and a
`Transport`. It is built to be a `PopoverShell` **hero**, which is why `set_hero` takes any widget.

**It exposes the two children rather than proxying them.** `scrubber()` and `transport()` return the
real widgets, so `NowPlaying` carries five properties instead of fifteen.

### Scrubber

A `Gtk.Scale`, not a drawn bar: it must be draggable and keyboard-reachable, and `Gtk.Range` brings
the drag, arrow keys, focus ring and accessible role.

**`set_position` is ignored while the pointer is down**, or a player reporting once a second yanks
the slider out from under a drag. The hold uses a `Gtk.EventControllerLegacy` in the capture phase,
because a `Gtk.GestureClick` there is *cancelled* the moment `Gtk.Range` claims the sequence — it
would report the press and never the release.

**A drag emits one `seek`, at the end.** `change-value` fires continuously and one D-Bus call per
motion event is not a design. A keyboard or scroll change is not a drag and emits immediately.

**`unmap` clears the hold** — a press whose release never arrives would freeze the widget, and a
popover closing mid-drag is how a popover closes.

**The range belongs to the adjustment.** `set_position` does not clamp; a duplicate clamp survived
every mutation aimed at it, which is what proved it dead.

**The step and page increments are set in Rust**, because `blueprint-compiler lint` rejects a
`Gtk.Adjustment` carrying anything besides `lower`, `upper` and `value` — measured, independent of
order. They are the arrow-key and Page-Up distances, so losing them silently kills keyboard seeking.

**Zero duration means a live stream** and the track disappears rather than sitting at either end
claiming something untrue. The elapsed figure stays.

The GTK test reaches `held` directly; the three things that *set* it need a synthetic press and are
verified by hand in the preview.

### Transport

Five buttons and one `action` signal carrying which was pressed. It holds no state it is not given:
pressing shuffle emits, it does not toggle.

**Previous, next and play dim; shuffle and repeat hide.** A missing capability is still one of the
three buttons under the pointer and removing it would move the other two between tracks. A player
without shuffle has no state for it to show, and a permanently dead icon is worse than none.

**Repeat is a three-state enum**, matching MPRIS `LoopStatus`; repeat-one is a different icon.

**The capability setters carry no compare-before-write guard.** They forward to `set_sensitive` and
`set_visible`, which already return early — a guard on top would be a second copy of GTK's. The rule
holds wherever the setter does more than one thing: `set_playing` and `set_repeat` write an icon
*and* a class, so both guard.

### PlayerList, PlayerRow and artwork

`&[Player]` as `PlayerRow`s — a `Row` subclass whose `[trail]` is a play/pause button. Clicking the
row emits `activated`, the button emits `toggled`, both carrying the index. The second line is
composed here: `Player` keeps `artist` and `name` apart because a caller has them apart, so a video
with no artist reads `VLC` rather than ` · VLC`. No artwork on a row — a thumbnail per row is a
decode per row for what the eye reads as the application icon anyway.

**Artwork is a `Gtk.Image`, because it is the only one that can be told how big to be.** Measured
with a 192px texture: `Gtk.Picture` reports a natural width of 192 — the paintable's own size, and
`can-shrink` only drops the *minimum* to zero — so a cover would set the popover's width.
`Gtk.Image` reports its icon size and takes it from CSS, which keeps the square in `rem`.
`overflow: hidden` plus `border-radius` rounds it; GTK clips a widget's own content there.

Two CSS rules do two jobs — `min-width`/`min-height` set the box, `-gtk-icon-size` caps what goes in
it — carrying the same number because a cover should fill its square. Removing either alone changes
nothing, so the test catches only their removal together and claims no more.

**The empty state is a class, not a second widget**, so the two states cannot drift apart the way
they did when the placeholder's size was reconstructed from padding plus a smaller icon.

**A widget built before `Styles::install()` never picks any of it up.** Rooting is not what matters —
an unparented widget is styled fine, measured — the *order* is.

## PopoverShell and Hero

`PopoverShell` is the frame every applet popover sits in: an optional hero, one content child, an
optional footer, and a `Gtk.Separator` between each pair. **A section and its hairline show and hide
together** — hiding the section alone leaves a line floating against nothing, which is the one
mistake this widget exists to prevent. It watches `notify::visible` on what is appended, so a slot
follows its children rather than the fact that something was appended once.

**The shell takes any widget as its hero** and does not know whether it got a `Hero`. That is what
lets an applet with something specific to show compose its own header.

`Hero` is `[ icon ] [ title / subtitle ] ←space→ [ slot ]`. There is deliberately no `set_toggle`
beside the slot: the previous generation had both a `toggle: Option<bool>` and a generic trailing
slot, which is two ways to put a switch on the right and no rule saying which.

Content is a single child and the footer is append/clear. The asymmetry is deliberate — the shell
owns the footer's box and so its orientation and spacing, while content's layout belongs to whoever
built it.

**Both widgets are declarable**, implementing `Gtk.Buildable`:

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

`Hero`'s properties route through the same capped setters Rust calls. `icon-name` is the declarative
spelling of `set_icon`, not a second piece of state.

**`add_child` must ignore the widget's own template children**, guarded by `try_get().is_none()`.
`init_template` adds them through `Gtk.Buildable` itself, so an unguarded override routes `hero_box`
into `content_box` and panics on an unbound `TemplateChild` before the widget exists.

**The shell paints its own surface** and draws no shadow: inside a `Gtk.Popover` the `contents` node
already draws one. Whatever hosts it must be transparent or agree with `--gl-radius`, because two
rounded surfaces of different radii show the mismatch at every corner.

**The shell does not scroll.** Capping height against the monitor belongs to whatever hosts it, which
is the only thing that knows the anchor's work area — and keeping it out is what lets the shell be
built in a test with no display.

## WorkspaceList and WorkspacesPopover

`WorkspacesPopover` is the pager's popover: `PopoverShell` + `Hero` + `WorkspaceList` + a
`Gtk.Revealer` drawer. The list groups workspaces into one `Section` per output and renders a
`SplitRow` each — the body activates, the chevron opens the drawer on that workspace's windows. Both
emit ids, because a row is rebuilt whenever the list changes and a closure capturing a widget would
outlive it.

**One `Workspace` carries more than the list renders.** A window's title changes on every keystroke,
so comparing the whole struct rebuilt every `SplitRow` several times a second — destroying the row
under the pointer. `same_rows` compares only what the list draws, so window traffic reaches the
drawer and stops at the list. The popover's own guard still compares everything, which is what lets
an open drawer follow the session.

**The drawer opens to the side, never downwards** — a `Gtk.Revealer` with `slide_right` as the second
child of a horizontal box. The row that opened it stays on screen and stays the row that closes it.
`var/design/popover_drawer.md` records the rejected alternatives: expand-in-place walks the list off
the bottom of the screen, and push-a-page removes the list you were comparing against.

Two lengths make that layout work, both found by getting them wrong:

- **The list carries `hexpand: true`.** A horizontal content box does not stretch its children the
  way a vertical one does, so the slack falls out at the right edge as a stray padding.
- **`.column` carries the width floor, not `.popover-shell`.** `Row` ellipsizes, dropping a label's
  minimum toward zero, so an unfloored column compresses and the drawer takes its width *out of* the
  list instead of growing the popover.

**A drawer row reports the window standing at its position**, read from the open workspace when the
click arrives. Rows are reused across workspaces, so a closure capturing the id would focus the wrong
window from the second drawer onward.

**An open drawer is re-revealed on every update**, because the workspace it shows has just been
replaced under it; one whose workspace disappeared closes.

**Neither counts anything.** A workspace already carries its window count as the row's value;
repeating it in the section header and the drawer header means three numbers for one fact.

**`.workspace-row--urgent` is set on the `SplitRow`, not the `Row`**, so the chevron carries the
state too. A popover that lists the same workspaces without urgency contradicts the bar above it.

## `CalendarPopover`

The clock applet's popover, assembled from `PopoverShell`, `Hero`, `Calendar`, `Section`,
`EventList`, `WorldClock`, `Placeholder` and `Row`. It owns the structure and none of the content:
every string is handed to it already formatted, because what an event's time *says* depends on `now`,
which is the applet's problem.

**The drawer is an offer, and the offer toggles.** `EventList` emits `overflow` when the viewer
clicks the "N more events" row, not when overflow merely exists. Clicking again closes it — nothing
here calls `set_reveal_child` directly; both popovers go through `drawer::toggle` / `drawer::set`.
`set_day` closes it when a shorter list no longer overflows.

**Both placeholder wordings live in the template.** The day's placeholder slot holds a `Gtk.Stack`
with a `nothing` page and a `truncated` page, each a `$Placeholder` with its own `_()` strings, and
`set_day_truncated` only switches between them. Writing the wording from Rust was the bug: nothing in
this tree translates a Rust string.

**`month-shown` fires from `render`, once per month actually shown.** Every navigation path funnels
through `Calendar::render`, so the signal is emitted there against an `announced` cell. A day picked
inside the month already shown emits nothing, which keeps the panel from re-asking the daemon for a
range it has.

**No counts.** `Section` can show one and the design sketch does, but a number nobody asked for is
noise that has to be kept correct as well as read. `set_day` names both sections after the same day —
the drawer holds that one day's complete list, so "Everything" said something the widget does not do.

The error placeholder the design draws — "Cannot reach the calendar service" — arrives with the
service that can fail.

## `NextEventPopover`

The next-event applet's popover: a `Hero` naming the entry the bar is showing, a `Readout` in its
slot counting down, one `Section` listing what follows, and the footer. No calendar and no drawer —
the applet answers one question, and a second surface would be the clock's popover with fewer
features.

**The empty wording is the template's, captured at `constructed` and restored by `set_nothing`.** The
hero's title is normally the event's summary, so the quiet wording cannot be a static property — but
writing it from Rust puts a user-facing string where no translator looks. The alternative was a
`Gtk.Stack` of two `$Hero`s, as `CalendarPopover` does for its placeholders; capture-and-restore
costs a `RefCell` and an ordering assumption, the stack costs a second copy of the hero's structure.
The ordering assumption is pinned by a GTK-test assertion, so a future GTK applying template
properties after `constructed` fails the test rather than blanking the wording silently.

**`set_nothing` does not touch the list.** The hero answers `within` and the list answers `horizon`,
which reaches further — emptying the list here wiped entries the horizon still held whenever an
event ended with the popover open.

**The countdown is one value, so `set_countdown` takes one argument.** A signature admitting
`(Some, None)` invites a caller to key visibility off whichever half it checked.

**`set_footer` is `crate::set_footer_row` in both popovers.** It sits in `lib.rs` beside `set_text`
rather than on `PopoverShell`, whose footer is a slot taking any widget and should not be narrowed to
a `Row` as a side effect — bead `glimpse-34sw`.

**The `column` class sits on the `Section`, not a box around it.** There is one child, and
`blueprint-compiler lint` reports `use_adw_bin` for a `Gtk.Box` holding one widget. `.column` is a
descendant rule, so it applies to any widget carrying the class. A second section brings the box back.

## Translations

A literal in a `.blp` that a person reads is marked `_("Text")`. GTK resolves it inside the
template at class-init, against the process default domain that `glimpse-utils::init_translations`
sets — so nothing here calls into gettext for a blueprint string, and nothing here needs to.

Text this crate *computes* is a different matter and does call `gettext` directly:
`set_play_pause`'s tooltip, `WorldClock`'s Tomorrow/Yesterday note, and `WorkspacesPopover`'s
summary. The summary uses `ngettext` because the count decides the wording, and Russian picks a
different form at 1, 3 and 7 where English changes once.

`day_note` returns `Option<String>` rather than `Option<&'static str>` for exactly this reason: a
translated string is owned, and there is no `'static` catalog to borrow from.

The preview host binds the domain too, so `LANGUAGE=ru GLIMPSE_LOCALE_DIR=$PWD/target/locale just
preview <blueprint.blp>` renders a widget in another language. A widget whose text grows by a third
in translation is worth seeing before it reaches a panel, the same argument that makes
`--scheme dark` worth a flag.

## Stylesheets

`Styles` owns the CSS providers for one process. `install()` registers them on the display **once**
and `load()` replaces their content in place — installing twice stacks every rule.

| Priority | Source | Holds |
| --- | --- | --- |
| `APPLICATION` | `styles/glimpse.css`, via `include_str!` | the token vocabulary and every component rule |
| `USER` | the theme's sheet for this surface | token redefinitions |
| `USER + 1` | the user's own `styles.css` | the last word |

The built-in is compiled in rather than installed, because `load()` points the theme provider at
**one** path: selecting `nord` loads `nord/panel.css` *instead of* the default's. A component rule
living in a theme is a rule the first second theme deletes. It is `include_str!` rather than a
gresource because only `glimpse-panel` calls `register_resources()`, and the lock screen and
wallpaper need the same sheet. The shipped `adwaita` theme is therefore **empty**, and that is the
test.

Each provider connects `parsing-error`, because GTK4's loaders return nothing. Theme sheets load by
path so a relative `@import` resolves against the importing file's directory; one that does not
resolve loads the empty string.

**`parsing-error` does not see a bad token.** Measured on GTK 4.22: a `var()` naming nothing, or an
`alpha()` given a percentage, produces a `Gtk-WARNING` on stderr and never fires the signal — the
surface renders transparent and nothing says why. Two guards: every `var()` in the built-in carries a
fallback, and `theme::tests` lints the vocabulary.

`Styles` takes resolved paths rather than a theme name, which is what keeps this crate free of the
configuration schema.

Editing `styles/glimpse.css` needs a rebuild. The hot loop is `GLIMPSE_THEMES_DIR=data/themes`, which
reloads on every save; a rule written there overrides the built-in but cannot delete one, so the
result is transcribed back when it settles.

### The token vocabulary

Thirty-one tokens, all `--gl-` prefixed, declared once in `:root`. Three tiers, and a rule may only
read the tier below it: libadwaita's tokens → `--gl-*` → component rules. A component rule naming
`--accent-bg-color` or a literal colour is a test failure.

| Group | Tokens |
| --- | --- |
| surfaces | `panel` `panel-fg` `surface` `surface-fg` `border` `shadow` |
| elevation | `elevation-raised` `elevation-floating` |
| text ramp | `muted` `dim` `faint` |
| accent | `accent` `accent-fg` `accent-text` `accent-soft` |
| state | `hover` `active` `control` `knob` `scrim` |
| semantic | `danger-text` `danger-soft` `warning-text` |
| type | `text-caption` `text-body` `text-title` |
| other | `radius` `duration` `ease` `font-family` `disabled` |

Eighteen derive from libadwaita, so the light/dark flip and the system accent cost nothing — which is
why there is no `--dark-*` mirror and no `@media (prefers-color-scheme)` on a colour anywhere.

Three are literal. `--gl-knob` is white in both schemes by design, `--gl-scrim` sits over a wallpaper
rather than an Adwaita surface, and `--gl-shadow` **cannot** be derived: `alpha()` multiplies rather
than replaces, and `--shade-color` is already 0.07, so `alpha(shade, 0.55)` yields 0.04 and no
visible shadow.

That same multiplication is why every token derived from `--gl-surface-fg` resolves lower in light
than dark — Adwaita's light foreground carries 80%, so `--gl-muted` is 0.44 light and 0.55 dark. This
matches `.dimmed` in every Adwaita application; **do not compensate for it.**

`--gl-control` reads `alpha(var(--gl-border), 1.5)` rather than a re-derived constant: written as
`alpha(var(--gl-surface-fg), 0.15)` it rendered pixel-identical to `--gl-border`. The ratio form
keeps the design's 1.5× separation and inherits libadwaita's high-contrast bump.

### Elevation is a closed set of two

`--gl-elevation-raised` is a surface lying on the desktop, `--gl-elevation-floating` one detached
from it. `every_drop_shadow_reads_an_elevation_token` fails the build on a rule that writes its own:
a component `box-shadow` may be `inset` (a ring, not a shadow) or `var(--gl-elevation-*)`, nothing
else. A third elevation arrives with the change that needs it.

**This deliberately moves `px` out of the linted half of the sheet.**
`only_hairlines_and_borders_are_measured_in_pixels` scans only what follows `:root`, so a `1px` blur
inside a token declaration is never seen. The trade: two offsets stop being checked individually, and
the elevation test proves every rule reads one.

**`var()` does expand into `box-shadow`**, measured rather than assumed — `rem` lengths in a
`box-shadow` parse cleanly and paint nothing at all, with no `parsing-error` and no log line. Two
identical boxes, one literal and one `var(--gl-elevation-floating)`, differed by at most 3/255.

## The type scale

Three sizes, all `rem`. **No rule may write a font size in `px`** —
`no_rule_sets_a_pixel_font_size` fails the build.

| token | | role |
| --- | --- | --- |
| `--gl-text-caption` | 0.85rem | subtitles, badges, secondary facts |
| `--gl-text-body` | 1rem | row titles, panel labels — the default |
| `--gl-text-title` | 1.2rem | a hero's title, a section heading |

**Lengths follow the same rule.** Padding, margins, `min-width`, `min-height`, `border-radius` and
`-gtk-icon-size` are `rem`. `px` is kept for a hairline, a border, an outline and a `999px` pill.
Measured: a two-line row in `px` reaches 83px at 200% text scaling against 148px in `rem` — it does
not clip, it loses the proportion, ending as doubled type inside untouched 8px padding.

`px` was measured and rejected: at 200% scaling GTK moves the root from 14.67px to 29.33px and a
`font-size: 14px` label does not move at all. `em` scales but **compounds** — 1.2em inside 1.5em is
1.8× — so a size would depend on where the widget sat. Nothing sets a base size; body text in a shell
popover *is* the system UI size.

A theme changes the scale by redefining the three tokens; there is no separate scale factor.

### The panel and indicator rules

Ported from the previous generation's `themes/base.css` so the bar reads the same: 6px horizontal
panel padding, a `0 1px 2px` shadow rather than a hairline, pill indicators at `4px 6px` in a 22px
box, semibold, `line-height: 1`, and a badge at 18% accent carrying the panel's foreground.

Four things were deliberately not carried across:

- **Thickness is `[[panels]] size`, not CSS.** The old sheet set `min-height`, and
  `Panel::set_thickness` calls `set_size_request` — also a minimum, so GTK takes the larger and a
  stylesheet floor silently overrides a smaller configured size. Measured: `size = 28` rendered 36px.
- **The icon is not dimmed.** `--gl-muted` on `.indicator__icon` departed from the old bar, where
  icon, label and badge all inherited the panel foreground at full strength.
- **`:active` keeps `--gl-active`.** The old sheet gave `:hover` and `:active` the same background,
  so a press looked exactly like a hover.
- **Font size is inherited.** The old bar's `11pt` restated the system UI size; naming one here
  overrides font scaling, which `.claude/rules/ui.md` forbids. `tabular-nums` sits on `.panel`.

Spacing, type sizes and inner radii are **not** tokens. The design's rhythm is hand-tuned at 1px
resolution — `3px`, `7px`, `9px` and `11px` all appear in load-bearing places, and the button and row
radii differ by exactly one pixel. `--gl-radius` is the exception: it rounds a surface, and nothing
else moves.

## Rules

A widget moves here as soon as a second binary needs it. Preventing copy-paste between the panel and
the lock screen is the entire reason this crate exists.

Widgets take values and emit signals. They do not know about topics, sockets or the daemon, which is
what lets one be built in a test with a literal value and nothing behind it.

Every widget assertion lives in one `#[ignore]`d test function. GTK binds to whichever thread calls
`gtk4::init()`, so a second test function constructing widgets on cargo's other test threads is a
race rather than a second test. `just test-compositor` runs it. The test registers the gresource
itself, because a template resolves its resource at class-init and only the binaries get that from
`main.rs`.
