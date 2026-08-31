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
| other | `radius` `duration` `ease` `font-family` `disabled` |

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
