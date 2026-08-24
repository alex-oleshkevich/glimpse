# glimpse-services

The service framework and every service implementation.

A service is one tokio task owning a set of topics and a set of commands. The runtime owns the
select loop; a service implements handlers that run serially on `&mut self` and never see a raw
connection.

## Contents

- `service.rs`, `context.rs`, `publisher.rs`, `lifecycle.rs` — the framework
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

A service publishes through a `Publisher` it takes from `ctx.publisher::<T>()` in `start` and keeps
for its lifetime. The publisher holds the last value it sent and drops a `set` that matches it, so
an unchanged payload is never serialized and never reaches the broker — this is the equality gate
the whole topic design rests on, and a publisher rebuilt per call would defeat it by starting from
no last value every time. `seq`, `ts` and `stale` are the broker's to assign; a publisher hands over
a topic name and a value and knows nothing about any of the three.

`just test-crate glimpse-services` runs every service against the mocks, with no display, no
session bus and no broker.

Spec: [`specs/001_architecture.md`](../../specs/001_architecture.md)
