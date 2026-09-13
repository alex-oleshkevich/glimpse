---
name: service
description: Writing typed in-process services in glimpse-services: Service, Ctx sources, watch-backed handles, injected dependencies, configuration, health, and headless tests. D-Bus specifics belong to the zbus skill.
---

# service

A service is one Tokio task that owns a model and its backend integration. The runtime owns the
select loop; handlers run serially on `&mut self`. Services communicate inside one process through
typed handles and `tokio::sync::watch`, not through sockets, topics, or a broker.

**Verified against the tree at `crates/glimpse-services/`.** Every signature, macro shape and error
code below was read out of the current code, not recalled. When this file and the code disagree,
the code is right and this file is a bug — fix it in the same change.

## The shape, in one screen

```rust
pub struct Weather {
    state: Publisher<WeatherState>,
    location: LocationHandle,
}

#[derive(Clone)]
pub struct WeatherHandle(ServiceEndpoint<Weather>);

impl Service for Weather {
    const NAME: &'static str = "weather";
    type Config = Config;
    type State = WeatherState;
    type Handle = WeatherHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = Watch;

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle { WeatherHandle(endpoint) }
    fn subscriptions(&self) -> Vec<Sub<Self>> { ... }

    async fn start(ctx: &Ctx<Self>, config: Self::Config, deps: Self::Dependencies)
        -> Result<Self, ServiceError> { ... }
    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) { ... }
}
```

`ServiceRuntime::new(initial, buses, cancel)` returns `(runtime, handle)`. The handle gives a
complete current snapshot, a `watch::Receiver` for changes, a health receiver, and the service's
typed command methods. `ServiceEndpoint` is the small common implementation behind concrete
handles; consumers should never need a generic command or string key.

`Input<S>` is `Event(S::Event)`, `Command(S::Command)`, or `Config(S::Config)`. One inbox keeps
commands and resulting backend events ordered. Command variants carry their own typed
`oneshot::Sender<Result<Reply, CommandError>>`; a public handle creates the sender, submits the
typed command, and awaits the reply. A stopped service maps a dropped reply to `Unavailable`.

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

1. **State is typed and complete.** `State: Clone + PartialEq + Send + Sync + 'static` is the
   service's public snapshot. Publish with one `Publisher<State>`; `set` and `update` suppress
   unchanged values. Consumers receive the initial snapshot from `snapshot()` or `watch` before
   later changes.

2. **Dependencies are explicit.** Define a `Dependencies` struct containing required typed handles,
   and pass it to `start`. Build producers before consumers in the composition root. Do not add a
   service locator, registry, dependency container, string key, or generic broker replacement.

3. **Sources are declarative.** Long-lived backend streams, timers, and dependency watches belong
   in `subscriptions`. A `SubKey` restarts only the source whose inputs changed. `Sub::watch` emits
   the receiver's current value immediately, each changed value, and the explicit unavailable event
   supplied by the consumer if the producer stops.

4. **Health is orthogonal to state.** A missing backend or dependency is normally
   `ctx.degraded(reason)`, followed by whatever safe current state the service can provide.
   `ctx.running()` clears a previous degraded state. A stopped service's state remains readable,
   while its health says `Stopped`.

5. **Keep backends authoritative.** Mirror system D-Bus and compositor state; commands are thin
   typed pass-throughs. Do not add a retry loop when the backend already reconnects or owns policy.
   Move slow command work into `ctx.spawn_detached` only when the caller does not need a reply; a
   command with a reply must preserve its typed sender until the backend call completes.

6. **Configuration is a typed slice.** A service config implements `From<&glimpse_config::Config>`
   or uses `NoConfig`. `PartialEq` lets the runtime send updates only when the service's own table
   changed.

7. **Keep process boundaries narrow.** Panel-local services stay in the panel process and expose
   Rust handles. A standalone provider exposes a typed zbus interface and proxy; do not tunnel the
   old JSON protocol through D-Bus. Any temporary legacy string/JSON adapter belongs in `glimpsed`
   and is deleted with that compatibility process.

## Definition of done

- Every service has a concrete cloneable handle with `snapshot`, `subscribe`, `health`, and typed
  command methods where needed. `ServiceRuntime::new` receives a complete initial state.
- State and command types are typed Rust models. Contract structs may remain in
  `glimpse-contracts` while they are useful domain values; their serialization derives do not make
  them wire topics.
- `type Config` implements `From<&glimpse_config::Config>`, or is `NoConfig`.
- Dependencies are visible in the composition root and long-lived sources are in `subscriptions`.
- No `unwrap`, `expect`, or blocking call in `start`, `handle`, or `subscriptions`.
- Strings taken off a backend are length-capped before publication.
- Tests run headless with an unavailable bus and fake handles; they prove snapshots, subscriptions,
  command results, failures, configuration, and shutdown without a broker or socket.
- `just verify` is clean — `just lint` runs `-D warnings`.
- The crate `README.md` says what changed, in the same commit.
