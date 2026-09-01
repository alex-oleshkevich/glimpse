---
paths:
  - "crates/glimpse-panel/**"
  - "crates/glimpse-widgets/**"
  - "crates/glimpse-wallpaper/**"
  - "crates/glimpse-lock/**"
---

# UI conventions

General GTK4, libadwaita and relm4 craft is covered by the `relm4`, `gtk4-styles` and
`libadwaita-styles` skills. What follows is specific to this project.

## Threading

- Never block the GTK main thread. Every value reaches the UI through a channel fed by a tokio
  task; nothing in a widget calls the socket, the filesystem, or D-Bus directly.
- No `glib::timeout_add` to refresh a widget from daemon data. Values arrive as topic events;
  polling recreates the split-brain the architecture exists to prevent. Timers are for animation.

## Widget boundaries

- A widget in `glimpse-widgets` takes values and emits signals. It never holds a `glimpse-ipc`
  client, never knows a topic name, never reaches the daemon. That is what lets it be built in a
  test with a literal value and no daemon behind it.
- A `Controller` that is not stored in the parent model is dropped, and its component silently stops
  receiving messages. Keep it.
- A widget moves into `glimpse-widgets` as soon as a second binary needs it.

## States

- A widget renders what it is given, and empty when it is given nothing. There is no `stale` or
  `degraded` rendering: a dead daemon stops sending events and the last value stays on screen.
- Data changes must not shift layout. A battery going 9% to 10%, a clock ticking, a track title
  changing — none of them may resize the bar. Reserve width, use tabular figures.

## Markup and styling

- Blueprint for structure, CSS for appearance, Rust for behaviour. No markup strings assembled in
  Rust, no colors hardcoded in Rust.
- Colors come from the `--gl-*` tokens declared in `glimpse-widgets/styles/glimpse.css`, never from
  a literal and never from a libadwaita token directly. `@theme_fg_color` and `@accent_bg_color` are
  GTK's pre-4.16 namespace, are not what a theme can override, and `@theme_fg_color` resolves to the
  *window's* foreground rather than the panel's. `theme::tests` fails the build on either mistake.
- Font sizes come from `--gl-text-caption` / `--gl-text-body` / `--gl-text-title`, never from a `px`
  literal. A `px` size does not move when the user scales their text, so the shell keeps its own
  size on a desktop that has doubled. The three tokens are `rem`, which follows `gtk-font-name`.
- **Every length that belongs to the type is `rem` too** — padding, margins, `min-width`,
  `min-height`, `border-radius`, `-gtk-icon-size`. `px` survives only for a hairline, a border, an
  outline and a `999px` pill, none of which are proportional to anything. GTK and libadwaita use
  `px` throughout and it is right for HiDPI, because a GTK pixel is a logical one already multiplied
  by the output scale — but it does not follow *text* scaling, so a layout in `px` keeps its 8px
  padding while the type inside it doubles. `theme::tests` fails the build on either mistake.
- A misspelled token is invisible: GTK renders the surface transparent, and `parsing-error` does not
  fire. Give every `var()` in the built-in stylesheet a fallback.
- No fixed `width-request` or `height-request` to force a layout. The panel must survive font
  scaling and fractional output scale.

## Untrusted text

Tray titles, notification summaries and bodies, MPRIS metadata and SSIDs come from other
applications. Cap length, ellipsize, and sanitize markup before any of it reaches a label.
