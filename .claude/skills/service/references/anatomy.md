# The `Service` trait and `Ctx`

## `trait Service` — `crates/glimpse-services/src/service.rs`

| Item | Meaning |
| --- | --- |
| `const NAME: &'static str` | identifies the service in `system.services` and owns its topics |
| `const TOPICS: &'static [&'static str]` | every topic it may publish, declared **before it starts** |
| `const METHODS: &'static [&'static str] = &[]` | every command it answers; default is none |
| `type Config: Clone + PartialEq + Send + 'static + for<'a> From<&'a glimpse_config::Config>` | its slice of the document |
| `type Command: Send + 'static` | the decoded command |
| `type Event: Send + 'static` | everything a source delivers |
| `type SubKey: Eq + Hash + Send + 'static` | identity for a declared source |
| `fn decode(method, args) -> Result<Self::Command, CallError>` | wire call → command; default refuses everything |
| `fn subscriptions(&self) -> Vec<Sub<Self>>` | sources that should be running; default empty |
| `async fn start(ctx, config) -> Result<Self, ServiceError>` | build the model |
| `async fn handle(&mut self, ctx, input)` | the one handler; cannot fail |
| `async fn stop(self, ctx)` | default no-op; skipped after a panic |

`TOPICS` and `METHODS` are declared before the service starts because the broker needs the mapping
while it is still stopped: a `get` on a declared topic must answer "declared, no value" rather than
"unknown", and a subscription pattern has to match it.

`handle` returning `()` is deliberate. `start` may fail the service; a handler may not. A failure
inside one is a `degraded` or a logged line, not an error the runtime can act on.

## `Ctx<S>` — `crates/glimpse-services/src/context.rs`

Cheap to clone; every field is owned, which is what lets a spawned task be handed a `Ctx` of its own
instead of a sender and a token threaded through its arguments.

| Method | Returns | For |
| --- | --- | --- |
| `publisher::<T: Message>()` | `Publisher<T::Payload>` | take once in `start`, keep for life |
| `session_bus()` / `system_bus()` | `Result<&Connection, &str>` | the `Err` is why there is none |
| `spawn(FnOnce(Ctx) -> Future<Output = S::Event>)` | `SourceGuard` | one unit of work, one event |
| `spawn_detached(FnOnce(Ctx) -> Future<Output = ()>)` | `SourceGuard` | work with nothing to report |
| `interval(period, Fn(Ctx) -> Future<S::Event>)` | `SourceGuard` | an event a tick |
| `at_interval(start, period, ...)` | `SourceGuard` | the same, from a chosen instant |
| `stream(FnOnce(Ctx) -> Future<Output = Stream<S::Event>>)` | `SourceGuard` | a backend signal stream |
| `subscribe::<T: Message>(Fn(T::Payload) -> S::Event)` | `SourceGuard` | another service's topic |
| `degraded(reason)` / `running()` | | health, both directions |
| `events()` | `mpsc::Sender<Input<S>>` | escape hatch; nothing uses it today |

`stream` is where every event-producing source actually delivers — `spawn` is a stream of one item
and `interval` a stream of ticks — so a closed inbox is answered in one place.

`SourceGuard` is `#[must_use]`. Dropping it aborts the task and releases any broker subscription
behind it. Written as a bare statement, `ctx.spawn(...)` drops the guard at the semicolon and aborts
the task before it runs: a call that looks right and does nothing. In practice you rarely hold one —
declare the source in `subscriptions` and let the runtime own the guard.

A panic inside a source is caught, logged, and turned into `degraded` on the owning service. A source
is where a backend's own data gets parsed, which makes it both the likeliest place to panic and the
least visible: uncaught, the task would simply stop and the service would go on believing it still
had a source.

## `Publisher<P>` — the equality gate

`ctx.publisher::<T>()` in `start`, held for the service's lifetime. `set(value)` drops a value equal
to the last one it sent, so an unchanged payload is never serialized and never reaches the broker.

A publisher rebuilt per call defeats this by starting from no last value every time. Take it once.

`seq`, `ts` and `stale` belong to the broker. A publisher hands over a name and a value.

## `Responder` — answering a command

`ok<T: Serialize>(self, output)` or `fail(self, CallError)`, both consuming. Dropped unanswered — 
queued when the service stopped, lost to a panicking handler, or simply forgotten — it answers
`Unavailable` from its `Drop` impl and logs, rather than leaving the caller to wait out its timeout
with nothing said anywhere.

## Adding a service, end to end

### 1. Payloads and commands — `glimpse-contracts`

