# glimpse-contracts

The vocabulary both ends of the socket share: payload types, and the trait that binds each one to a
topic name.

## Contents

- `topics.rs` — `trait Message`, the `topic!` / `topics!` macros, and every topic payload
- `commands.rs` — `trait Command`, the `commands!` macro, and every command
- `types.rs` — the component types the other two are built from: `SolarPhase`, `GeoCoordinates`,
  `ServiceState`, `TopicReport`, `MethodReport`, `HeartbeatInterval`

The split is by direction first, then payload against part. State the daemon publishes is a topic;
something a client asks the daemon to do is a command; a type that only appears *inside* one of
those lives in `types.rs`. Keeping each set together is what makes the topics, and the commands,
readable in one place.

## Binding a name to a payload

`Message` is the whole interface: a topic name and the type that travels under it.

```rust
pub trait Message {
    const NAME: &'static str;
    type Payload: Serialize + DeserializeOwned + PartialEq + Send + 'static;
}
```

`topics!` declares the ordinary case — a struct of named fields — deriving everything and
implementing `Message` in one place, so a payload cannot exist without a topic name or gain one
that disagrees with its type:

```rust
topics! {
    #[name = "solar.status"]
    pub struct SolarStatus { phase: SolarPhase }
}
```

`topic!` is the primitive underneath: it implements `Message` and nothing else, and `topics!`
expands into it. Declare topics through `topics!` — reaching for `topic!` directly lets a payload
and its name drift apart, which is what these macros exist to prevent.

One shape, always a struct of named fields. A payload that is really just a map still gets a field
holding it, so `system.topics` is `{"topics": {…}}` rather than a bare object. That costs a level of
nesting and buys one way to declare a topic.

## Binding a name to a command

`Command` is the same idea one step over: a name, the arguments that travel to the daemon, and what
comes back. `commands!` declares one, and the trailing type is the result:

```rust
commands! {
    #[name = "heartbeat.set_interval"]
    pub struct HeartbeatSetInterval { period_ms: u64 } -> HeartbeatInterval;
}
```

A command that takes no arguments still declares an empty struct — `GeolocationRefresh {}` — so
that every command has one shape.

`type Args = Self`, the way a topic's `Payload` is — the command *is* its argument struct, so there
is no second type to keep in step with the name. A command that returns nothing declares `-> ()`,
which is `null` on the wire and prints as nothing.

## Rules

**Nothing here knows about transport.** No tokio, no zbus, no GTK, no `glimpse-ipc`. A payload is
serde and nothing else, which is what lets the daemon, four UI binaries and the SDK generators all
compile against it without dragging a socket implementation behind them.

**Payloads derive `PartialEq`.** That is the equality gate `Publisher::set` uses to stop a service
republishing a value that did not change — the reason a 200-step volume drag is not 200 frames.

**Payloads accept unknown fields**, which is the opposite of the configuration rule and deliberately
so: a newer daemon and an older client survive a version skew instead of failing to deserialize.

**No backend type reaches a payload.** A `zbus` value or a `gtk` type here could not be generated
for Python, TypeScript or Go, and this crate is the input those generators read.

`topics!` and `commands!` are each invoked once for the whole tree and emit `ALL_TOPICS` and
`ALL_COMMANDS` beside the types they generate. A second invocation of either is a duplicate
definition of that constant, which is what keeps every name in one block. `glimpse-services` checks
a service's `TOPICS` and `METHODS` against them in `assert_declarations`.
