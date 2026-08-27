---
name: service
description: Writing services in glimpse-services — the Service trait, Ctx sources, declarative subscriptions, topics and commands in glimpse-contracts, registration in glimpsed, and headless tests against MockBroker. Use for any new service, any change to an existing one under crates/glimpse-services/src/services/, and any change to the framework in service.rs, context.rs, subscription.rs or publisher.rs. Trigger on the location, not the wording — if the file is a service or the framework under it, this applies. D-Bus specifics belong to the zbus skill; this covers the shape a service takes around them.
---

# service

A service is one tokio task owning a set of topics and a set of commands. The runtime owns the
select loop. A service implements handlers that run serially on `&mut self` and never touch a
socket, a `wl_` object, or a bus it opened itself.

**Verified against the tree at `crates/glimpse-services/`.** Every signature, macro shape and error
code below was read out of the current code, not recalled. When this file and the code disagree,
the code is right and this file is a bug — fix it in the same change.

## The shape, in one screen

```rust
pub struct Weather {
    status: Publisher<WeatherStatus>,   // from ctx.publisher::<T>(), held for the service's life
    place: Option<GeoCoordinates>,      // model
}

impl Service for Weather {
    const NAME: &'static str = "weather";
    const TOPICS: &'static [&'static str] = &[WeatherStatus::NAME];
    const METHODS: &'static [&'static str] = &[WeatherRefresh::NAME];

    type Config = Config;               // From<&glimpse_config::Config>, or NoConfig
    type Command = Command;             // this service's own decoded commands
    type Event = Event;                 // everything a source delivers
    type SubKey = Watch;                // identity for declared sources

    fn decode(method: &str, args: Value) -> Result<Self::Command, CallError> { ... }
    fn subscriptions(&self) -> Vec<Sub<Self>> { ... }

    async fn start(ctx: &Ctx<Self>, config: Self::Config) -> Result<Self, ServiceError> { ... }
    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) { ... }
}
```

`Input<S>` is the one inbox: `Event(S::Event)`, `Command(S::Command, Responder)`,
`Config(S::Config)`. One channel means one order — a command and the event that follows it reach
the handler in the order they were produced, which two channels raced in a `select!` could not
promise.

## Decision table

| Task | Go to |
| --- | --- |
| Adding a service from nothing | `references/anatomy.md` → Adding a service |
| The `Service` trait or `Ctx` surface, method by method | `references/anatomy.md` |
| A source that should live only while the model says so | `references/subscriptions.md` |
| Per-entity children — players, devices, tray items | `references/subscriptions.md` → Dynamic children |
| Writing tests, or judging whether a test proves anything | `references/testing.md` |
| Something does not work and you want the symptom, not the theory | `references/pitfalls.md` |
| Cost: allocation, wakeups, inbox pressure, publish volume | `references/performance.md` |
| Anything touching a bus | the `zbus` skill |
| Handler rules, payload rules, boundaries | `.claude/rules/daemon.md` |

## Rules that are not already loaded

`.claude/rules/daemon.md` loads automatically for everything under `crates/glimpse-services/` and
carries the handler rules: serial `&mut self`, no `unwrap` or `expect`, no blocking call, no retry on
top of a backend that already retries, and long-lived sources declared rather than started. They are
not repeated here. Neither is capping hostile text off a backend — that is a critical constraint in
`AGENTS.md`, and the `zbus` skill covers the bus case. What follows is what none of those say.

1. **Degraded is running.** A missing bus, a refused request, a half-configured table — all are
   `ctx.degraded(reason)` and carry on. A service does not exit because its backend is absent, and
   `degraded` never marks its topics `stale`: `stale` means the producer is not running at all, not
   that it is running badly. A degraded service keeps publishing what it can, and those values are
   current.

2. **`start` may fail the service; `handle` may not.** `start` returns `Result<Self, ServiceError>`
   and a failure there stops the service before it runs. `handle` returns `()` deliberately — there
   is nothing the runtime could do with an error from it. A failure inside a handler is
   `ctx.degraded(reason)` or a logged line, never a panic and never an early exit.

3. **Declare before you publish.** `TOPICS` and `METHODS` are read while the service is still
   stopped, so a `get` on a declared topic answers "declared, no value" rather than "unknown" and a
   subscription pattern still matches it. A publish to a topic outside `TOPICS` is dropped and
   logged as an error.

4. **Take the publisher once, in `start`, and hold it.** `Publisher` remembers the last value it
   sent and drops one equal to it. Rebuilt per call it starts from no last value every time, which
   defeats the gate the whole topic design rests on.

## What the framework will not do for you

- **Topics are static.** `TOPICS` is `&'static [&'static str]`, and the broker drops a publish to a
  topic nothing declared (`store.rs:81`, logged as an error). There is no way to declare
  `tray.item.{id}.menu` today. A collection in one topic is the pattern that works — see
  `references/pitfalls.md` → A per-entity topic cannot be declared.
- **Nothing checks declarations at compile time.** `const { assert!(...) }` and an associated-const
  check both compile and never fire. One test per service closes it instead — see Definition of done
  and `references/testing.md` → Declaration drift.

## Definition of done

- Every topic the service publishes is in `TOPICS`; every command it answers is in `METHODS` **and**
  in `decode`. One test per service asserts it:
  `#[test] fn declared_topics_and_methods_exist() { assert_declarations::<Weather>(); }`
- Payload and command types live in `glimpse-contracts`, declared with `topics!` / `commands!`. No
  zbus, GTK or backend type reaches either crate.
- `type Config` implements `From<&glimpse_config::Config>`, or is `NoConfig`.
- Long-lived sources are in `subscriptions`, and each `SubKey` carries exactly what should restart it.
- No `unwrap`, `expect`, or blocking call in `start`, `handle`, `subscriptions` or `decode`.
- Strings taken off a backend are length-capped before publication.
- Tests run headless against `MockBroker` with no socket and no bus, and each one has been checked
  against a deliberately broken version of the code it covers.
- `just verify` is clean — `just lint` runs `-D warnings`.
- The crate `README.md` says what changed, in the same commit.
