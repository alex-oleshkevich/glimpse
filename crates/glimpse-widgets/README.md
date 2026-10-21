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
- **An ellipsized label shows its text as a tooltip** through `tooltip_when_ellipsized`, decided
  at hover from `Layout::is_ellipsized` so a label that fits shows none; an ancestor below the
  `Native` with its own tooltip wins, so a `Row`'s explicit `tooltip` is never shadowed.
- **Untrusted text is capped and set as plain text**: tray titles,
  MPRIS metadata, SSIDs and device names come from other applications and are unbounded.

## Indicator, IndicatorGroup, Pager, TrayStrip and InhibitorList

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

Each `InhibitorList` row is an `ExpanderRow` opening a card of
what it prevents and, when releasable, a destructive Release — it ends another app's hold, which
"Cancel" does not say.

## Calendar and Placeholder

**A month change slides; everything else repaints in place** — a new month paints into the hidden
page of a two-page `Gtk.Stack` and slides from the side it lies on. **Month names use `%OB`, not
`%B`**, which English does not distinguish, making it easy to ship broken. `select` compares before
it writes and that guard is load-bearing: it emits `day-selected`, so a handler that reacts by
selecting would otherwise overflow the stack.

## Section, EventList, WorldClock and the notification widgets

Sections toggle visibility instead of stacking pages that reserve the largest page's height. A
mapped keyed row that leaves, and a mapped Section that hides, leave a picture of themselves in a
collapsing, fading revealer and go at once, so the real widget is never reparented or kept alive
for an animation. A Section takes that picture a painted frame after the hide, because a row
reconciled out of it in the same turn leaves it unallocated and it would draw nothing. Exits need a
`Gtk.Box` parent; elsewhere, unmapped, or at zero duration they remove synchronously. A widget that
already faded and collapsed itself is removed as it is, never given a second exit, and what replaces
it (the notifications placeholder) slides in during the exit rather than after it.
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

`NotificationStack` reconciles **by key**, because notifications arrive and leave from the middle;
matching on position would rebuild every row below the change and destroy hover and focus. It has no
`BoxLayout`: the cards behind must overlap the front one at its *measured* height, which
`Gtk.Overlay` cannot report while taking its size from that same child, and paint order is child
order, so strips are parented ahead of the front card and clipped to below whatever covers them — a
blurred card is translucent and would show a strip through its own bottom.

## Popovers

Framed applet popovers use `PopoverShell`: `icon-name`, `title` and `subtitle` describe the header,
`[addon]` fills its trailing slot, and plain children scroll in the body with the settings link last.
The shell owns the surface and its regular or wide size. **A row in a list is one line;
its second line is only ever a problem**, in `.row--warning` amber, so rows of a kind keep one height.
**Facts precede actions in every drawer.**
Cards have at most one destructive action, so Forget outranks Disconnect.

**`add_child` must ignore the widget's own template children**, guarded by `try_get().is_none()`:
`init_template` adds them through `Gtk.Buildable`, so an unguarded override routes `card` into
`body` and panics. **A switch driven from state never reports it back** — `set_dnd` and the
bluetooth power switch raise a guard flag while writing, and `ToggleRow::set_active` applies the
state without emitting `toggled`. **A detail unfolds in place, never beside the list**: every detail is an
`ExpanderRow` whose open state makes the whole of it one `.card`, so the head never moves. **Focus
belongs to `PopoverShell`**: opening one detail closes every unrelated `Expandable` in the shell, and
a press on anything else dimmed closes the detail instead of acting, claimed in the capture phase.
**A question the user starts is a page of its own popover**, and `NetworkPopover::set_prompt`
clears the box whenever the key changes. **A question a service raises is a dialog**, so it opens
once however many bars exist: `PairingDialog` covers every pairing kind and clears its entry only
when the **device** changes, not the name, so a credential cannot reach the next device.

## ForecastStrip, ForecastList, the media widgets and SourceList

Artwork is a `Gtk.Image`, the only one that can be told how big to be, since `Gtk.Picture` reports
the paintable's natural width. `Pixbuf::file_info` reads dimensions out of the header without
decoding, so an oversized `mpris:artUrl` is refused before anything expands it in memory.

`SourceList` has no template: a row and its `SliderRow` are parented as
siblings, never nested, so the row's `activatable: false` cannot reach the slider beneath it.
`SliderRow::set_range` takes the device's own floor and maximum and steps by a hundredth of the
maximum, and never less than 1, so neither `SourceList` nor `DisplayPopover` pre-clamps a value.

## DisplayList, DisplayPopover, AudioPopover, PowerPopover and SessionPopover

Each entry is an `ExpanderRow` keyed by connector. The enable toggle cannot be turned off only when its
own output is enabled **and** is the sole one enabled, never on a disabled output, which would
strand the user with no way to turn a display back on. **`set_output_power(false)` removes the
toggle from the row, rather than locking it** — that guard answers "your last enabled display", the
gate answers "this compositor cannot do this at all", and the two never fight. Blanking the screens
in `DisplayPopover` is gated by `set_blank_screens` and is a plain `$ActionRow`, never a `$ToggleRow` — DPMS has no state to sit in, since the
first input undoes it. `AudioPopover` uses no `Gtk.Stack`: sound has no prompt page, unlike bluetooth
or network. **`DisplayPopover::set_sources` takes the slice already ordered with the current
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

