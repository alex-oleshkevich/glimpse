# Testing a service

Everything runs headless: no socket, no bus, no compositor. `Buses::unavailable("no bus in tests")`
is what a service sees when it asks for a connection, which is also the `degraded` path worth
testing.

## The scaffolding

`crates/glimpse-services/src/testing.rs`, a `#[cfg(test)] mod` at the crate root:

| | |
| --- | --- |
| `Probe` | a service that does nothing; `Event = u8` so a test can tell which source delivered |
| `Ping` | a topic to publish into a subscriber under test, independent of any real contract |
| `probe() -> (Ctx<Probe>, Inbox)` | a context and its inbox |
| `wired_probe() -> (Ctx<Probe>, Inbox, Arc<MockBroker>)` | the same, keeping the broker |
| `event(&mut Inbox) -> Option<u8>` | pull the next event out of an `Input` |

`MockBroker` records `published()` and `health()`, keeps the sinks it is handed, and `deliver(topic,
data)` stands in for the broker's own fan-out so a subscriber can be exercised without one.

## Three levels

**Unit — a free function.** Config projections and validators are plain functions; test them
directly. `glimpse_config::Config` and its sections derive `Default` with public fields, so build a
document inline rather than parsing TOML (`toml` is not a dependency of this crate):

```rust
fn document(provider: ConfiguredProvider, latitude: Option<f64>, longitude: Option<f64>)
    -> glimpse_config::Config
{
    glimpse_config::Config {
        location: glimpse_config::Location { provider, latitude, longitude },
        ..Default::default()
    }
}
```

**Context — a source in isolation.** `probe()`, build the source, assert on the inbox.

**Runtime — the whole loop.** `ServiceRuntime::<S>::new(broker, buses, cancel)` with `sender()` to
queue input, then `run(config)` on a spawned task. This is the only level that covers `start`, the
reconcile after `handle`, panic isolation and the health reports:

```rust
let mock = Arc::new(MockBroker::default());
let broker: Arc<dyn BrokerHandle> = mock.clone();
let cancel = CancellationToken::new();
let mut runtime = ServiceRuntime::<S>::new(broker, Buses::unavailable("no bus in tests"), cancel.clone());

let sender = runtime.sender();
sender.send(Input::Config(...)).await.expect("queued");     // queue before running

let running = tokio::spawn(async move { runtime.run(config).await });
for _ in 0..8 { tokio::task::yield_now().await; }
cancel.cancel();
let _ = running.await;

assert!(mock.published().iter().any(|(topic, _)| topic == Something::NAME));
```

Queue input **before** `run`, then yield, then cancel. `#[tokio::test]` is current-thread, so a
spawned task runs only when the test yields.

## Declaration drift

`TOPICS`, `METHODS`, `decode` and the `topics!` / `commands!` blocks are four lists that must agree,
and the compiler checks none of it. `assert_declarations::<S>()` checks three of the four ways they
drift. **Every service gets this test — it is one line:**

```rust
#[test]
fn declared_topics_and_methods_exist() {
    crate::service::assert_declarations::<Weather>();
}
```

| Drift | Caught by | Symptom without the test |
| --- | --- | --- |
| A name in `TOPICS`/`METHODS` that no `topics!`/`commands!` entry defines | `ALL_TOPICS` / `ALL_COMMANDS` | a topic nothing can name; a command that routes nowhere |
| A name in `METHODS` with no arm in `decode` | calling `decode(name, Value::Null)` | the broker routes it, the service refuses it, the caller sees `UnknownCommand` |
| Two services declaring one name | **not** this test — a runtime `error!` from `Store::declare` | last declaration wins, silently, and the loser's publishes are attributed to the winner |
| An arm in `decode` for a name **not** in `METHODS` | nothing | dead code; unreachable, because the broker routes by `METHODS` alone |

Null args are the trick that makes the second row work: a missing arm answers `UnknownCommand`, while
an arm that exists and wants real arguments answers `InvalidArgs`. Only `UnknownCommand` fails.

Writing `T::NAME` rather than a string literal makes the first row a compile error instead, which is
why it is the convention — the test is what catches a literal somebody typed anyway, or a name that
was deleted from contracts.

The fourth row has no mechanism. You cannot enumerate match arms, and a stale arm is harmless beyond
being dead. Delete it when you notice it.

## Every test must be checked against broken code

A test that passes against a deliberately broken implementation proves nothing, and this is not
hypothetical — **two tests in this tree did exactly that** before being fixed. Break the thing the
test covers, confirm the test fails, restore. It costs one minute and it is the only evidence a test
is load-bearing.

### Trap 1 — asserting absence without yielding

```rust
for _ in 0..3 { live.reconcile(&ctx, vec![forever(Watch::First, 1)]); }
assert!(received.try_recv().is_err(), "the source was left running");   // passes even if rebuilt
```

A rebuilt source delivers its first item on the **next poll**. With no `await` between the reconcile
and the `try_recv`, the newly spawned task has never been polled, so the inbox is empty either way.
The mutation `live.clear()` — rebuild everything, every time — passed this test.

Fix: yield before asserting absence.

```rust
for _ in 0..3 { tokio::task::yield_now().await; }
assert!(received.try_recv().is_err(), "the source was left running");
```

**Any test asserting that nothing happened is suspect on a current-thread runtime unless it yields
first.** Absence proves nothing if nothing has been allowed to run.

### Trap 2 — a helper that routes through the code under test

```rust
fn manual(latitude: f64, longitude: f64) -> Config {
    Config { provider: Provider::Manual(coordinates(Some(latitude), Some(longitude))) }
}
```

`coordinates` is the function under test. Mutating `-90.0..=90.0` to `-90.0..90.0` made both sides of
the assertion `Manual(None)`, and the test named `the_edges_of_the_ranges_are_inside_them` passed.

Fix: build the expectation literally.

```rust
fn manual(latitude: f64, longitude: f64) -> Config {
    Config { provider: Provider::Manual(Some(GeoCoordinates { latitude, longitude })) }
}
```

**A test helper must never call the code under test to produce its expectation.**

## What is worth a test

- Every `decode` arm, plus a name the service does not declare (`UnknownCommand`) and a mistyped
  argument (`InvalidArgs`).
- The config projection: each variant, and every way a table can be malformed.
- Any validator, at its boundaries — inclusive edges are where `..` and `..=` diverge.
- A source's teardown, if the model can turn it off.
- The straggler case: an event queued before a model change, delivered after it.
- `degraded` for the backend-absent path — `Buses::unavailable` gives it to you for free.

## What is not

- That the standard library works.
- That a `Publisher` deduplicates — the framework has that test.
- That `reconcile` diffs — the framework has those four.
- A test whose only assertion is that a function was called.

## Naming

Sentences saying what must be true, as the tree already does:

```
a_geoclue_event_arriving_after_a_switch_to_manual_is_ignored
a_key_that_stays_is_not_rebuilt
set_interval_refuses_a_period_outside_the_supported_range
a_panicking_handler_stops_its_own_service
```

A doc comment above a test says why the case matters, not what the code does — the failure this
guards against, not the steps.
