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
