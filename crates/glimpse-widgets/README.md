# glimpse-widgets

Shared GTK4 widgets: GObject subclasses, Blueprint templates and the CSS they expect. Used by the
panel, the notification popup process and the lock screen.

## Layout

- `src/<widget>/` — one directory per widget, `mod.rs` plus `imp.rs`
- `blueprints/` — `.blp` templates, compiled by `build.rs` through `blueprint-compiler`
- `resources/widgets/` — generated `.ui`, bundled into `glimpse-widgets.gresource`

Adding a template is three edits: the `build.rs` pair, the `gresource.xml` entry, the module in
`lib.rs`. Resource prefix `/me/aresa/GlimpseShell`; `build.rs` only compiles, and `just lint` runs
`blueprint-compiler lint` separately.

**Every type a template names must be bound as a `TemplateChild`, even one Rust never reads.**
Binding registers the GType before `init_template` resolves the class by name, or `Builder` reports
`Invalid object type` and the constructor panics.

## Recurring rules

- **Every setter compares before it writes**; `gio::Icon` compares with `Icon::equal`.
- **`css_classes` on a builder replaces the list, and `has-frame: false` *is* the `flat` class**, so
  a chain setting both paints a button behind a frameless icon. Add classes after `build()`.
- **`get_visible()`, not `is_visible()`** — the second walks ancestors, so a `Row` inside a `Section`
  marked empty reports no title for one it holds. A runtime-built label handed to `set_text` starts
  `visible: false`, since `set_text` returns early when unchanged: a visible empty label never hides.
- **A row highlights only if its body acts.** `activatable: false` drops `can-target`, so neither the
  row, its children, nor a tooltip is reachable — a row holding a control uses `SwitchRow` instead.
- **Both labels cap their natural width**, because `ellipsize` lowers only a label's *minimum*.
- **Untrusted text is capped and set as plain text**, with no markup setter anywhere: tray titles,
  MPRIS metadata, SSIDs and device names come from other applications and are unbounded.

## Indicator, IndicatorGroup and Pager

The icon is one `Option<gio::Icon>`; `gdk::Texture` implements it, so a themed name, a file and a
tray pixmap all arrive through one setter — sniffing a string for a leading slash guesses wrong on a
themed name containing one. `IndicatorSpec` holds a `gio::Icon` and so is not `Send`.

- **The icon sits in a `Gtk.Overlay`, and `overlay` is an emblem on its trailing corner** — the
  Windows-taskbar idiom, for a state the application's own icon does not carry. The *slot* follows
  the base icon's presence, so an indicator with neither reserves no space.
- **`extension` is any widget the applet owns**, placed in a box beside the icon slot. The
  indicator compares by identity and only reparents on a change, so an applet keeps one widget —
  the color picker's `Swatch` — and updates it in place.
- **A badge hides the attention dot, and must not cancel attention itself.** Two marks for one fact
  is noise, so the dot yields while `indicator--attention` stays and colours the chip. Assert it from
  a clean spec — `set_attention` returns early on an unchanged flag.

`Pager` is a strip of `PagerItem`, the one indicator that is not an `IndicatorGroup`: a click per
slot over a list whose length changes.

- **An item takes no click of its own.** `GtkButton` restricts its gesture to the primary button,
  the one every applet's popover opens on; acting on a workspace happens in the popover.
- **One token drives both dimensions of the labels shape.** GTK4's `min-width`/`min-height` bound
  the *content* box and padding is added outside it, so padding belongs on `.pager-item__label`.
- **`PagerItem` deliberately has no `dispose`.** *Naming* a `Gtk.Button` template's root child makes
  `dispose_template()` unparent it twice; `Row` and `Notice` escape by wrapping contents in an
  **unnamed** box, and a `gtk4::Widget` subclass owns no child and needs the call.

`TrayStrip` is the second. It renders one `Indicator` per `TrayChip` and exists because a tray item
is its own remote object, which an `IndicatorGroup` cannot express — that group is one clickable
thing whose `pressed` names no chip.

- **Each chip owns its click and scroll controllers, and the key is captured when it is built.**
  `reconcile::by_key` binds a widget to a key for as long as the key lives, so the capture stays
  correct across a reorder with no hit-testing arithmetic — the opposite of `IndicatorGroup`.
- **Visible chips and overflow chips are two boxes**, because `by_key` assumes the items are its
  parent's only children and the strip also holds the chevron and the revealer.
- **The chevron is handled inside the strip and emits nothing** — overflow is presentation, so an
  applet never learns it happened. It hides when nothing is hidden, and closes the drawer on the way.
