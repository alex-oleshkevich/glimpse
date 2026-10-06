# glimpse-widgets

Shared GTK4 widgets: GObject subclasses, Blueprint templates and the CSS they expect. Used by the
panel, the notification popup process and the lock screen.

## Layout

- `src/<widget>/` — one directory per widget, `mod.rs` plus `imp.rs`
- `blueprints/` — `.blp` templates, compiled by `build.rs` through `blueprint-compiler`
- `resources/widgets/` — generated `.ui`, bundled into `glimpse-widgets.gresource`

Adding a template is three edits: the `build.rs` pair, the `gresource.xml` entry, the module in
`lib.rs`. Resource prefix `/me/aresa/GlimpseShell`. **Every type a template names must be bound as a
`TemplateChild`, even one Rust never reads** — binding registers the GType before `init_template`
resolves the class by name, or `Builder` reports `Invalid object type` and the constructor panics.

## Recurring rules

- **Every setter compares before it writes**; `gio::Icon` compares with `Icon::equal`.
- **`css_classes` on a builder replaces the list, and `has-frame: false` *is* the `flat` class** —
  add classes after `build()`, or a chain setting both paints a button behind a frameless icon.
- **`get_visible()`, not `is_visible()`** — the second walks ancestors, so a `Row` inside a hidden
  `Section` reports no title.
- **Inactive rows block input, never backend updates.** They fade once and leave Tab navigation;
  slider recovery icons stay usable. Expander heads always permit inspection, and inactivity never
  propagates to drawer children.
- **Both labels cap their natural width**, because `ellipsize` lowers only a label's *minimum*.
- **Untrusted text is capped and set as plain text, with no markup setter anywhere**: tray titles,
  MPRIS metadata, SSIDs and device names come from other applications and are unbounded.

## Indicator, IndicatorGroup, Pager, TrayStrip, TooltipCard and InhibitorList

The icon is one `Option<gio::Icon>`, sitting in a `Gtk.Overlay` whose `overlay` is an emblem on the
trailing corner; the slot follows the base icon's presence, so an indicator with neither reserves no
space. `extension` reparents only on identity change, so an applet keeps one widget and updates it
in place. **A badge hides the attention dot but must not cancel attention itself** — two marks for
one fact is noise, so `indicator--attention` keeps colouring the chip while the dot yields. `Pager`
is a click per slot; an item takes no click of its own, since `GtkButton` restricts its gesture to
the primary button and acting on a workspace happens in the popover.

`TrayStrip` renders one `Indicator` per `TrayChip`, a remote object `IndicatorGroup` cannot express.
**The chevron is pinned to one edge and hidden chips grow away from it**: a right-zone applet wants
`Edge::Start`, a left-zone one `Edge::End`, and the slide direction and `halign` follow that pair, or
the strip shoves the chips already on the bar aside.

`TooltipCard` reports itself invisible when every one of its four fields is empty, so a host shows
no tooltip rather than an empty box. Each `InhibitorList` row is an `ExpanderRow` opening a card of
what it prevents and, when releasable, a destructive Release — it ends another app's hold, which
"Cancel" does not say.

## Calendar, LegacyRow, SplitRow and Placeholder

**A month change slides; everything else repaints in place** — a new month paints into the hidden
page of a two-page `Gtk.Stack` and slides from the side it lies on. **Month names use `%OB`, not
`%B`**, which English does not distinguish, making it easy to ship broken. `select` compares before
it writes and that guard is load-bearing: it emits `day-selected`, so a handler that reacts by
selecting would otherwise overflow the stack.

```
[ check ] [ lead ] [ title    ]  ←space→  [ value ] [ spinner ] [ trail ]
                   [ subtitle ]
```

**It navigates, it does not expand** — a popover's height is capped by the work area, so expanding
one row in a long list hides what it just revealed. `.row` must reset `font-weight`, since libadwaita
styles bare `button` bold and the grammar distinguishes a selected row by weight. `SwitchRow`'s
`locked` disables the knob and makes a row-body click a no-op, but never the row itself —
`set_sensitive(false)` would dim the subtitle explaining the lock. `SplitRow` wraps a `LegacyRow` rather
than subclassing one, so its trailing button lands inside the row's box without `LegacyRow` knowing.

## Section, EventList, WorldClock and the notification widgets

