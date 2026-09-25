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
- **A row highlights only if its body acts**; `activatable: false` drops `can-target`, so a row
  holding a control uses `SwitchRow` instead.
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
no tooltip rather than an empty box. Each `InhibitorList` row is an `Expandable` opening a card of
what it prevents and, when releasable, a destructive Release — it ends another app's hold, which
"Cancel" does not say.

## Calendar, Row, SplitRow and Placeholder

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
`set_sensitive(false)` would dim the subtitle explaining the lock. `SplitRow` wraps a `Row` rather
than subclassing one, so its trailing button lands inside the row's box without `Row` knowing.

## Section, EventList, WorldClock and the notification widgets

**Visibility toggle, not a `Gtk.Stack`** — a stack sizes to its largest page, so a placeholder would
reserve its height under a short agenda. **`when` arrives formatted; a `Zone` does not** — derive in
the widget when formatting destroys the derivation, since `"00:47"` has thrown away that it is
tomorrow there. A zone that does not resolve reads `—`, since `g_time_zone_new_identifier` returns
NULL where the older `g_time_zone_new` silently returns UTC.

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
order, so strips are parented ahead of the front card.

## Popovers

`PopoverShell` frames every applet popover: optional hero, one content child, optional footer, a
separator between each pair, and every separator paints transparent. **A detail card has at most one
red action**, `crate::DESTRUCTIVE`: the least reversible one, so Forget outranks Disconnect.

**`add_child` must ignore the widget's own template children**, guarded by `try_get().is_none()`:
`init_template` adds them through `Gtk.Buildable`, so an unguarded override routes `hero_box` into
`content_box` and panics. **A switch driven from state never reports it back** — `set_dnd` and the
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

**Row's lead icon is `lead-icon`, not `icon-name`** — `Gtk.Button` already owns an `icon-name` that
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
`ToggleButton` renders dimmed. `SourceList` has no template: a row and its `Fader` are parented as
siblings, never nested, so the row's `activatable: false` cannot reach the fader beneath it.
`Fader::set_floor` is the one place that reconciles `maximum` and `floor`, so `SourceList` must not
pre-clamp either value.

## DisplayList, DisplayPopover, AudioPopover, BrightnessPopover, BatteryPopover and SessionPopover

Each entry is an `Expandable` keyed by connector. The enable switch's `locked` is set only when its
own output is enabled **and** is the sole one enabled, never on a disabled output, which would
strand the user with no way to turn a display back on. **`set_output_power(false)` removes the
switch from the row, rather than locking it** — `locked` answers "your last enabled display", the
gate answers "this compositor cannot do this at all", and the two never fight. Blanking the screens
in `DisplayPopover` is a plain `$Row`, never a `$SwitchRow` — DPMS has no state to sit in, since the
first input undoes it. `AudioPopover` uses no `Gtk.Stack`: sound has no prompt page, unlike bluetooth
or network, and its output master fader carries `.accent`, added in Rust, because `Fader` exposes no
accent property. **`BrightnessPopover::set_sources` takes the slice already ordered with the current
source first**; `SourceList` renders only `sources[1..]`. Its night light section keeps its
last-good snapshot rather than collapsing on `None`, since the provider is a separate process that
can restart mid-popover — a snapshot that has never arrived is a different state, no section at all.

`BatteryPopover`'s Health is an `Expandable` whose card is a `$FactList`, hidden and closed when
empty; `.row--warning` turns a row's subtitle and value amber. `SessionPopover`'s other-sessions and
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
the moment it is pressed and ignores a second press while it does. In `RemovablePopover`, **every
drive and volume is an `Expandable`**, keyed by id; the head swapping between `Row` and `SplitRow` is
how a successful mount or unmount closes the card, and the capacity bar leads the mounted card since
an `Expandable` toggles only on its head.

## PrintingPopover, PrivacyPopover, Swatch, ColorList, ColorPickerPopover and WorkspaceNamePopover

**A job and a printer are the same shape: an `Expandable` whose whole row opens its card.** **An
action is a row, never an icon button, and it cannot live in the head** — `Row` is a `Gtk.Button`,
so a button inside one is a button inside a button and the inner never emits `clicked`. Formatting
and sanitizing raw CUPS text is the panel applet's job, not this widget's.

In `PrivacyPopover`, **a row is one app, and only a row that can be stopped opens.** `Mute
microphone` is a `SwitchRow` under the list, not an action in an app's card, because it mutes the
input device rather than that one app. `Swatch` paints its `color` property and nothing else; a
color is data, so it is never a CSS class per value. `ColorList` takes finished `Shade`s, already
rendered by the applet, and reports ids and notation keys, never a format it would have to
understand. `WorkspaceNamePopover::set_name` writes the entry only while the user has not touched
it, or a compositor event arriving mid-edit would replace what is being typed.

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

## Rules

A widget moves here as soon as a second binary needs it: no copy-paste between panel and lock.
