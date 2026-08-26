# glimpse-contracts

The vocabulary both ends of the socket share: payload types, and the trait that binds each one to a
topic name.

## Contents

- `messages.rs` — `trait Message`, the `topic!` / `topics!` macros, and every topic payload
- `types.rs` — the component types payloads are built from: `SolarPhase`, `GeoCoordinates`,
  `ServiceState`, `TopicReport`

The split is payload against part. A type that is what a topic carries lives in `messages.rs` next
to the name that binds it; a type that only appears *inside* one lives in `types.rs`. Keeping the
payloads together is what makes the set of topics readable in one place.

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

Spec: [`specs/012_ipc.md`](../../specs/012_ipc.md)