```rust
// src/topics.rs
topics! {
    #[name = "weather.status"]
    pub struct WeatherStatus { temperature_c: Option<f64>, condition: Option<String> }
}

// src/commands.rs
commands! {
    #[name = "weather.refresh"]
    pub struct WeatherRefresh {} -> ();
}
```

The macros derive `Debug, Clone, PartialEq, Serialize, Deserialize` and bind the name to the type
through `trait Message` / `trait Command`. Topics are `domain.name`, commands `domain.verb_object`,
both lower snake case with dots as separators.

Each macro is invoked **once** for the whole tree and emits `ALL_TOPICS` / `ALL_COMMANDS` alongside
the types — a second invocation is a duplicate definition of those, which is how they stay in one
block. `assert_declarations` checks a service's `TOPICS` and `METHODS` against them.

Wire payloads **accept** unknown fields, so a newer client and an older daemon survive version skew.
Config **rejects** them, so a typo is an error the user sees. These two rules point in opposite
directions on purpose.

### 2. The config table, if the service reads one

A service cannot read a table that does not exist in the schema. Four edits, all in
`crates/glimpse-config/src/schema/`:

```rust
// weather.rs — new
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Weather { pub units: Units }
```

then in `mod.rs`: `mod weather;`, `pub use weather::Weather;`, a `pub weather: Weather` field on
`Config`, and the matching line in its hand-written `Default` impl. `deny_unknown_fields` is what
makes a typo an error the user sees rather than a setting silently ignored.

Then regenerate both checked-in artifacts, or `just test` fails — `schema/mod.rs` asserts each one
matches the compiled-in types:

```bash
just gen-config-default    # -> data/config.default.toml
just gen-config-schema     # -> data/config.schema.json
```

### 3. The config slice

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    units: Units,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self { units: document.weather.units }
    }
}
```

Beside the slice, never on the service. A service that reads no configuration writes
`type Config = NoConfig;` and no impl — `()` will not do, because `From<&Config> for ()` puts a
foreign trait on a foreign type and the orphan rules refuse it.

`PartialEq` is what narrows a reload to the services whose own table moved: editing `[[panels]]`
cannot perturb the night light schedule, because that subtree is unchanged and its service never
hears about the reload at all.

### 4. The service — `glimpse-services/src/services/weather.rs`

```rust
pub enum Event { Fetched(Option<Reading>), Unavailable(String) }
pub enum Command { Refresh }

#[derive(PartialEq, Eq, Hash)]
pub enum Watch { Poll { units: Units } }

impl Service for Weather {
    const NAME: &'static str = "weather";
    const TOPICS: &'static [&'static str] = &[WeatherStatus::NAME];
    const METHODS: &'static [&'static str] = &[WeatherRefresh::NAME];

    type Config = Config;
    type Command = Command;
    type Event = Event;
    type SubKey = Watch;

    fn decode(method: &str, _args: Value) -> Result<Self::Command, CallError> {
        match method {
            WeatherRefresh::NAME => Ok(Command::Refresh),
            _ => Err(unknown_command(Self::NAME, method)),
        }
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        vec![Sub::interval(Watch::Poll { units: self.units }, POLL, fetch)]
    }

    async fn start(ctx: &Ctx<Self>, config: Self::Config) -> Result<Self, ServiceError> {
        Ok(Self { status: ctx.publisher::<WeatherStatus>(), units: config.units })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Fetched(reading)) => { ctx.running(); self.publish(reading); }
            Input::Event(Event::Unavailable(reason)) => { ctx.degraded(reason); }
            Input::Config(config) => self.units = config.units,
            Input::Command(Command::Refresh, responder) => { ...; responder.ok(()); }
        }
    }
}
```

Commands with arguments decode through `decode_args`, which accepts unknown fields and catches a
missing or mistyped one:

```rust
WeatherSetUnits::NAME => {
    let WeatherSetUnits { units } = decode_args(args)?;
    Ok(Command::SetUnits { units })
}
```

### 5. Register it

`services/mod.rs`:

```rust
mod weather;
pub use weather::Weather;
```

`glimpsed/src/main.rs`:

```rust
Daemon::new(filter)
    .register::<Weather>()
```

`register::<S>()` is the last place the concrete type is known, so it builds three things there: the
erased `Dispatch` handed to the broker inside the same `Declare` that carries `METHODS`, the
`ConfigSink` that projects with `S::Config::from(document)` and compares, and the service task.

### 6. Test it

`references/testing.md`. Headless, against `MockBroker`, no socket and no bus. One test is not
optional, because nothing else checks the declarations agree:

```rust
#[test]
fn declared_topics_and_methods_exist() {
    crate::service::assert_declarations::<Weather>();
}
```

### 7. Update `crates/glimpse-services/README.md`

In the same change. A stale README is worse than none, because it is believed.