Sections toggle visibility instead of stacking pages that reserve the largest page's height. A
mapped keyed row that leaves, and a mapped Section that hides, leave a picture of themselves in a
collapsing, fading revealer and go at once, so the real widget is never reparented or kept alive
for an animation. A Section takes that picture a painted frame after the hide, because a row
reconciled out of it in the same turn leaves it unallocated and it would draw nothing. Exits need a
`Gtk.Box` parent; elsewhere, unmapped, or at zero duration they remove synchronously.
`PopoverShell` scrolls only when its body exceeds the cap, since an unused scrollbar still adds its
minimum height.

**`when` arrives formatted; a `Zone` does not** — derive in the widget when formatting destroys the
derivation, since `"00:47"` has thrown away that it is tomorrow there. A zone that does not resolve
reads `—`, since `g_time_zone_new_identifier` returns NULL where the older `g_time_zone_new` silently
returns UTC.

The card subclasses `Gtk.Widget`, not `Gtk.Button`, so its default action is a gesture plus keyboard
activation on the root. **The image is bounded before it reaches the card, because the sender chose
it**: dimensions above 4096px refused, centre-cropped, decoded toward 64px. Urgency is behaviour, not
appearance: `set_urgency` stores the value and writes no CSS class. **Body text is the one place
`set_markup` is called**, only through `body-markup`, whose setter runs `pango::parse_markup` first —
refused markup renders **empty**, not raw tags, so callers sanitize first through
`glimpse_utils::markup`, and a refused body falls back to `plain`, never the markup string. There is
no inline reply here: a reply field needs keyboard focus, which a panel popover cannot take.

`NotificationList` reconciles **by key**, because notifications arrive and leave from the middle;
matching on position would rebuild every row below the change and destroy hover and focus. `set_cap`
hides rows rather than dropping them, so expanding is a visibility flip. The stack has no
`BoxLayout`: the cards behind must overlap the front one at its *measured* height, which
`Gtk.Overlay` cannot report while taking its size from that same child, and paint order is child
order, so strips are parented ahead of the front card and clipped to below whatever covers them — a
blurred card is translucent and would show a strip through its own bottom.

## Popovers

Framed applet popovers use `PopoverShell`: optional hero, one content child, optional footer, a
separator between each pair, and every separator paints transparent. **A row in a list is one line;
its second line is only ever a problem**, in `.row--warning` amber, so rows of a kind keep one height.
**Facts precede actions; network and Bluetooth lead with Connect, Pair or Disconnect.**
Cards have at most one destructive action, so Forget outranks Disconnect.

**`add_child` must ignore the widget's own template children**, guarded by `try_get().is_none()`:
`init_template` adds them through `Gtk.Buildable`, so an unguarded override routes `card` into
`body` and panics. **A switch driven from state never reports it back** — `set_dnd` and the
bluetooth power switch raise a guard flag while writing, and `SwitchRow::set_active` compares and
silences its own knob. **A detail unfolds in place, never beside the list**: every detail is an
`Expandable` whose `expanded` makes the whole of it one `.card`, so the head never moves. **Focus
belongs to `PopoverShell`**: opening one detail closes every unrelated `Expandable` in the shell, and
a press on anything else dimmed closes the detail instead of acting, claimed in the capture phase.
**A network is asked for a password on a page of its own popover, not a window beside it**, and
`NetworkPopover::set_prompt` clears the box whenever the key changes. **A pairing prompt is a
`Gtk.Stack` page, not a dialog**, and `PairingDialog` clears its entry only when the **device**
changes, not the name, so a credential cannot reach the next device.

## ForecastStrip, ForecastList, the media widgets, Fader and SourceList

**LegacyRow's lead icon is `lead-icon`, not `icon-name`** — `Gtk.Button` already owns an `icon-name` that
replaces the button's child, so a subclass calling `set_icon_name` destroys the row's template.
Artwork is a `Gtk.Image`, the only one that can be told how big to be, since `Gtk.Picture` reports
the paintable's natural width. `Pixbuf::file_info` reads dimensions out of the header without
decoding, so an oversized `mpris:artUrl` is refused before anything expands it in memory.

`Fader` lifts `Scrubber`'s drag guard wholesale, a capture-phase `held: Cell<Option<f64>>`, so an incoming
state update cannot fight a drag. **`maximum` and `floor` put value and clamp in the device's own
units**, never a literal 100 or 0, and clamp each other on write so neither setter can invert the
range; both skip `NaN`, since `g_param_value_validate`'s `CLAMP` leaves it unchanged and glib-rs
reads that as changed (`NaN != NaN`) and panics before either setter body runs. `toggleable: false`
swaps the leading `ToggleButton` for a plain, non-dimmed `Gtk.Image`, since an insensitive
`ToggleButton` renders dimmed. `SourceList` has no template: a row and its `SliderRow` are parented as
siblings, never nested, so the row's `activatable: false` cannot reach the slider beneath it.
`SliderRow::set_range` takes the device's own floor and maximum and steps by a hundredth of the
maximum, so neither `SourceList` nor `DisplayPopover` pre-clamps a value.

