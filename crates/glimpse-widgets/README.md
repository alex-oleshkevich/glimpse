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

## Rules

A widget moves here as soon as a second binary needs it. Preventing copy-paste between the panel and
the lock screen is the entire reason this crate exists.

Widgets take values and emit signals. They do not know about topics, sockets or the daemon, which is
what lets one be built in a test with a literal value and no daemon behind it.
