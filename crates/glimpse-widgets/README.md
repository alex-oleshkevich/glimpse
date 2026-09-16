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
tray pixmap all arrive through one setter. Sniffing a string for a leading slash guesses wrong on a
themed name containing one. `IndicatorSpec` holds a `gio::Icon` and so is not `Send`.

- **The icon sits in a `Gtk.Overlay`, and `overlay` is an emblem on its trailing corner** — the
  Windows-taskbar idiom, for a state the application's own icon does not carry. The *slot* follows
  the base icon's presence, so an indicator with neither reserves no space.
- **A badge hides the attention dot, and must not cancel attention itself.** Two marks for one fact
  is noise, so the dot yields; `indicator--attention` stays and is what colours the chip. Assert it
  from a clean spec — `set_attention` returns early on an unchanged flag.

`Pager` is a strip of `PagerItem`, the one indicator that is not an `IndicatorGroup`: a click per
slot over a list whose length changes.

- **An item takes no click of its own.** `GtkButton` restricts its gesture to the primary button,
  the one every applet's popover opens on; acting on a workspace happens in the popover.
- **One token drives both dimensions of the labels shape.** GTK4's `min-width`/`min-height` bound
  the *content* box and padding is added outside it, so padding belongs on `.pager-item__label`.
- **`PagerItem` deliberately has no `dispose`.** *Naming* a `Gtk.Button` template's root child makes
  `dispose_template()` unparent it twice; `Row` and `Notice` escape it by wrapping contents in an
  **unnamed** box, and a `gtk4::Widget` subclass owns no child and always needs the call.

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
  left-zone one `Edge::End`, and the slide direction *and* the chevron's `pan-*-symbolic` follow
  that pair — **and the strip's `halign` has to be that same zone edge**, or the strip grows from
  its anchor and shoves the chips already on the bar aside. Closed, the chevron points back over the
  drawer and `--open` rotates it 180°; **the icon has to be directional**, since a rotated
  symmetrical glyph reads as nothing.
- **The strip does not author text.** `set_overflow_tooltip` takes the wording; the widget joins
  what it is given for the accessible name and invents none of it.

## TooltipCard

Icon, title, body and a status line, for a tray item's `ToolTip` — which is four fields, not a
string, and loses its icon and its title/body split the moment it is flattened into one.

- **It reports itself invisible when every field is empty**, so a host shows no tooltip rather than
  an empty box. An icon alone is still a tooltip; a card with only an icon reserves no text column.
- **Caps are the widget's, because the text is another application's**: 128 characters of title,
  512 and six lines of body. A sync log would otherwise grow the tooltip past the screen.
- Title, body, status and `icon-name` are GObject properties, which is what lets its states board be
  pure Blueprint.

## Calendar

- **Four measurements are tokens on `.calendar` itself.** The selection ring is not one: it is `2px`
  inside a `box-shadow`, and the pixel lint recognises `px` by property name.
- **Month names use `%OB`, not `%B`**, which is the form a date is built from. English does not
  distinguish them, which is what makes it easy to ship broken.
- **Dots are drawn, not styled**, and `measure` reports the same height with or without events.
- **`select` compares before it writes, and that guard is load-bearing** — it emits `day-selected`,
  so a handler that reacts by selecting overflows the stack without it.
- **Weekdays are numbered as `glib::DateTime` numbers them**, Monday 1 through Sunday 7. The letters
  come from January 2024, whose 1st was a Monday, so `%a` gives the locale's own abbreviations.

## Row, SplitRow and Placeholder

```
[ check ] [ lead ] [ title    ]  ←space→  [ value ] [ spinner ] [ trail ]
                   [ subtitle ]
```

- **It navigates, it does not expand.** A popover's height is capped by the work area, so expanding
  row 15 of 20 grows it past the fold and hides the thing just revealed. Expanding is right only when
  the revealed content is one or two rows *and* the list cannot grow.
- **`icon-name` and `value` are properties; `lead` and `trail` stay slots.** Without properties a
  `.blp` can name the type and nothing else, and being separate widgets lets a row carry a value
  *and* a chevron.
- **`selectable` and `selected` are separate**, so a selectable row reserves the check column before
  anything is selected and selecting one shifts no label in the list.
- **`busy` spins where the value sits**; a word like "Connecting…" beside it says it twice.
- **Sizes are rule-scoped tokens** declared in `.row` itself; `:root` stays the shared vocabulary.
- **`.row` must reset `font-weight`.** libadwaita styles bare `button` bold and weight inherits, so
  every row would render bold — and the grammar distinguishes a selected row by weight.
- **`SwitchRow` is the toggle row.** Its body flips the knob and the knob's `notify::active` is the
  only emitter, so the row and the switch can never double each other.

`Placeholder` stands where content would be; its `error` flag only recolours the icon.

- **`SplitRow` wraps a `Row` rather than subclassing one**, or its trailing button lands inside the
  row's box where `Row` would have to know about it. Its hairline is a `Gtk.Separator`: the pixel
  lint allows `border:` but not `border-left:`.

## Section, EventList and WorldClock

- **Visibility toggle, not a `Gtk.Stack`** — a stack sizes to its largest page, so a placeholder
  reserves its height under a four-row agenda.
