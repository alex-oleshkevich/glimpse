# glimpse-services

The service framework and every service implementation.

A service is one tokio task owning a set of topics and a set of commands. The runtime owns the
select loop; a service implements handlers that run serially on `&mut self` and never see a raw
connection.

## Contents

- `service.rs`, `context.rs`, `publisher.rs` — the framework
- `broker.rs` — `BrokerHandle`, the trait the daemon implements, with `MockBroker` beside it;
  `Responder` and the erased `Dispatch` a command travels through
- `services/` — one module per service; `tray/` will be a directory because it is the largest

## Rules

The dependency arrow points from `glimpsed` to here and never back. Anything the framework needs
from the daemon is a trait declared in this crate.

Mirror services (network, bluetooth, audio, battery, mpris, brightness) enumerate once then follow
change signals. The backend is right when they disagree, and no decision the backend already makes
gets reimplemented here.

A handler that can block moves its `Responder` into `ctx.spawn`. Handlers run serially, so one slow
D-Bus call otherwise freezes the whole service. A `Responder` that is dropped unanswered — queued
when the service stopped, lost to a panicking handler, or simply forgotten — answers `Unavailable`
from its `Drop` impl and logs, rather than leaving the caller to wait out its whole timeout with
nothing said anywhere.

Commands are declared the way topics are: `METHODS` lists the names, `decode` turns one plus its
JSON arguments into the service's own `Command` type, and the two must agree — a name in `METHODS`
that `decode` refuses is a command the broker will route and the service will then reject. The
default `decode` refuses everything, which is right for a service that declares no methods. A
command reaches the inbox through `ServiceSender::dispatch`, which offers rather than queues:
the caller is the broker, and the broker must never await.

Everything that reaches a handler arrives as an event from a **source**, and every source is one
`ctx` call returning a `SourceGuard` the service keeps for as long as it wants the source alive.
Dropping the guard is the whole cancellation story — it aborts the task or drops the subscription,
so there is no token to remember and no shutdown path to write.

| Source | Produces | For |
| ------------------- | ---------------- | -------------------------------------------------- |
| `ctx.spawn`         | one event        | one unit of async work whose result is an event    |
| `ctx.interval`      | an event a tick  | polling, clocks; `at_interval` picks the first tick |
| `ctx.stream`        | many events      | a backend signal stream, a watch, a subscription   |
| `ctx.subscribe::<T>` | many events     | another service's topic                            |

`spawn`, `interval` and `stream` each take an async closure receiving a `Ctx` of its own, so a task
reaches the buses, the publishers and `degraded` without any of them being threaded through its
arguments — `Ctx` is cheap to clone and its `degraded` flag is shared, so a task that degrades the
service is visible to the runtime. `stream`'s closure is async because building a source usually is:
a D-Bus signal stream has to be requested before it can be read. `ctx.events()` hands out the raw
sender and is for the one case the sources do not cover — a synchronous callback from a foreign
thread, as `notify`'s debouncer delivers.

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
