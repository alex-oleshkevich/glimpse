# glimpse-services

The service framework and every service implementation.

A service is one tokio task owning a set of topics and a set of commands. The runtime owns the
select loop; a service implements handlers that run serially on `&mut self` and never see a raw
connection.

## Contents

- `service.rs`, `context.rs`, `publisher.rs` — the framework
- `broker.rs` — `BrokerHandle`, the trait the daemon implements, with `MockBroker` beside it
- `services/` — one module per service; `tray/` will be a directory because it is the largest

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

A service reaches D-Bus through `ctx.session_bus()` and `ctx.system_bus()`, never by opening a
connection of its own. Both return `Result<&zbus::Connection, &str>`: the daemon connects once
before any service starts, and the `Err` is why there is no connection. A service that needs a bus
and gets `Err` calls `ctx.degraded(...)` with that reason and carries on — a missing bus costs it
its backend, not its life, and `system.services` is where anyone finds out which.

`just test-crate glimpse-services` runs every service against the mocks, with no display, no
session bus and no broker. `Buses::unavailable("...")` is the no-bus case a test injects, the way
`MockBroker` is the no-broker one.

Spec: [`specs/001_architecture.md`](../../specs/001_architecture.md)
