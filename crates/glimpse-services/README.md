# glimpse-services

The service framework and every service implementation.

A service is one tokio task owning a set of topics and a set of commands. The runtime owns the
select loop; a service implements handlers that run serially on `&mut self` and never see a raw
connection.

## Contents

- `service.rs`, `ctx.rs`, `publisher.rs`, `lifecycle.rs` — the framework
- `broker_handle.rs`, `wayland_edge.rs` — traits the daemon implements, each with a mock
- `dbus/` — shared zbus helpers for property and signal streams
- `services/` — one module per service; `tray/` is a directory because it is the largest

## Rules

The dependency arrow points from `glimpsed` to here and never back. Anything the framework needs
from the daemon is a trait declared in this crate.

Mirror services (network, bluetooth, audio, battery, mpris, brightness) enumerate once then follow
change signals. The backend is right when they disagree, and no decision the backend already makes
gets reimplemented here.

A handler that can block moves its `Responder` into `ctx.spawn`. Handlers run serially, so one slow
D-Bus call otherwise freezes the whole service.

`just test-crate glimpse-services` runs every service against the mocks, with no display, no
session bus and no broker.

Spec: [`specs/001_architecture.md`](../../specs/001_architecture.md)
