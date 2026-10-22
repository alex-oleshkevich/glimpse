# The `Service` trait and `Ctx`

## `trait Service` — `crates/glimpse-services/src/service.rs`

| Item | Meaning |
| --- | --- |
| `const NAME: &'static str` | stable diagnostic name and health identity |
| `type Config: Clone + PartialEq + Send + 'static + for<'a> From<&'a glimpse_config::Config>` | its slice of the document |
| `type State: Clone + PartialEq + Send + Sync + 'static` | the complete current snapshot |
| `type Dependencies: Send + 'static` | required typed handles supplied by the composition root |
| `type Command: Send + 'static` | typed commands, each carrying its own reply sender |
| `type Event: Send + 'static` | everything a source delivers |
| `type SubKey: Eq + Hash + Send + 'static` | identity for a declared source |
| `fn initial_state(&Self::Config) -> Self::State` | complete snapshot before the service starts |
| `fn subscriptions(&self) -> Vec<Sub<Self>>` | sources that should be running; default empty |
| `async fn start(ctx, config, dependencies) -> Self` | build the model |
| `async fn handle(&mut self, ctx, input)` | the one handler; cannot fail |
| `async fn stop(self, ctx)` | default no-op; skipped after a panic |

`ServiceRuntime::new(config, buses, cancel)` creates the state and health watch cells, the inbox,
and the concrete handle. The composition root holds the runtime task and passes only typed handles
to consumers. `start` reports unavailable backends through health; handler failures are reported
through health or a command's typed reply, so `handle` returns `()`.

## `Ctx<S>` — `crates/glimpse-services/src/context.rs`

Cheap to clone; every field is owned, which is what lets a spawned task be handed a `Ctx` of its own
instead of a sender and a token threaded through its arguments.

| Method | Returns | For |
| --- | --- | --- |
| `publisher()` | `Publisher<S::State>` | take once in `start`, keep for life |
| `session_bus()` / `system_bus()` | `Result<&Connection, &str>` | the `Err` is why there is none |
| `spawn(FnOnce(Ctx) -> Future<Output = S::Event>)` | `SourceGuard` | one unit of work, one event |
| `spawn_detached(FnOnce(Ctx) -> Future<Output = ()>)` | `()` | work with nothing to report |
| `interval(period, Fn(Ctx) -> Future<S::Event>)` | `SourceGuard` | an event a tick |
| `stream(FnOnce(Ctx) -> Future<Output = Stream<S::Event>>)` | `SourceGuard` | a backend signal stream |
| `subscribe` through `Sub::watch(key, receiver, map, unavailable)` | `SourceGuard` | another service's typed state |
| `degraded(reason)` / `running()` | | health, both directions |
| `events()` | `mpsc::Sender<Input<S>>` | escape hatch; nothing uses it today |

`stream` is where every event-producing source actually delivers — `spawn` is a stream of one item
and `interval` a stream of ticks — so a closed inbox is answered in one place.

`SourceGuard` is `#[must_use]`. Dropping it aborts the task and releases its backend or dependency
watch. Written as a bare statement, `ctx.spawn(...)` drops the guard at the semicolon and aborts the
task before it runs, so long-lived sources belong in `subscriptions` and the runtime owns the guard.

A panic inside a source is caught, logged, and turned into `degraded` on the owning service. A source
is where a backend's own data gets parsed, which makes it both the likeliest place to panic and the
least visible: uncaught, the task would simply stop and the service would go on believing it still
had a source.

## `Publisher<P>` — the equality gate

`ctx.publisher()` in `start`, held for the service's lifetime. `set(value)` and `update(change)` drop
a value equal to the last one, so unchanged state produces no watch notification.

A publisher rebuilt per call defeats this by starting from no last value every time. Take it once.

`watch::Receiver::borrow()` gives a complete immediate snapshot; `changed()` waits for the next
different state. A handle method uses `self.call(stopped, |reply| Command::Refresh { reply }).await?`.
`call` creates the typed oneshot and maps a closed reply to `CommandError::Unavailable(stopped)`.

## Adding a service, end to end

### 1. State and commands

State and command types are ordinary Rust types. A shared model may still live in
`glimpse-dbus` while a provider decodes it off the bus, but serialization and string names are outside
the service framework. Commands carry typed arguments and a command-specific
`oneshot::Sender<Result<Reply, CommandError>>`.

```rust
#[derive(Clone, PartialEq)]
pub struct TidesState { pub current: Option<Reading> }

pub enum Command {
    Refresh { reply: oneshot::Sender<Result<(), CommandError>> },
}
```

