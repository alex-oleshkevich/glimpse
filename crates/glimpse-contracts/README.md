# glimpse-contracts

The vocabulary both ends of the socket share: payload types, and the trait that binds each one to a
topic name.

## Contents

- `topics.rs` — `trait Message`, the `topic!` / `topics!` macros, and every topic payload
- `commands.rs` — `trait Command`, the `commands!` macro, and every command
- `types.rs` — the component types the other two are built from: `SolarPhase`, `GeoCoordinates`,
  `ServiceState`, `TopicReport`, `MethodReport`, `HeartbeatInterval`, `CalendarEvent`

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

## An enum variant is not a field

Payloads accept unknown *fields*, which is what carries a newer client past an older daemon. An
unrecognised *variant* of an enum is a different matter: it fails the whole payload rather than one
key. `Condition` is therefore internally tagged, `#[serde(tag = "condition")]`, so that
`#[serde(other)]` is available and an unfamiliar condition decodes as `Unknown` — serde offers
`other` only on a tagged enum, never on a bare unit-only one. Any wire enum that a later version may
grow needs the same shape; one that cannot grow, like `UnitSystem`, does not.

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

`chrono` is the one exception, and it is not a backend type: `DateTime<Utc>` serializes as an
RFC 3339 string, which every generator already has a date type for, and both ends of the socket
were converting timestamps by hand without it. It arrived with `CalendarEvent`.

**`calendar.set_range` decides what `calendar.events` holds.** The topic is not a fixed window the
daemon picks; it is whatever range was last asked for, and a surface that shows a month asks for
that month and the one after it. The clock applet does exactly that, on start and on every month
step, so browsing a year out costs one command and no fetching — the daemon keeps the parsed
calendars and re-expands them. Until something asks, the daemon publishes a near window around now,
so a client that never sends the command still gets a working topic. The ask is clipped rather than
refused: an end before its start is an empty window, and anything wider than 400 days is cut to 400,
because a client asking for a century would have the daemon expand every recurrence rule to answer.

**`CalendarEvents.truncated_from` is where the list stops being complete.** The merged list is
capped at 512 events, and truncating by count after sorting by start silently narrows the window a
surface believes it has — measured, a directory of 2020 occurrences over a 69-day window published
10 days of it and said nothing, so a busy calendar rendered an empty October that looked like a
panel bug. The field carries the start of the first entry that was dropped, so everything before it
is known complete and a surface can say "nothing loaded beyond here" rather than "nothing here".
`None` means the whole requested range is present, which it now usually is: the cap is reached only
by a genuinely crowded range, not by asking for one wider than the daemon fetches.

**`CalendarEvent.end` is the last instant the entry covers, not iCalendar's exclusive bound.** A
single all-day entry runs from local midnight to a second before the next one, so `end.date_naive()`
answers "which day is this on" and a surface renders a multi-day entry without knowing what a
`DTEND` is. `start` and `end` are always UTC; the daemon and every UI binary share a machine, so
converting to local on arrival round-trips exactly. `color` is the hex string the source was
configured with, carried per event because a UI binary reads only the tables it owns and `[calendar]`
is not one of them.

`topics!` and `commands!` are each invoked once for the whole tree and emit `ALL_TOPICS` and
`ALL_COMMANDS` beside the types they generate. A second invocation of either is a duplicate
definition of that constant, which is what keeps every name in one block. `glimpse-services` checks
a service's `TOPICS` and `METHODS` against them in `assert_declarations`.