## DisplayList, DisplayPopover, AudioPopover, PowerPopover and SessionPopover

Each entry is an `ExpanderRow` keyed by connector. The enable toggle cannot be turned off only when its
own output is enabled **and** is the sole one enabled, never on a disabled output, which would
strand the user with no way to turn a display back on. **`set_output_power(false)` removes the
toggle from the row, rather than locking it** — that guard answers "your last enabled display", the
gate answers "this compositor cannot do this at all", and the two never fight. Blanking the screens
in `DisplayPopover` is a plain `$ActionRow`, never a `$ToggleRow` — DPMS has no state to sit in, since the
first input undoes it. `AudioPopover` uses no `Gtk.Stack`: sound has no prompt page, unlike bluetooth
or network, and its output master fader carries `.accent`, added in Rust, because `Fader` exposes no
accent property. **`DisplayPopover::set_sources` takes the slice already ordered with the current
source first**; `SourceList` renders only `sources[1..]`. Its night light section keeps its
last-good snapshot rather than collapsing on `None`, since the provider is a separate process that
can restart mid-popover — a snapshot that has never arrived is a different state, no section at all.

`PowerPopover`'s Battery drawer shows while it has facts or a charge limit, and closes before it
hides; History closes before it hides too, and emits `history-opened` only on opening. `SessionPopover`'s other-sessions and
updates sections hide when empty, and the widget emits `action-requested` and `activate-session`
without knowing logind.

## PasswordPrompt, LockClock, SessionSheet, LockStage, StatusIsland, TrackCard and NotificationChips

`PasswordPrompt` knows nothing about PAM: `submitted` and `edited` never carry the text. **The
password leaves the entry exactly once** — `take_text` copies the buffer into a `Zeroizing<String>`
and clears it, never `EditableExt::text`, since a `GString` is immutable. `set_busy` makes the entry
non-editable, never `set_sensitive`, which drops focus mid-attempt.

`LockClock::set_formats` takes strftime patterns. **The day suffix follows `LC_TIME`, not the
catalog**: an English, `C` or `C.<codeset>` time locale gets `1st`, anything else the plain number.

`SessionSheet` rows start hidden until `set_action` shows one, and every action goes through a
confirm page that re-checks the row before emitting, since `set_action` can revoke it mid-page.
`StatusIsland` has five fixed `$Indicator` slots with no popovers; `set_*(None)` hides a slot.
`TrackCard`'s labels carry `width-chars` beside `max-width-chars` and a fixed `min-width`, so a
track change never resizes the footer. `NotificationChips` is built with `accessible-role: Img`
through `glib::Object::builder`, since the role is construct-only.

`LockStage::set_session_actions` writes every action first, then keeps the power button in step with
`SessionSheet::has_actions`, so the outcome never depends on order. Escape and a press anywhere but
the sheet or the power button close the sheet, both in the capture phase so the press is consumed.

## ClipboardPopover, ClipboardList, PlacesPopover, KdeconnectPopover and RemovablePopover

`ClipboardList` reconciles by clip id. An image's head is a tile with a badge carrying
`expandable__opener`, since `Expandable` cannot otherwise find the opener inside a head that is
neither a row nor a split row. **The card is built on first open and never rebuilt**, since a clip's
content cannot change under its id — each render only rewrites the pin row's wording.

`PlacesPopover`'s four independent `$Section`s each hide when they have nothing, with no exception
and no placeholder. In `KdeconnectPopover`, **a card's rows are rebuilt only when its actions
change**, so the row under the pointer of an open card is never swapped out, and a nearby row spins
the moment it is pressed and ignores a second press while it does. In `RemovablePopover`, **every volume is one `ExpanderRow` in a flat list, keyed by id**, whose
head opens the drawer even when idle (`inactive` dims it, nothing more). A drawer reads capacity
`MeterRow`, facts, Open or Mount, then the destructive Eject (Unmount when the drive cannot eject)
last. The device icon is the only icon in the popover, and busy is the row's spinner. The applet
supplies the final "Drive · Volume" title. Overflow is an `ExpanderRow` "N more" whose drawer holds the
same row widget, with the applet owning its open state and the `quiet` flag muting the echo.

