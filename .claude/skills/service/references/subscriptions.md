# Declarative subscriptions

`crates/glimpse-services/src/subscription.rs`

A source that should live as long as the model says so is **declared**, not started. `subscriptions`
returns what ought to be running given the service as it stands; the runtime diffs that against what
is running, after `start` and after every input.

```
handle(&mut self, …)  ──►  subscriptions(&self) ──►  diff by key ──►  build new / drop removed
                                                       └─ dropping a guard aborts the task and
                                                          releases any broker subscription
```

## Constructors

| | Mirrors | Restart behaviour |
| --- | --- | --- |
| `Sub::stream(key, source)` | `ctx.stream` | rebuilds by re-reading the backend — comes back current |
| `Sub::interval(key, period, on_tick)` | `ctx.interval` | restarts the timer from now |
| `Sub::topic::<T>(key, map)` | `ctx.subscribe` | **does not come back current** — see Hazards |

## `SubKey` is the whole design

A boxed closure has no equality, so the runtime cannot ask "is this the source I am already
running?" The key is the identity the closure cannot supply, and the contract is two lines:

> **Same key ⇒ same source ⇒ left running, untouched.**
> **Key gone ⇒ guard dropped. Key new ⇒ built.**

```rust
pub(crate) fn reconcile(&mut self, ctx: &Ctx<S>, declared: Vec<Sub<S>>) {
    let keys: HashSet<&S::SubKey> = declared.iter().map(|sub| &sub.key).collect();
    self.running.retain(|key, _| keys.contains(&key));       // removed: guard drops here
    for sub in declared {
        self.running.entry(sub.key).or_insert_with(|| (sub.start)(ctx));   // new keys only
    }
}
```

`subscriptions` runs after **every** input, and an unchanged key costs a hash lookup, not a restart.

### Choosing the key

**Whatever must force a restart belongs in the key; whatever must not must stay out.** Both failure
modes are real and both are quiet:

| Mistake | Symptom |
| --- | --- |
| Key too coarse — omits a parameter that changed | the command silently does nothing; the old source keeps running |
| Key too fine — holds something that moves per event | the source is torn down and rebuilt constantly; a D-Bus watch reconnects forever |

Worked examples in the tree:

```rust
// heartbeat: the period IS the restart trigger, so it is the key.
// set_interval becomes `self.period_ms = period_ms` and the timer restarts itself.
#[derive(PartialEq, Eq, Hash)]
pub struct Tick { period_ms: u64 }

// geolocation: `refresh` has no parameter to change, so the key carries a counter whose only
// job is to differ. Bumping `attempt` is what restarts the watch.
#[derive(PartialEq, Eq, Hash)]
pub enum Watch { Geoclue { attempt: u64 } }
```

The accuracy level geolocation requests is a `const` and stays **out** of the key — it can never
change, so putting it in would be noise.

## Dynamic children

This is what the mechanism is really for. A service owning per-entity sources keys on the entity:

```rust
#[derive(PartialEq, Eq, Hash)]
pub enum Watch { NameOwner, Player(String) }

fn subscriptions(&self) -> Vec<Sub<Self>> {
    let mut subs = vec![Sub::stream(Watch::NameOwner, name_owner_changes)];
    subs.extend(self.players.keys().map(|bus| {
        let name = bus.clone();
        Sub::stream(Watch::Player(bus.clone()), move |ctx| properties_changed(ctx, name))
    }));
    subs
}
```

A player quits, the handler removes it from `self.players`, its key stops appearing, its guard drops,
its match rule is released. **There is no teardown code** — no second map to keep in sync with
`self.players`, which is exactly the pair that drifts.

## Sub against Cmd

| | declared, diffed, lives as long as the model says | fires once, never re-declared |
| --- | --- | --- |
| | `Sub::stream` · `Sub::interval` · `Sub::topic` | `ctx.spawn` · `ctx.spawn_detached` |
| use | a signal stream, a tick, another service's topic | a slow command that moved its `Responder` into a task |

Both exist on purpose. A one-shot effect inside a command handler is **not** a subscription and must
not be re-declared on the next input.

## Hazards

**Teardown does not un-queue.** Dropping a guard stops a source producing *more* events; it does not
remove what the source already put in the inbox. An event queued before a model change is still
delivered after it. Guard the handler on the model, not on the guard's existence:

```rust
Input::Event(_) if !matches!(self.provider, Provider::Geoclue) => {}
```

Without that, a stale GeoClue fix publishes over manually configured coordinates and sticks. This was
a real bug; see `references/pitfalls.md`.

**Two declarations sharing a key** is a service bug — the second is dropped. Warned once, not per
input, because `reconcile` runs after every one.

**A key change is drop-then-build,** in that order, so there is a brief window with nothing running.
Fine for everything in the tree; worth knowing if a source is expensive to establish.

## Cost

`subscriptions` allocates a `Vec` and boxes one closure per declared source, after every input, and
throws away all but the new ones. For the handful of sources any service in the tree declares this is
noise. A service with dozens of dynamic children on a high-rate event stream is where it would start
to matter — measure before restructuring.
