# glimpse-widgets

Shared GTK4 widgets: GObject subclasses, Blueprint templates and the CSS they expect.

Used by the panel and the lock screen, and previewed by `glimpse-devtools`.

## Layout

- `src/<widget>/` — one directory per widget, `mod.rs` plus `imp.rs` for the GObject boilerplate
- `blueprints/` — `.blp` templates, compiled by `build.rs` through `glib-build-tools`

## Rules

A widget moves here as soon as a second binary needs it. Preventing copy-paste between the panel and
the lock screen is the entire reason this crate exists.

Widgets take values and emit signals. They do not know about topics, sockets or the daemon, which is
what lets `glimpse-devtools` render them from a fixture file.

Spec: [`specs/002_structure.md`](../../specs/002_structure.md)
