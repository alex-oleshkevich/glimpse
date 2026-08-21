---
paths:
  - "crates/glimpse-panel/**"
  - "crates/glimpse-widgets/**"
  - "crates/glimpse-wallpaper/**"
  - "crates/glimpse-lock/**"
  - "crates/glimpse-devtools/**"
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
  client, never knows a topic name, never reaches the daemon. That is what lets `glimpse-devtools` render it
  from a fixture file.
- A `Controller` that is not stored in the parent model is dropped, and its component silently stops
  receiving messages. Keep it.
- A widget moves into `glimpse-widgets` as soon as a second binary needs it.

## States

- Every widget renders four states: value, empty, `stale`, `degraded`. The daemon publishes the last
  two, and a widget that ignores them shows the user frozen data as if it were live.
- Data changes must not shift layout. A battery going 9% to 10%, a clock ticking, a track title
  changing — none of them may resize the bar. Reserve width, use tabular figures.

## Markup and styling

- Blueprint for structure, CSS for appearance, Rust for behaviour. No markup strings assembled in
  Rust, no colors hardcoded in Rust.
- Colors come from libadwaita semantic variables such as `@theme_fg_color` and `@accent_bg_color`.
  Literal hex breaks dark mode.
- No fixed `width-request` or `height-request` to force a layout. The panel must survive font
  scaling and fractional output scale.

## Untrusted text

Tray titles, notification summaries and bodies, MPRIS metadata and SSIDs come from other
applications. Cap length, ellipsize, and sanitize markup before any of it reaches a label.