## ClipboardPopover, ClipboardList, PlacesPopover and RemovablePopover

`ClipboardList` reconciles by clip id. An image's head is a tile with a badge carrying
`expandable__opener`, since `Expandable` cannot otherwise find the opener inside a head that is
not a row. **The card is built on first open and never rebuilt**, since a clip's
content cannot change under its id — each render only rewrites the pin row's wording.

`PlacesPopover`'s four independent `$Section`s each hide when they have nothing, with no exception
and no placeholder. In `RemovablePopover`, **every device is one `ExpanderRow` in a flat list, keyed by id**, whose
head opens the drawer even when unmounted and carries the accent check while its main volume is
mounted. A drawer reads the space `MeterRow`, one `ActionRow` per extra volume (its size beside the
check, since a `Row`'s `[end]` slot no longer hides the value), Open, then the destructive Eject
last; while busy they stay in place and insensitive. The device icon is the only icon in the
popover, and busy is the head's spinner. Overflow is an `ExpanderRow` "N more" whose drawer holds
the same row widget, with the applet owning its open state and the `quiet` flag muting the echo.

## PrintingPopover, PrivacyPopover, Swatch and WorkspaceNamePopover

**A job and a printer are an `ExpanderRow` whose head opens the drawer; a printer with no facts is a
plain `Row`.** A drawer holds info first, then toggles, then actions, with the destructive one
last: a job's facts, Pause, Resume or Retry as allowed, then Cancel. The job count is the section's
count, and there is no hero. Formatting and sanitizing raw CUPS text is the panel applet's job, not
this widget's.

In `PrivacyPopover`, **a row is one app, and it opens to what it uses and, when it can be stopped,
Stop sharing.** `Mute
microphone` is a `ToggleRow` under the list, not an action in an app's card, because it mutes the
input device rather than that one app. `Swatch` paints its `color` property and nothing else; a
color is data, so it is never a CSS class per value. `WorkspaceNamePopover::set_name` writes the
entry only while the user has not touched it, or a compositor event arriving mid-edit would
replace what is being typed.

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

## EntryRow

`EntryRow` is a `Gtk.Entry` drawn as a row: css name `listrow`, so it takes the row height, padding
and radius, with the entry flat inside it and its text where a row title starts. The row owns the
`changed` and `activated` signals and forwards them from the entry, because exec blocks a mounted
widget's own handlers while it dresses it; **an owner `set_text` never emits `changed`** and keeps the
caret, clamped to the new length. The text caps at 4096 characters, the title, placeholder and
tooltip at `TEXT_MAX_CHARS`. One end child sits in `[end]`; `inactive` makes the entry and that child
insensitive together. Only `warning` and `error` tint it.

In exec a bare control (an `entry`, a `box` of buttons, a `scale`; anything that is not a section,
row, separator, revealer or fact list) carries `exec-bare` when its parent stacks vertically: a
section, the popover, a revealer or a container. The class spaces it from its neighbours and from
the parent's end with margins, since rows stack flush and box spacing would separate them too. A
`box` or `stack` lays out its own children, so they never carry it. The entry row rests on its own
`--row-entry-bg` fill with a hairline, never `--gl-hover`, so it never reads as a hovered row, and
keeps a small margin from an adjacent row so two filled rows never merge.

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

**`parsing-error` does not see a bad token** — a `var()` naming nothing renders transparent with
only a `Gtk-WARNING` on stderr, so every `var()` in the built-in carries a fallback.

Tokens live in `:root` in three tiers, and a rule may only read the tier below it: libadwaita's
tokens → `--gl-*` → component rules. **`--gl-muted`, `--gl-dim` and `--gl-faint` resolve lower in
light than dark**, matching `.dimmed` in every Adwaita application; do not compensate for it.
**`--gl-duration` belongs to `[appearance] animation-speed`, not to a sheet**, since GTK cannot hand
a CSS value back to code. **Thickness is `[[panels]] size`, not CSS** — `set_size_request` is also a
minimum, so a CSS floor silently overrides a smaller configured size.

## Blur

`blur::apply(window, enabled)` sets class `blurred` when enabled, GTK is 4.23.3 or newer and the
compositor advertises blur; below that surfaces stay opaque. **A widget blurs only if a
`window.blurred` rule gives it `backdrop-filter: blur(var(--gl-blur-radius))`**: GTK sends the region
itself, so **glimpse never creates an effect object** — a second one is a protocol error on GTK's own
connection, and the capability probe only reads. **The radius is a switch, not a strength**: GTK
reports no region below 10px. **An ancestor with `opacity < 1` drops the region**, so a fade sets
opacity on the blurred widget itself and a blur, which cannot fade, is gated at half shown. **niri blurs a layer surface in xray mode** unless a `layer-rule` sets
`background-effect { xray false; }`, which is the user's, not installed here.

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