- **The chevron is pinned to one edge and the hidden chips grow away from it**, so the overflow
  opens into the bar rather than off the screen. A right-zone applet wants `Edge::Start`, a
  left-zone one `Edge::End`, and the slide direction *and* the chevron's `pan-*-symbolic` follow that
  pair — **as must the strip's `halign`**, or the strip grows from its anchor and shoves the chips
  already on the bar aside. Closed, the chevron points back over the drawer and `--open` rotates it
  180°; **the icon has to be directional**, a rotated symmetrical glyph reading as nothing.
- **The strip does not author text.** `set_overflow_tooltip` takes the wording; the widget joins
  what it is given for the accessible name and invents none of it.

## TooltipCard

Icon, title, body and a status line, for a tray item's `ToolTip` — which is four fields, not a
string, and loses its icon and its title/body split the moment it is flattened into one.

- **It reports itself invisible when every field is empty**, so a host shows no tooltip rather than
  an empty box. An icon alone is still a tooltip; a card with only an icon reserves no text column.
- **Caps are the widget's, because the text is another application's**: 128 characters of title,
  512 and six lines of body. A sync log would otherwise grow the tooltip past the screen.
- Title, body, status and `icon-name` are GObject properties, which is what lets its states board
  be pure Blueprint.

## Calendar

- **Four measurements are tokens on `.calendar` itself.** The selection ring is not one: it is `2px`
  inside a `box-shadow`, and the pixel lint recognises `px` by property name.
- **Month names use `%OB`, not `%B`**, which is the form a date is built from. English does not
  distinguish them, which is what makes it easy to ship broken.
- **Dots are drawn, not styled**, and `measure` reports the same height with or without events.
- **`select` compares before it writes, and that guard is load-bearing** — it emits `day-selected`,
  so a handler that reacts by selecting overflows the stack without it.
- **Weekdays are numbered as `glib::DateTime` numbers them**, Monday 1 through Sunday 7; the letters
  come from January 2024, whose 1st was a Monday, so `%a` gives the locale's own abbreviations.

## Row, SplitRow and Placeholder

```
[ check ] [ lead ] [ title    ]  ←space→  [ value ] [ spinner ] [ trail ]
                   [ subtitle ]
```

- **It navigates, it does not expand.** A popover's height is capped by the work area, so expanding
  row 15 of 20 hides the thing just revealed. Expand only one or two rows, and only from a list that
  cannot grow.
- **`icon-name` and `value` are properties; `lead` and `trail` stay slots.** Without properties a
  `.blp` names the type and nothing else, and separate widgets let a row carry a value *and* a
  chevron.
- **`selectable` and `selected` are separate**, so a selectable row reserves the check column before
  anything is selected and selecting one shifts no label in the list.
- **`busy` spins where the value sits**; a word like "Connecting…" beside it says it twice.
- **Sizes are rule-scoped tokens** declared in `.row` itself; `:root` stays the shared vocabulary.
- **`.row` must reset `font-weight`.** libadwaita styles bare `button` bold and weight inherits, so
  every row would render bold — and the grammar distinguishes a selected row by weight.
- **`SwitchRow` is the toggle row.** Its body flips the knob and the knob's `notify::active` is the
  only emitter, so the row and the switch can never double each other. **`locked` disables the knob
  and makes a row-body click a no-op, but never the row itself** — `set_sensitive(false)` on the row
  would dim the subtitle explaining the lock, the same defect `Fader::toggleable` exists to avoid.
- **`Placeholder` stands where content would be**; its `error` flag only recolours the icon.
- **`SplitRow` wraps a `Row` rather than subclassing one**, or its trailing button lands inside the
  row's box where `Row` would have to know about it. Its hairline is a `Gtk.Separator`: the pixel
  lint allows `border:` but not `border-left:`.

## InhibitorList

Each inhibitor is a regular `Row`. Clicking it opens a drawer with its source and targets. A
releasable inhibitor has a clickable Cancel row inside the drawer. Drawer chevrons rotate downward
when their rows open.

- **`InhibitorEntry`/`InhibitorSource`/`InhibitorTargets` are local to this crate**, per the widget
  boundary rule above; `InhibitorSource` maps to a lead icon internally, not carried as a string.
- **The Cancel row hides when `can_release` is false.** The handler reads the current entry by
  position, because rows are reused across updates and an id captured at construction can go stale.
- **A states board declares `$InhibitorList` and feeds real data through `set_inhibitors`**, the same
  way `tray_states` tags each `TrayStrip` with a `demo__<case>` class.

## Section, EventList and WorldClock

- **Visibility toggle, not a `Gtk.Stack`** — a stack sizes to its largest page, so a placeholder
  reserves its height under a four-row agenda.
- **`when` arrives formatted; a `Zone` does not.** Derive in the widget when formatting destroys the
  derivation — `"00:47"` has thrown away that it is tomorrow there.
