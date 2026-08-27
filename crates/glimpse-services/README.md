# glimpse-services

The service framework and every service implementation.

A service is one tokio task owning a set of topics and a set of commands. The runtime owns the
select loop; a service implements handlers that run serially on `&mut self` and never see a raw
connection.

## Contents

- `service.rs`, `context.rs`, `subscription.rs`, `publisher.rs` — the framework
- `broker.rs` — `BrokerHandle`, the trait the daemon implements, with `MockBroker` beside it;
  `Responder` and the erased `Dispatch` a command travels through
- `services/` — one module per service; `tray/` will be a directory because it is the largest

## The geolocation service

Two providers behind one topic. `[location] provider = "manual"` publishes the configured pair
directly; `"geoclue"` follows GeoClue's `Location` property. Either way `geolocation.status` is the
only thing downstream services see, which is what lets `solar` subscribe to it without knowing a
provider exists.

Three details are load-bearing:

- **The GeoClue watch is subscribed before `Start`**, because the first fix can arrive before that
  call returns.
- **`GCLUE_ACCURACY_LEVEL_CITY`, not exact.** Sunrise, sunset and weather are everything downstream
  of this service, and none of them is sharper than a city.
- **Authorization is a shipped file, not code.** `data/geoclue/conf.d/glimpse.conf` is what stops
  GeoClue deferring to an agent that either is not running or has nobody to answer it. Its section
  name and `DESKTOP_ID` must agree.

A missing fix, a refused request or a `manual` table without coordinates all leave the service
`degraded` and publishing `None` — running, and honest about having nothing.

## Rules

The dependency arrow points from `glimpsed` to here and never back. Anything the framework needs
from the daemon is a trait declared in this crate.

Mirror services (network, bluetooth, audio, battery, mpris, brightness) enumerate once then follow
change signals. The backend is right when they disagree, and no decision the backend already makes
gets reimplemented here.

A handler that can block moves its `Responder` into `ctx.spawn`. Handlers run serially, so one slow
D-Bus call otherwise freezes the whole service. Such a task usually returns the event saying the
command finished, which the handler wants anyway; one with nothing to report uses
`ctx.spawn_detached` rather than inventing an event for the handler to ignore.

A `Responder` that is dropped unanswered — queued when the service stopped, lost to a panicking
handler, or simply forgotten — answers `Unavailable` from its `Drop` impl and logs, rather than
leaving the caller to wait out its whole timeout with nothing said anywhere.

Commands are declared the way topics are: `METHODS` lists the names, `decode` turns one plus its
JSON arguments into the service's own `Command` type, and the two must agree — a name in `METHODS`
that `decode` refuses is a command the broker will route and the service will then reject. The
default `decode` refuses everything, which is right for a service that declares no methods. A
command reaches the inbox through `ServiceSender::dispatch`, which offers rather than queues:
the caller is the broker, and the broker must never await.

Everything that reaches a handler arrives as an event from a **source**, and every source is one
`ctx` call returning a `SourceGuard`. Dropping the guard is the whole cancellation story — it aborts
the task or drops the subscription, so there is no token to remember and no shutdown path to write.

| Source | Produces | For |
| -------------------- | ---------------- | --------------------------------------------------- |
| `ctx.spawn`          | one event        | one unit of async work whose result is an event     |
| `ctx.spawn_detached` | nothing          | work with no result to report — see below           |
| `ctx.interval`       | an event a tick  | polling, clocks; `at_interval` picks the first tick |
| `ctx.stream`         | many events      | a backend signal stream, a watch, a subscription    |
| `ctx.subscribe::<T>` | many events      | another service's topic                             |

`SourceGuard` is `#[must_use]`, because `ctx.spawn(...)` written as a statement drops the guard at
the semicolon and aborts the task before it runs — a call that looks right and does nothing.

A panic inside a source is caught, logged and turned into `degraded` on the owning service. A source
is where the backend's own data gets parsed, which makes it both the likeliest place to panic and
the least visible: uncaught, the task stops and the service goes on believing it still has a source.