## PrintingPopover, PrivacyPopover, Swatch, ColorList, ColorPickerPopover and WorkspaceNamePopover

**A job and a printer are an `ExpanderRow` whose head opens the drawer; a printer with no facts is a
plain `Row`.** A drawer holds info first, then toggles, then actions, with the destructive one
last: a job's `FactList`, Pause or Resume as allowed, then Cancel. The job count is the section's
count, and there is no hero. Formatting and sanitizing raw CUPS text is the panel applet's job, not
this widget's.

In `PrivacyPopover`, **a row is one app, and it opens to what it uses and, when it can be stopped,
Stop sharing.** `Mute
microphone` is a `SwitchRow` under the list, not an action in an app's card, because it mutes the
input device rather than that one app. `Swatch` paints its `color` property and nothing else; a
color is data, so it is never a CSS class per value. `ColorList` takes finished `Shade`s, already
rendered by the applet, and reports ids and notation keys, never a format it would have to
understand. `WorkspaceNamePopover::set_name` writes the entry only while the user has not touched
it, or a compositor event arriving mid-edit would replace what is being typed.

## Bars and Sparkline

Both are snapshot-drawn and take their colour from the CSS `color` of the widget; every dim part
(the older bars, the empty tick, the area fill) is that colour at a lower alpha, so dark and light
need no second rule. Both have accessible role `img` and a char-capped `label`, take at most 366
values and keep the newest, and treat a `None` as no data. Setters compare before they write.

`Bars` sizes itself from its allocation and a caller sets no width. `layout` is a pure function
returning integer widths of at least 1 that never leave `0..width`: the gap shrinks to 0 before a
bar goes below 1px, and when there are more values than pixels the series is bucketed by maximum so
a spike survives. A value that is negative or not finite is no data, not a bar. A non-zero value is
at least 2px tall, zero is a 1px baseline, no data is a fainter 1px tick, and an explicit maximum
clamps an overshoot rather than overflowing. The newest bar is the only one at full alpha while
`highlight_last` is on.

`Bars::set_tooltips` is parallel to the values and plain text only, capped by chars, shown with
`Tooltip::set_text` like `EventList`. A hover resolves through the same `layout` as the drawing:
a gap maps to the nearer bar, past the last pixel is no tooltip, and a bucketed bar reports the
element holding its maximum.

`Sparkline` scales min..max of the finite values with 10% vertical padding so the stroke never
clips, draws a flat series as a centered line, breaks the line at every `None`, draws an isolated
point as a dot, and clips to its bounds. A gap is a position on the x axis, not a skipped one.

## Stylesheets

`Styles` owns the CSS providers for one process. `install()` registers them **once** and gives every
provider libadwaita's concrete effective scheme, since GTK treats a provider's `default` as light.

| Priority | Source | Holds |
| --- | --- | --- |
| `APPLICATION` | `styles/glimpse.css`, via `include_str!` | the token vocabulary and every component rule |
| `USER` | the theme's sheet for this surface | token redefinitions |
| `USER + 1` | the theme's `dark.css` | what the theme changes in dark |
| `USER + 2` | the user's own `styles.css` | the last word in light |
| `USER + 3` | the user's own `dark.css` | the last word in dark |
| `USER + 4` | generated by `set_animation_speed` | `--gl-duration`, and nothing else |

**Each owner's dark sheet refines that owner's own base sheet and nothing above it**, so one
precedence rule — user beats theme beats built-in — holds in both schemes. A dark sheet is applied on
`dark-notify`, with no file or config event behind it, since a config change touching only
`color-scheme` never reaches `load()`. The built-in is compiled in rather than installed, because
`load()` points the theme provider at **one** path.

**The applet host's `gtk4::ProgressBar` is styled by bare node selectors** — `progressbar trough`,
`progressbar progress`, `progressbar.warning progress` and `progressbar.error progress` — reading
`--gl-faint`, `--gl-muted`, `--gl-warning-text` and `--gl-danger-text`, so a theme retints it by
redefining those tokens. The selectors carry no class, which keeps their specificity below every
native bar rule (`.capacity-bar`, `.notification__progress`); each of
those sets the same four properties itself, so the bare rule never reaches a native bar.

**`parsing-error` does not see a bad token** — a `var()` naming nothing renders transparent with
only a `Gtk-WARNING` on stderr, so every `var()` in the built-in carries a fallback.