- **`EventList` defaults to inert, the overflow row exempt** — the flag would otherwise make it inert
  in exactly the case that puts it on screen.
- **`EventList` answers the tooltip, not the row**: GTK picks tooltips and skips a non-activatable
  row, so the list maps the pointer's `y` onto row allocations. **Times use `tabular-nums`**, since
  proportional digits give up to 17px of animated jitter.
- **`Zone::note` and `Zone::icon_name` travel together**, or a sun sits above "light rain", and the
  icon carries no colour. A second line appears only when the date differs in that instant's own
  timezone — pass a local `DateTime`.
- **A zone that does not resolve reads `—`**: `g_time_zone_new_identifier` returns NULL where the
  older `g_time_zone_new` silently returns UTC, hence the `v2_68` feature.

## NotificationCard, NotificationHeader and the body widgets

- **The card subclasses `Gtk.Widget`, not `Gtk.Button`.** It contains close and action buttons, so
  its default action is a gesture plus keyboard activation on the root. `:hover` is on
  `.notification`, so the actions row does not read as a detached strip below a card.
- **Every action carries `min-height: 0`.** Adwaita gives every `Gtk.Button` an intrinsic minimum
  that GNOME's St buttons do not have. This is a toolkit difference, not a style preference.
- **The image is bounded before it reaches the card, because the sender chose it**: header checked
  before decoding, dimensions above 4096px refused, centre-cropped, decoded toward 64px. **Compare
  the source, not the result** — `bound` builds a new texture every resample, so comparing its
  output never matches and the image is resampled twice on the main loop.
- **Urgency is behaviour, not appearance.** `Critical` persists and ignores do not disturb and looks
  like everything else; `set_urgency` stores the value and writes no CSS class.
- **What a screen reader hears is assembled in `announce`, and it is the whole card** — unread
  first, because it decides whether the rest is worth hearing. Every leaf is `presentation`, and the
  close button takes the summary too, or twenty cards give twenty `Dismiss, button` tab stops.
- **Body text is the one place `set_markup` is called**, and only through `body-markup`, whose
  setter runs `pango::parse_markup` first: a `GtkLabel` handed markup Pango refuses renders **empty**
  rather than showing raw tags. Callers sanitize first through `glimpse_utils::markup`.
- **A refused body falls back to `plain`, not the markup string.** `plain` decodes the five XML
  entities Pango knows plus `&nbsp;`, leaves anything unrecognised as written, and bounds the search
  for a reference's `;` by **characters**: a byte bound slices inside `&` followed by six `é`.
- **Each custom widget binds its template root**, so `dispose_template` unparents the whole subtree.

There is no inline reply and no timer here: a reply field needs keyboard focus, which a panel
popover cannot take, and expiry is the stack's business.

## NotificationList and NotificationStack

`NotificationList` takes `&[Notification]` and reconciles **by key**, because notifications arrive
and leave from the middle; matching on position rebuilds every row below the change and destroys
hover, focus and any pending press. The key can therefore be captured when the row is built.
`PlayerList` reuses by position and reads its key back at signal time for exactly that reason.

- **`set_cap` hides rows rather than dropping them**, so expanding is a visibility flip and a row
  mid-hover survives it.

- **It has no `BoxLayout`, and could not have one.** The cards behind must overlap the front one and
  sit against its *measured* height; a box cannot overlap children, and `Gtk.Overlay` takes its size
  from its main child — the very height it is being asked to produce.
- **Paint order is child order, so strips are parented ahead of the front card.** One parented after
  it draws *over* it, as a bar across the bottom rather than an edge peeking out.
- **The strips mix toward the foreground rather than shading.** `shade()` moves lightness one
  absolute way, so "recede" reads on white and disappears on charcoal.
- **It reconciles by key without `reconcile::by_key`** — that helper asserts the items are the
  parent's *only* children and would fight `arrange` over the strips every update.
- **The strip's corner radius is written out rather than shared**: `--gl-notification-radius` is
  declared on `.notification`, and a strip is a sibling the variable does not reach.

## Popovers

`PopoverShell` frames every applet popover: optional hero, one content child, optional footer, a
separator between each pair. An applet can suppress its footer separator when no divider is needed.

- **A section and its hairline show and hide together**, the shell watching `notify::visible` on
  what is appended; hiding the section alone leaves a line floating against nothing.
- **`add_child` must ignore the widget's own template children**, guarded by `try_get().is_none()`:
  `init_template` adds them through `Gtk.Buildable`, so an unguarded override routes `hero_box` into
  `content_box` and panics.
- **The shell paints its own surface, draws no shadow and does not scroll.** A `Gtk.Popover`'s
  `contents` node draws one already, two radii show at every corner, and capping height belongs to
  whatever knows the anchor's work area.

Per-popover rules that are traps rather than taste:

- **A switch driven from state never reports it back** — `set_dnd` and the bluetooth power switch
  raise a guard flag while writing, and `SwitchRow::set_active` compares and silences its own knob.
  Anything drawn from the same value is set *above* the guard: it stops a set being reported, not
  drawn.
- **`.column` carries the width floor, not `.popover-shell`**, and `same_rows` compares only what
  the list draws — a title changes on every keystroke.
- **Static wording lives in the template; wording the data decides lives in Rust.** A fixed label is
  `_("…")` in the `.blp`, a slot holding two a `Gtk.Stack` of `$Placeholder` pages; a plural is
  `ngettext` with named `{placeholders}`, which `format!` cannot reorder.
- **A `NetworkPopover` row is a `$SplitRow`**: the body activates it, the chevron unfolds its card,
  and a line keyed by network *and* action cannot act on the one no longer shown.
- **A network is asked for a password on a page of its own popover, not in a window beside it.**
  `NetworkPopover::set_prompt` swaps the list for the ask, makes the hero insensitive so the Wi-Fi
  switch is not a way out of a question, and **clears the box whenever the key changes**. **What a
  box accepts is decided by what is asked** — a passphrase is 8 to 63 characters or 64 hex digits,
  an SSID at most 32 octets, a VPN token only bounded — since one rule for all three rejects a valid
  token and accepts a password NetworkManager refuses. **A hidden network chooses its own security**
  and *None* takes the box away. `SecretDialog` asks the same on the host. **The Wi-Fi switch goes
  insensitive under a hardware block**, which reads as refused, not merely off.
**A detail unfolds in place, never beside the list.** `crate::drawer` builds the holder — a row with
its own `Gtk.Revealer` under it — so a card grows down instead of sideways off an output edge. The
open row takes `.open`, the card `.detail-card`, and a capped list ends in an overflow row. **No
card row carries a lead icon**: the head above it already names the thing, so a column of glyphs
beside one-word labels is decoration the eye has to step over. **What
recedes follows the row the list shows, not the id asked for**: a hidden section takes the card.
**`IdlePopover`'s hold switch is a second master control, not a readout**, emitting `hold-toggled` the
same as an indefinite preset does; its six preset buttons carry their durations hardcoded in
`imp.rs`, a fixed UI fact the applet has no reason to supply. It reuses the `quiet`-guard above.
Its duration choices use a detail card and dim the rest of the popover while open. Its inhibitor and
footer separators stay hidden until an inhibitor row exists.
**A pairing prompt is a `Gtk.Stack` page, not a dialog.** `BluetoothPopover`'s `pages` swaps the
device column for the question, hero and footer insensitive, `hhomogeneous` on so the card takes the
wider page once. `PairingDialog` keeps the two prompts needing an entry; its `answered` signal
carries `(response, value, numeric)`, since a value alone turns a numeric PIN into a passkey. It
focuses its entry on `map` and clears it when the **device** changes, not the name, so a credential
cannot reach the next device; BlueZ re-asking as a name resolves must not wipe a half-typed one.

## ForecastStrip, ForecastList and the media widgets

- **`ForecastDay` subclasses `Row`**: `Row` is `IsSubclassable`, its own `Buildable` routes the
  subclass's `[trail]`, and its setters are inherent so a subclass reaches them by `upcast_ref`.
- **`ForecastList`'s children are holders, not rows.** Each day is parented with the revealer that
  unfolds under it, so reach a row through `rows`, never through the list's own children.
- **Row's lead icon is `lead-icon`, not `icon-name`.** `Gtk.Button` already owns an `icon-name` that
  replaces the button's child, so a subclass calling `set_icon_name` resolves the *parent's* setter
  and destroys the row's template. `Hero`, `Notice` and `Placeholder` extend `Gtk.Widget` and keep
  `icon-name`.
- **`set_position` is ignored while the pointer is down, and its increments are set in Rust** — the
  same capture-phase `held` guard and `Adjustment` setup `Fader` reuses (below). `blueprint-compiler
  lint` rejects an `Adjustment` carrying anything but `lower`, `upper` and `value`, so nothing in the
  template can guard them, and losing them kills keyboard seeking silently.
- **Artwork is a `Gtk.Image`**, the only one that can be told how big to be: `Gtk.Picture` reports
  the paintable's natural width, so a cover would set the popover's. It also centres a paintable at
  its own aspect ratio, so cover art arrives already square or the corners read as broken.
- **`Pixbuf::file_info` reads dimensions out of the header without decoding**, so an oversized
  `mpris:artUrl` is refused before anything expands it in memory. Scaling is by the shorter side,
  only downward, cropped from the middle; `cover()` is pure arithmetic.
- **A widget built before `Styles::install()` picks up none of it.** Order matters, rooting does not.