- **`when` arrives formatted; a `Zone` does not.** Derive in the widget when formatting destroys the
  derivation — `"00:47"` has thrown away that it is tomorrow there.
- **`EventList` defaults to inert, but the overflow row is exempt** — the flag would make it inert in
  exactly the case that puts it on screen.
- **`EventList` answers the tooltip, not the row.** GTK finds tooltips by picking and a
  non-activatable row is skipped, so the list maps the pointer's `y` onto row allocations.
- **`Zone::note` and `Zone::icon_name` travel together**, or a sun sits above "light rain". The icon
  carries no colour. A second line appears only when the date differs, compared in that instant's
  own timezone — pass a local `DateTime`, not a UTC one.
- **A zone that does not resolve reads `—`.** `g_time_zone_new_identifier` returns NULL where the
  older `g_time_zone_new` silently returns UTC — hence the `v2_68` feature.
- **Times use `tabular-nums`**: proportional digits give up to 17px of animated jitter.

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
separator between each pair.

- **A section and its hairline show and hide together**, the shell watching `notify::visible` on
  what is appended; hiding the section alone leaves a line floating against nothing.
- **`add_child` must ignore the widget's own template children**, guarded by `try_get().is_none()`:
  `init_template` adds them through `Gtk.Buildable` itself, so an unguarded override routes
  `hero_box` into `content_box` and panics.
- **The shell paints its own surface, draws no shadow and does not scroll.** A `Gtk.Popover`'s
  `contents` node already draws a shadow, and two radii show it at every corner; capping height
  belongs to whatever knows the anchor's work area, and keeping it out lets the shell be built in a
  test with no display.

Per-popover rules that are traps rather than taste:

- **A click handler is connected at build, never while dressing**, which stacks one per reconcile.
- **A switch driven from state never reports it back** — `set_dnd` and the bluetooth power switch
  raise a guard flag while writing and the handler returns early on it. Anything drawn from the same
  value is set *above* the guard: it stops a set being reported, not drawn.
- **`.column` carries the width floor, not `.popover-shell`**, and `same_rows` compares only what
  the list draws — a title changes on every keystroke.
- **Static wording lives in the template; wording the data decides lives in Rust.** A fixed label is
  `_("…")` in the `.blp` — a slot holding two is a `Gtk.Stack` of `$Placeholder` pages, while
  `NextEventPopover` captures its template title at `constructed` and restores it; a plural is
  `ngettext` with named `{placeholders}`, which `format!` cannot reorder.
- **A switch reconciled from a backend goes through `SwitchRow::set_active`**, which compares and
  silences the knob, so a published state never fires `toggled` back as a command.

**A detail unfolds in place, never beside the list.** `crate::drawer` builds the holder — a row with
its own `Gtk.Revealer` under it — and `BluetoothPopover` and `ForecastList` both reconcile into one,
so a card grows down instead of sideways off an output edge. The open row takes `.open`, the card
`.detail-card`, and a capped list ends in an overflow row. **What recedes follows the row the list
shows, not the id asked for**: a hidden section takes card and dimming with it.
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
- **`set_position` is ignored while the pointer is down**, or a player reporting once a second yanks
  the slider out from under a drag. The hold uses a capture-phase `EventControllerLegacy`, because a
  `GestureClick` there is *cancelled* the moment `Gtk.Range` claims the sequence. A drag emits one
  `seek`, at the end — one D-Bus call per motion event is not a design.
- **The step and page increments are set in Rust**, because `blueprint-compiler lint` rejects a
  `Gtk.Adjustment` carrying anything besides `lower`, `upper` and `value`. They are the arrow-key
  distances, so losing them kills keyboard seeking silently.
- **Artwork is a `Gtk.Image`**, the only one that can be told how big to be: `Gtk.Picture` reports
  the paintable's natural width, so a cover would set the popover's. It also centres a paintable at
  its own aspect ratio, so cover art arrives already square or the corners read as broken.
- **`Pixbuf::file_info` reads dimensions out of the header without decoding**, so an oversized
  `mpris:artUrl` is refused before anything expands it in memory. Scaling is by the shorter side,
  only downward, cropped from the middle; `cover()` is pure arithmetic.
- **A widget built before `Styles::install()` picks up none of it.** Order matters, rooting does not.

## Stylesheets

`Styles` owns the CSS providers for one process. `install()` registers them on the display **once**
and gives every provider libadwaita's concrete effective scheme — GTK treats a provider's `default`
as light, so an automatic request passed through would disagree with a dark application. Installing
twice stacks every rule; `load()` replaces content in place.

| Priority | Source | Holds |
| --- | --- | --- |
| `APPLICATION` | `styles/glimpse.css`, via `include_str!` | the token vocabulary and every component rule |
| `USER` | the theme's sheet for this surface | token redefinitions |
| `USER + 1` | the user's own `styles.css` | the last word |

The built-in is compiled in rather than installed, because `load()` points the theme provider at
**one** path: a component rule living in a theme is one the first second theme deletes. The shipped
`adwaita` theme is therefore empty, and that is the test.

**`parsing-error` does not see a bad token** — a `var()` naming nothing renders transparent with only
a `Gtk-WARNING` on stderr. Hence the two guards: every `var()` in the built-in carries a fallback,
and `theme::tests` lints the vocabulary. The rules it enforces are in `.claude/rules/ui.md`.

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