Tokens live in `:root` in three tiers, and a rule may only read the tier below it: libadwaita's
tokens → `--gl-*` → component rules. **`--gl-muted`, `--gl-dim` and `--gl-faint` resolve lower in
light than dark**, matching `.dimmed` in every Adwaita application; do not compensate for it.
**`--gl-duration` belongs to `[appearance] animation-speed`, not to a sheet**, since GTK cannot hand
a CSS value back to code. **Thickness is `[[panels]] size`, not CSS** — `set_size_request` is also a
minimum, so a CSS floor silently overrides a smaller configured size.

## Blur

`blur::Blur` asks the compositor to blur what lies behind chosen widgets of one window, through
`ext-background-effect-v1`. With no protocol the surfaces stay opaque.

**`sync` guards against its own reentrancy**: GTK can invoke `attach`/`detach` nested, and a
`g_signal_emit` trampoline cannot unwind a Rust panic, so a double `RefCell` borrow is a whole
process abort. **It borrows GTK's own Wayland connection**, so a protocol error on it kills GTK —
the effect object exists only while blur is enabled **and** the window is mapped. **A shape is
blurred only while at least half shown**: a blur cannot fade, so from zero it paints a blurred block
before the content, and from full it leaves the finished content over a sharp background. **Corner
radii are read from what GTK drew**, never duplicated from CSS, through the widget's last render
node. **niri blurs a layer surface in xray mode** — the wallpaper, not the windows between — unless a
`layer-rule` sets `background-effect { xray false; }`, which is the user's, not installed here.

## Screenshot

`ShotToolbar`, `ShotWindowList`, `ShotRegion`, `ShotBanner`, `ShotCountdown`, `ShotCapturePage`,
`ShotResultPage`, `ShotPreferences` and `shot_shortcuts()` are the screenshot surfaces. They take plain values and emit signals; the
`glimpse-shot` binary owns capture, files and the discard dialog.

- **A setter that is handed the value already showing writes nothing and emits nothing.** Toggle
  buttons report a choice only when the click turns that button on, so pressing the active tool
  again is silent. Programmatic `set_*` never emits.
- **Tool ids are `crop`, `arrow`, `rect`, `filled-rect`, `oval`, `number`, `text`, `blur`.** Targets are `screen`
  and `window`. Palette index `0..=4` is red, yellow, green, blue, white. Stroke `1..=3` is thin,
  medium, thick. Anything else is ignored.
- **`action` names** `undo`, `redo`, `apply-crop`, `copy-close`, `cancel` and the copy split
  button's menu entries `save`, `save-as`, `open-with`, `open-window`. The menu is the `shot` action
  group; `set_finish_actions` enables only the entries a mode can run, and a disabled entry is
  hidden. Apply crop shows only while `set_apply_visible(true)`. Undo and redo stay insensitive
  until `set_can_undo` / `set_can_redo`.
- **`ShotResultPage` embeds a `ShotToolbar` in its bottom bar with targets, history and finishing hidden**
  (`toolbar()` reaches it); `picture()` is the page's `Gtk.Picture` and `name-changed` fires on each
  edit of the name. Bottom undo and redo emit that toolbar's `action`. The header emits copy, save,
  save-as, open-with and new. The rename popover holds the name; the folder is in Preferences.
  `set_zoom` is `fit` or `actual`.
- **`ShotPreferences` supplies the fixed groups and controls; `glimpse-shot` owns the config.**
  The five color choices carry `shot-color` plus their color class so CSS paints them in either scheme.
  `shot_shortcuts()` loads the six-section shortcuts dialog from the resource.
- **Window titles and prompts are capped by character and set as plain text.** A row's
  first line is `name`, else the title, else the app id; `icon` is used when set. A section with no
  rows is hidden. The focused entry carries `shot-list__row--current`.
- **`ShotRegion` only paints.** The dim, outline, handles and hint are a snapshot; the overlay owns
  the pointer. The size label is hidden until there is both a selection and a size string.
- **A portal request is “{app}” asked for a screenshot, plus a keys line only then.** The name goes
  through `glimpse_utils::text::clean` at 200 characters; an empty name reads "An application".
- **The screenshot icons live in this crate's gresource.** `Styles::install` adds
  `/me/aresa/GlimpseShell/icons` to the display's icon theme, which is what lets `icon-name` recolor
  them. A `file:` image does not.

## Rules

A widget moves here as soon as a second binary needs it: no copy-paste between panel and lock.