`spawn`, `interval` and `stream` each take an async closure receiving a `Ctx` of its own, so a task
reaches the buses, the publishers and `degraded` without any of them being threaded through its
arguments — `Ctx` is cheap to clone and its `degraded` flag is shared, so a task that degrades the
service is visible to the runtime. `stream`'s closure is async because building a source usually is:
a D-Bus signal stream has to be requested before it can be read.

`stream` is also the one that does the delivering: `spawn` is a stream of one item and `interval` a
stream of ticks, so a closed inbox is answered in a single place rather than once per constructor.
`subscribe` is the exception, because it has a broker subscription to release as well as a task to
abort. Its sink parks the newest payload in a `tokio::sync::watch` cell and a pump task delivers it
— the broker is called from its own task and must never be made to wait, and newest-wins is what a
bounded channel cannot give, since a full one drops whatever it is handed, which is always the
newest.

## Subscriptions

A source that should live as long as the service says so is **declared**, not started. `subscriptions`
returns what ought to be running, given the service as it stands, and the runtime diffs that against
what is running after `start` and after every input:

```rust
type SubKey = Watch;

fn subscriptions(&self) -> Vec<Sub<Self>> {
    match self.provider {
        Provider::Geoclue => vec![Sub::stream(Watch::Geoclue { attempt: self.attempt }, geoclue)],
        Provider::Manual(_) => Vec::new(),
    }
}
```

`Sub::stream`, `Sub::interval` and `Sub::topic::<T>` mirror the `ctx` constructors above; the runtime
calls one only for a key it is not already running. Switching geolocation to `manual` releases GeoClue
because the key stops being named, not because a handler remembered to drop a guard.

`SubKey` is the identity a boxed closure cannot supply, and the whole discipline follows from how it
is chosen: **whatever must force a restart belongs in the key, and whatever must not must stay out.**
Heartbeat keys its timer on `period_ms`, so `heartbeat.set_interval` restarts it by assigning a field.
Geolocation keys on an `attempt` counter that carries nothing but its own difference, because
`geolocation.refresh` has no parameter to change and an unmoved key would leave the watch running.
A key too coarse silently ignores a change; a key holding something that moves per event silently
rebuilds the source every time. Both fail quietly, which is why the key is worth choosing deliberately.
Two declarations sharing a key is a bug — the second is dropped, warned about once.

The two kinds of source do not survive a restart alike. `Sub::stream` rebuilds by re-reading its
backend, so it comes back current. `Sub::topic` does not: the broker delivers a topic to a new sink
only on the next publish, and a publisher's equality gate means an unchanged value is never
republished — so a resubscribed topic can sit blank indefinitely. Key a topic subscription on
something constant unless the service can live with that gap.

This is `Sub` against `Cmd`, and the split is the same one Elm draws: `subscriptions` is for sources
whose lifetime the model decides, and `ctx.spawn` / `ctx.spawn_detached` for an effect that fires once
and is never re-declared — a slow command that moved its `Responder` into a task. A service with no
declared sources writes `type SubKey = ();` and inherits the empty default, since associated type
defaults are still unstable.

`subscriptions` runs inside the same `catch_unwind` as the handler, so a panic while declaring stops
that one service rather than the runtime loop.

A service declares `type Config` and receives it as `Input::Config`. The projection from the whole
document down to that slice is `From<&glimpse_config::Config>`, implemented beside the slice rather
than on the service, so whatever it validates stays private to the module:

```rust
impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self { ... }
}
```

A service that reads no configuration writes `type Config = NoConfig;` and no impl. `()` will not do
— `From<&Config> for ()` is a foreign trait on a foreign type and the orphan rules refuse it, which
is the whole reason `NoConfig` exists. `S::Config: PartialEq` is what narrows a reload to the
services whose own table moved.

Events, commands and configuration all arrive on **one** inbox. One channel means one order: a
command and the event that follows it reach the handler in the order they were produced, which two
channels raced against each other in a `select!` could not promise. The cost is a shared budget —
a service flooding its own inbox with events makes `dispatch` refuse commands with `Unavailable`,
which is the honest answer but a coarse one.

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