The handle is the consumer API. It owns no state itself; `snapshot()` reads the current value,
`subscribe()` clones the watch receiver, and a method such as `refresh()` submits the typed command
and awaits its typed result.

### 2. The config table, if the service reads one

A service cannot read a table that does not exist in the schema. Four edits, all in
`crates/glimpse-config/src/schema/`:

```rust
// tides.rs — new
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Tides { pub units: Units }
```

then in `mod.rs`: `mod tides;`, `pub use tides::Tides;`, a `pub tides: Tides` field on
`Config`, and the matching line in its hand-written `Default` impl. `deny_unknown_fields` is what
makes a typo an error the user sees rather than a setting silently ignored.

Then regenerate both checked-in artifacts, or `just test` fails — `schema/mod.rs` asserts each one
matches the compiled-in types:

```bash
just gen-config-default    # -> data/config.default.toml
just gen-config-schema     # -> data/config.schema.json
just gen-config-commented  # -> data/config.commented.toml
```

### 3. The config slice

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    units: Units,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self { units: document.tides.units }
    }
}
```

Beside the slice, never on the service. A service that reads no configuration writes
`type Config = NoConfig;` and no impl — `()` will not do, because `From<&Config> for ()` puts a
foreign trait on a foreign type and the orphan rules refuse it.

`PartialEq` is what narrows a reload to the services whose own table moved: editing `[[panels]]`
cannot perturb the night light schedule, because that subtree is unchanged and its service never
hears about the reload at all.

### 4. The service — `glimpse-services/src/services/tides.rs`

```rust
pub enum Event { Fetched(Option<Reading>), Unavailable(String) }
pub enum Command {
    Refresh { reply: oneshot::Sender<Result<(), CommandError>> },
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch { Poll { units: Units } }

pub struct Dependencies { pub location: LocationHandle }

pub type TidesHandle = ServiceEndpoint<Tides>;

impl ServiceEndpoint<Tides> {
    pub async fn refresh(&self) -> Result<(), CommandError> {
        self.call("tides stopped before refreshing", |reply| Command::Refresh { reply }).await?
    }
}

impl Service for Tides {
    const NAME: &'static str = "tides";

    type Config = Config;
    type State = TidesState;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = Watch;

    fn initial_state(_: &Self::Config) -> Self::State { TidesState { current: None } }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        vec![Sub::interval(Watch::Poll { units: self.units }, POLL, fetch)]
    }

    async fn start(ctx: &Ctx<Self>, config: Self::Config, deps: Self::Dependencies)
        -> Self {
        Self { state: ctx.publisher(), location: deps.location, units: config.units }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Fetched(reading)) => { ctx.running(); self.publish(reading); }
            Input::Event(Event::Unavailable(reason)) => { ctx.degraded(reason); }
            Input::Config(config) => self.units = config.units,
            Input::Command(Command::Refresh { reply }) => { ...; let _ = reply.send(Ok(())); }
        }
    }
}
```

Slow backend calls move into a cancellable context task. A command that has a reply keeps its
oneshot sender until the backend operation finishes; serial handlers must remain responsive.

### 5. Compose it

Export the concrete service and handle from `services/mod.rs`, then create the runtime and handle in
the process that owns the service:

```rust
mod tides;
pub use tides::{Tides, TidesHandle};
```

```rust
let (mut location_runtime, location) = ServiceRuntime::new(
    location_config, buses.clone(), cancel.child_token());
let (mut tides_runtime, tides) = ServiceRuntime::new(
    tides_config, buses.clone(), cancel.child_token());
let deps = tides::Dependencies { location };
tokio::spawn(async move { location_runtime.run(()).await });
tokio::spawn(async move { tides_runtime.run(deps).await });
```

The composition root makes dependency order visible. A panel-local root passes handles directly to
applets; a standalone process exposes only the narrow state and command API that another process
actually needs over typed zbus. A temporary legacy string/JSON adapter, if required during
migration, is gone with the daemon that needed it.

### 6. Test it

See `references/testing.md`. Headless tests use an initial watch state, unavailable buses, and fake
handles; they need no broker, socket, compositor, or live D-Bus.

```rust
#[tokio::test]
async fn refresh_returns_a_typed_result() {
    let (mut runtime, handle) = ServiceRuntime::new(config, buses, cancel);
    let task = tokio::spawn(async move { runtime.run(deps).await });
    handle.refresh().await.expect("service is running");
    task.abort();
}
```
