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
values and emits `pressed(button)` and `scrolled(dx, dy)`; deciding what a press means belongs to
whoever owns it.

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

`set_items` takes the whole desired list and reconciles it, keyed on `IndicatorSpec::id`. Matching
on id is what keeps a value change to a property write instead of a rebuilt widget subtree, and it
is also a correctness requirement: the signal closure connected when an indicator is built captures
its id, so reusing a widget under a different id would report presses under the wrong one. Duplicate
ids are skipped rather than collapsed, because collapsing them orphans a parented widget.

Placement is one `Widget::insert_after` call, which both parents a new child and reorders an
existing one.

An empty group sets itself invisible rather than merely rendering nothing. A visible empty widget
still draws its own padding and still counts toward the enclosing section's spacing, which shows up
as a gap between its neighbours.

Orientation is settable because `Panel` flips between horizontal and vertical; spacing is not,
because nothing varies it.

## Stylesheets

`Styles` owns the CSS providers for one process. `install()` registers them on the display **once**,
and `load()` replaces their content in place — installing twice stacks every rule. Two providers, in
cascade order: the theme's sheet for this surface at `STYLE_PROVIDER_PRIORITY_USER`, and the user's
own `styles.css` one above it, so a drop-in always has the last word.

Each provider connects `parsing-error`, because GTK4's loaders return nothing and a malformed
stylesheet is otherwise indistinguishable from a selector that does not match. Sheets load by path
rather than by string: a theme's `panel.css` carries `@import url("base.css")`, and relative imports
resolve only against a provider that was given a location. A sheet that does not resolve loads the
empty string, which clears its provider.

`Styles` takes resolved paths rather than a theme name — locating them is `glimpse-config`'s job, and
keeping it that way is what lets this crate stay free of the configuration schema.

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