## Fader

- **Lifts `Scrubber`'s drag guard wholesale** — the same `held: Cell<Option<f64>>`, capture-phase
  `EventControllerLegacy` and `connect_unmap` reset, so an incoming state update cannot fight a drag.
- **`set_muted` raises `quiet` around `ToggleButton::set_active`**, as `SwitchRow::set_active` does,
  or a muted render fires `toggled` on its own.
- **`maximum` (default 100 — the audio popover needs no change) puts value and clamp in the
  device's own units**, and `set_value`/`connect_change_value` clamp to it, never to a literal 100.
  Increments (`max(1, max/100)`, `max(1, max/20)`) are a pure function of `maximum`, computed by
  the single `apply_increments` also run at construction — cache them once and a later
  `set_maximum` leaves two same-state faders answering arrow keys differently. Still not in the
  template, for the same `blueprint-compiler lint` reason as `ForecastList`'s `Adjustment` above.
- **`toggleable: false` swaps the leading `ToggleButton` for a plain, non-dimmed `Gtk.Image`** — an
  insensitive `ToggleButton` renders dimmed, the bug this property avoids. `set_icon_name` writes
  both children unconditionally, and `.fader__icon` matches `.fader__mute`'s min size so the track's
  edge does not shift with the presentation.
- **`floor` (default 0 — the audio popover needs no change) moves the adjustment's lower bound**,
  the same shape as `maximum`: the getter reads `adjustment().lower()` rather than a stored `Cell`,
  and `set_value`/`connect_change_value` clamp to `floor..=maximum`, never to a literal `0`. Raising
  `floor` past the current value pulls the value up to it, because `GtkAdjustment::set_lower` does
  not re-clamp `value` on its own. A `floor` above the current `maximum` is clamped down to it, and
  a `maximum` set below the current `floor` is clamped up to it — both setters guard the same
  direction so neither can invert the range and panic the next `f64::clamp`. A negative `floor` is
  clamped the same way a negative `maximum` already is. Neither setter guards a non-finite value:
  `f64::max`/`min` turn `±∞` finite, but a `NaN` write panics instead of being rejected —
  `g_param_value_validate`'s `CLAMP` leaves `NaN` unchanged, and glib-rs reads that as changed (`NaN
  != NaN`) before either setter body runs.

## SourceList

- The second shape, exactly `PlayerList`'s: no template, a `BoxLayout` set in `class_init`,
  children parented at runtime. A row and its `Fader` are parented as siblings one after the
  other, never nested, so the row's `activatable: false` cannot reach the fader beneath it.
- A source's own `maximum` and `floor` pass straight through to the `Fader`, `maximum` first —
  `Fader::set_floor` is the one place that reconciles the two, clamping a floor above the maximum
  down to it, never the maximum up to the floor. `SourceList` must not pre-clamp either value, or
  the fader's own clamp never fires.
- Each fader is built with `toggleable: false` and `display-brightness-symbolic` as its icon: a
  brightness source has nothing for a mute button to mute.
- `Source.key` is read back when a fader reports `changed`, not captured when the row was built,
  for the reason `Player.key` documents: a reconcile reuses a row in place.

## DisplayList

- Also the second shape. Each entry is a `crate::drawer::holder`; `OPEN` and `RECEDED` are read
  back from `Gtk.Revealer::reveals_child()` on every apply rather than from a stored index, so a
  reconcile can never disagree with what is actually on screen.
- The detail is a `FactList` (make, model, serial, current mode, scale, position — a field the
  snapshot does not carry is left out of the list, never shown as `Unknown`) followed by a
  `SwitchRow` enabling the output. The connector is not repeated there: the head's own title
  already carries it.
- The enable switch's own `locked` is set only when its own output is enabled **and** it is the sole
  one enabled — never on a disabled output, which would strand the user with no way to turn a
  display back on. `DisplayList` sets the property directly rather than walking the switch's
  children for its knob.
- **`set_output_power(false)` removes the switch from the row, rather than locking it.** `locked`
  answers "offered, but this is your last enabled display"; the gate answers "this compositor
  cannot do this at all", and the two never fight because nothing computes or applies a lock while
  the switch is out. `render` reparents the same `SwitchRow` into or out of the body as the gate
  flips — never rebuilt — so a reused row keeps its identity and an open detail stays open either
  way.

## AudioPopover

- **No `Gtk.Stack`.** Bluetooth and network each have a prompt page; sound has none, so the column
  is the whole content.
- **The output master fader carries `.accent`, added in Rust** (`fader.add_css_class("accent")`),
  because `Fader` exposes no accent property. `.fader.accent .fader__track` is the only rule that
  reads it; the input fader stays plain.
- **A card holds one block per role the application has**, output before input, each its own
  `Fader` plus the selectable device rows for that direction; a role with `adjustable: false` still
  renders, insensitive, rather than being hidden. `recede` dims both master faders along with the
  device and application rows, since a card open under them reads against the whole popover.
- **Outputs, inputs and applications each end in their own overflow row** (`more_outputs`,
  `more_inputs`, `more_apps`), on the same footing as `more_paired`/`more_nearby`: the widget only
  shows and labels the row, and leaves whether the fuller list stays open to whoever is asking.

## BrightnessPopover

- **`set_sources` takes the slice already ordered with the current source first.** The caller
  resolves "current" — focused output, then the single internal source, then the first display
  source, then none — and hands it over that way; `primary` mirrors `sources[0]` and `SourceList`
  renders only `sources[1..]`. Handing the whole slice to both would put two faders on the current
  display that do not track each other until a round trip, and would make the hero state a number
  for whichever source enumerated first rather than the one on screen.
- **The hero's `Readout` is the only place a percentage appears.** `primary`, `temperature` and
  every `SourceList` fader alike carry their exact value in a tooltip instead, the audio popover's
  own settled decision — `SourceList` sets it itself, since `BrightnessPopover` cannot reach its
  children to add one after the fact.
- **The night light section keeps its last-good snapshot rather than collapsing on `None`.** The
  provider is a separate process that can restart mid-popover; losing the section for a moment
  reads as a fault. It stays up, greyed by `set_sensitive(false)`, until a fresh snapshot lifts it.
  A snapshot that has never arrived is a different state — no section at all.
- **The temperature rail hides, rather than disables, while the switch is off** — the night light
  service hands its outputs back at that point, and an insensitive override would fight the release.
  **The knob drives it optimistically**: toggling `enabled` shows or hides the rail immediately,
  before `set_night_light` ever reconciles it, per the rule that UI state never waits on a round
  trip.
- **`.fader--warm` is the rail's only styling hook**, reading `--gl-warning-text` rather than the
  accent colour `.fader.accent` uses elsewhere, so it cannot be mistaken for `.indicator--notice`.
  Its selector reads `.fader__track:not(:disabled)`, because it otherwise has the identical
  specificity of `.fader__track:disabled` and sits later in the sheet — without the guard, greying
  the section out under AC-5 would leave the rail looking live and warm rather than muted.
- **The knob's own row carries `_("Warm the screen")`, not `_("Enabled")`.** `DisplayList` already
  owns that msgid for "this output is on"; sharing it would ask one translation to serve two
  unrelated ideas.

## BatteryPopover

Hero readout is the percentage; `$ChoiceList` is the power-mode selector (its first caller);
device rows hide when empty. Battery details is the last row in the column; facts and the
charge-limit switch unfold under it, the same in-place card IdlePopover and NetworkPopover
use. Static labels live in the blueprint.

## SessionPopover

Lock, sleep and power rows are template children; other sessions and the updates row are sections
that hide when empty. Static labels live in the blueprint. The widget emits `action-requested` and
`activate-session` and does not know logind.

## DisplayPopover

- Display rows carry a chevron that rotates down while their detail drawer is open.
- Composes `DisplayList` unchanged; blanking the screens is a plain `$Row` beside it, never a
  `$SwitchRow` — DPMS has no state to sit in, since the first input undoes it and the popover is
  already gone by then.
- **The `displays` section and the blank row are absent, not disabled, once there is nothing to
  act on** — an empty list hides the section, and `output_power: false` hides the blank row.
- **`set_output_power` forwards straight to `DisplayList`'s own setter of the same name.** The
  popover owns no logic of its own here: `DisplayList` decides what its gate means, `DisplayPopover`
  only relays the one value it already tracks for the blank row, so the blank row and every
  per-output switch disappear on the same signal.

## ClipboardPopover and ClipboardList

`ClipboardList` is index-reconciled like `PlayerList`: one `crate::drawer::holder` per clip, a
`$SplitRow` head and its own `Gtk.Revealer`. The row body emits `restored`, the chevron `detailed`,
and the panel underneath emits `pinned` and `removed`. **Every signal reads its id back at fire
time** from the position the row was built for — a row outlives the clip that was in it.

**The detail panel is rebuilt on each reveal rather than cached.** Its pin row reads *Pin* or
*Unpin* depending on the clip, so a panel kept from a previous open would contradict the entry
above it.

**The list owns no wording.** `set_actions` takes the two action labels from the applet, because a
widget owns structure and no content; the popover's only built-in strings are the placeholder's,
which live in the `.blp`.

`ClipboardPopover` holds two of these lists and drives `set_open` across both, so one cannot keep a
panel open while the other opens a second. Its `$Notice` carries only the standing condition a
notification cannot — no data-control protocol at all; a refused command is a notification.

## PlacesPopover

Four independent `$Section`s — bookmarks, places, network, trash — each hiding when it has nothing,
with no exception and no placeholder anywhere. A popover with nothing to list is a hero and a
footer; the user's own bookmarks lead, because they are what was chosen rather than what exists.
Every row is a plain `Row` that opens a location, so the popover emits `activated` and nothing else.

## RemovablePopover

One titleless `$Section`, because the hero already says what the list is. Drives live here rather
than in `PlacesPopover` because their applet appears and disappears with the hardware, while places
are always there — one popover cannot honestly do both.

- **A device row is the third reconcile shape.** Like `SourceList` and `DisplayList`, each drive or
  volume is a runtime-built `Gtk.Box`, not a template, keyed by id through `reconcile::by_key`; a
  row inside it is a plain `Row` or a `SplitRow` depending on whether it needs a click target beyond
  the body, swapped in once and reused rather than rebuilt on every apply.
- **A capacity readout is a `Gtk.ProgressBar` appended after the row, not a property of it** —
  `progress::apply_bar(cell, fraction, class)` builds one lazily on the first `Some(fraction)` and
  removes it again on `None`, so a drive with no mounted filesystem carries no empty bar. The `class`
  argument is the caller's own CSS class, so each caller's margins live in `glimpse.css` under its
  own class rather than as pixel values in Rust; `system_monitor_popover` shares the same function
  under its own class and adds its warning/error coloring as a second class on top.
- **`.row.dimmed` marks present-but-unusable, not absent** — a drive with no media and no volumes to
  browse, still listed, greyed rather than hidden, since ejecting it is still a thing to do.
- **`SplitRow`'s chevron is the eject or unmount control**, never the row body, which stays the
  open/mount target; a read-only volume gets a plain trailing icon instead, since there is nothing
  behind a chevron to unfold.
- **`SystemMonitorPopover` is the same composite-widget shape as `RemovablePopover`**, minus the
  split-row eject/unmount controls it has no equivalent for: two `Section`s (usage tiles, plain
  detail rows) reconciled by key, a usage tile's `Gtk.ProgressBar` shared through
  `progress::apply_bar`, and a `warning`/`error` modifier class layered on top of that bar for
  threshold coloring — the shared function itself knows nothing about severity.

## PrintingPopover

Jobs are the primary section: `$Section`'s `empty` property switches between the row list and a bare
icon `$Placeholder`, and `render_jobs` sets the hero's subtitle (`"No print jobs"` / an `ngettext`
count) from the same job list — the count text lives in exactly one place, the hero, not repeated on
the placeholder. Job and printer rows reuse the plain `$Row` template; its title label already caps
its own width, which is what keeps a long job name from resizing the row.

A job's second line reads `"{printer} · {status}"`, and page progress replaces the status word there
(`"{printer} · Page {n} of {m}"`) rather than sitting in the row's value column, which job rows leave
unset. `.printing-popover .row__subtitle` carries `font-variant-numeric: tabular-nums` in this
widget's own `styles/glimpse.css` rule, because `.row__subtitle` (unlike `.row__value`) has no
tabular figures by default.

**A job and a printer are the same shape: a `$SplitRow` head over its own `Gtk.Revealer`**, built
with `drawer::holder` exactly as `BluetoothPopover` builds a device. The head carries the name, the
status line and a spinner; the chevron opens a `detail-card` of plain `$Row`s. A job's card is its
actions — Pause, Resume, Cancel — and a printer's is what the queue answered about itself.

**An action is a row, never an icon button, and it cannot live in the head.** `Row` is a
`Gtk.Button`, so a button placed inside one is a button inside a button: the outer gesture claims
the press and the inner never emits `clicked`. A headless test cannot catch it — `emit_by_name`
bypasses the gesture entirely — so the shape is the guard. `SplitRow` exists for this reason and
keeps its own control a sibling.

The chevron hides when there is nothing behind it, and the drawer is forced shut in the same pass,
so it can never stand open on an empty card. The widget emits `cancelled`/`paused`/`resumed`, each
carrying the job id, the same shape as `BluetoothPopover`'s `connect_selected`. `Job.printer`,
`Job.status`, `Printer.status` and every `Detail` are caller-supplied, already-cleaned text —
formatting and sanitizing raw CUPS text is the panel applet's job, not this widget's, which is why
no detail label is translated here.

## PrivacyPopover

- **A resource with an action is a `$SplitRow`; a resource with none is a plain `Row`.** `Row` is a
  `Gtk.Button`, so a button nested inside one never emits `clicked` — the shape is the guard, not a
  test, the same reasoning `PrintingPopover` states above for its own actions.
- **The banner is driven by `set_screen_shared`, never inferred from a usage's action.** A
  `WlrScreencopy` capture carries no action at all, so inferring the banner from "some resource has
  an action" would miss it and leave the screen indicator lit with no warning behind it.
- **The reconcile key is `(id, action)`, not `id` alone.** A resource whose action changes rebuilds
  its control rather than keeping a stale one — `by_key` matching on `id` only would reuse a `Row`
  that used to be a `SplitRow`, or the other way round, and leave the wrong widget in the cell.

## Swatch, ColorList and ColorPickerPopover

- **`Swatch` paints its `color` property and nothing else.** Its CSS node is `swatch`, which is
  how `.indicator__extension swatch` sizes it for the bar. CSS owns its size, radius and hairline;
  `overflow: hidden` is what clips the fill to that radius. A color is data, so it is never a CSS
  class per value. `color` is `explicit_notify`: GObject otherwise notifies on every write,
  including an unchanged one.
- **`ColorList` takes finished `Shade`s** — title, subtitle and every `Notation` already rendered by
  the applet — and reports ids and notation keys, never a format it would have to understand.
- **An open detail recedes the popover's hero and footer**, as `ClipboardPopover` does.

## WorkspaceNamePopover

- **`set_name` writes the entry only while the user has not touched it.** A compositor event
  arriving mid-edit would otherwise replace what is being typed.
- **Esc is caught in the capture phase on the popover, not on the entry**, so it closes whatever has
  focus; the entry is focused and its text selected on `map`, when it first has a root to focus in.

## Stylesheets

`Styles` owns the CSS providers for one process. `install()` registers them on the display **once**
and gives every provider libadwaita's concrete effective scheme — GTK treats a provider's `default`
as light, so an automatic request passed through would disagree with a dark application. Installing
twice stacks every rule; `load()` replaces content in place.

| Priority | Source | Holds |
| --- | --- | --- |
| `APPLICATION` | `styles/glimpse.css`, via `include_str!` | the token vocabulary and every component rule |
| `USER` | the theme's sheet for this surface | token redefinitions |
| `USER + 1` | the theme's `dark.css` | what the theme changes in dark |
| `USER + 2` | the user's own `styles.css` | the last word in light |
| `USER + 3` | the user's own `dark.css` | the last word in dark |

**Each owner's dark sheet refines that owner's own base sheet and nothing above it**, so one
precedence rule — user beats theme beats built-in — holds in both schemes.

**A dark sheet is applied on `dark-notify`, with no file or config event behind it.** Under
`ColorScheme::Default` the desktop preference arrives from the portal after `install()` returns, and
a config change touching only `color-scheme` never reaches `load()`, so `Styles` keeps the two dark
paths and re-points their providers from the handler. It is a convenience, not the only route:
`@media (prefers-color-scheme: dark)` works in any sheet.

`set_variant` puts `appearance.theme-variant` on every toplevel as a CSS class. It reads the live
toplevel list rather than a window passed in, because each binary owns a different number of them —
one hidden host for the lock screen, a bar per output for the panel — so re-running it after a
window is created is what covers the new one.

The built-in is compiled in rather than installed, because `load()` points the theme provider at
**one** path: a component rule in a theme is one the first second theme deletes. The shipped
`adwaita` theme is therefore empty, and that is the test.

**`IndicatorSpec.class` is the one styling hook beside `severity`, and it exists because the
severity classes colour the icon and the label together.** A chip that must colour them apart — the
privacy applet's screen cast, a danger-red record glyph beside a timer that stays the bar's own
foreground — names a class and `glimpse.css` decides both. It is not a second severity; a chip
reporting a condition still uses `severity`.

**`parsing-error` does not see a bad token** — a `var()` naming nothing renders transparent with only
a `Gtk-WARNING` on stderr. Hence two guards: every `var()` in the built-in carries a fallback, and
`theme::tests` lints the vocabulary, whose rules are in `.claude/rules/ui.md`.

### Tokens and the type scale

Thirty-eight `--gl-` tokens in `:root`, in three tiers, and a rule may only read the tier below it:
libadwaita's tokens → `--gl-*` → component rules. **Adding a token means updating the count asserted
in `theme::tests` and the number above, together.**

**`--gl-muted`, `--gl-dim` and `--gl-faint` are `alpha(var(--gl-surface-fg), …)`, so they resolve
lower in light than dark** — `alpha()` composites against the surface, which is the opposite colour
in each scheme. This matches `.dimmed` in every Adwaita application; **do not compensate for it.**

**Thickness is `[[panels]] size`, not CSS.** `Panel::set_thickness` calls `set_size_request`, also a
minimum, so GTK takes the larger and a CSS floor silently overrides a smaller configured size.

## Rules

A widget moves here as soon as a second binary needs it: no copy-paste between panel and lock.
