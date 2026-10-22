# Troubleshooting, by symptom

## State never reaches a consumer

**The handle always shows the initial value.** Confirm the service retained the `Publisher<State>`
returned by `ctx.publisher()` and calls `set` or `update` after changing its model. The publisher
suppresses equal values by design, so check `PartialEq` if a changed field is being ignored.

**A dependent service misses the first value.** Use
`Sub::watch(key, dependency.subscribe(), map, unavailable)`.
It emits the receiver's current snapshot before waiting for `changed()`, while a manually written
stream can race its first read against the producer.

**A state value is visible but stale.** Health and state are separate: `ctx.degraded(reason)` marks
the producer unhealthy without making its last usable state disappear. Invalidate or replace state
only when the service's contract says the old value is unsafe.

## A typed command does not work

**The handle returns `Unavailable`.** `ServiceEndpoint::command` uses a bounded `try_send`; the
service is stopped or its inbox is full. Keep handlers non-blocking and map a dropped oneshot reply
to `CommandError::Unavailable`.

**The command reaches the service but no caller receives a result.** Every command variant must own
its command-specific oneshot sender, and every handler arm must send either `Ok` or a typed
`CommandError`, including backend failures.

**A command is slow and blocks state updates.** Awaiting is correct when ordering with the resulting
backend event matters. Otherwise move fire-and-forget work to a cancellable context task; do not
invent a generic command dispatcher.

## A service does not reconfigure

**Nothing happens on reload.** The projection produced an equal value. `S::Config: PartialEq` is what
narrows a reload to services whose own table moved — verify the field you edited is actually in the
slice.

**"inbox full, dropped a configuration update".** `reconfigure` offers rather than queues: awaiting
would park the one task that reloads every service behind whichever of them is wedged.

**A `manual`-style table is half-filled and the service guesses.** Validate in the `From` impl and
return the "unusable" variant, then `ctx.degraded` in the handler. Latitude zero off the coast of
Africa is not a location anyone configured.

## A source does not run, or will not stop

**It never starts.** Either the guard was dropped at the semicolon (`ctx.spawn(...)` as a bare
statement — `SourceGuard` is `#[must_use]` for this), or `subscriptions` does not name its key under
the current model.

**It will not restart when a parameter changes.** The parameter is not in the `SubKey`. Same key
means same source, left untouched.

**It restarts constantly.** Something that moves per event is in the `SubKey`.

**It stopped and the service still reports healthy.** A panic in a source is caught and turned into
`degraded`; inspect the service handle's health receiver for the reason. Uncaught, the task would
simply stop.

## An event arrives after it should be impossible

Dropping a `SourceGuard` stops a source producing *more* events. It does **not** remove what the
source already put in the inbox, and both events and configuration share one 128-deep channel with
nothing ordering them. So this is reachable:

```
inbox: [ Config(manual, 51.5/-0.1), Event::Located(52.2/21.0) ]   <- queued while still geoclue

handle Config  -> publishes 51.5/-0.1, provider = Manual, watch torn down
handle Located -> publishes 52.2/21.0                              <- stale, and it sticks
```

The same shape degrades a healthy service permanently, when the straggler is an `Unavailable`.

**Fix: guard on the model, not on the guard.**

```rust
Input::Event(_) if !matches!(self.provider, Provider::Geoclue) => {}
```

Every service whose sources depend on a mode needs this arm. It was a real bug in `geolocation`.

## A dependency behaves incorrectly

**The consumer starts with missing data.** Construct the producer runtime and handle first, pass that
handle in a typed `Dependencies` struct, and use `Sub::watch` so the consumer receives the producer's
initial snapshot. A dependency handle is not a global service lookup.

**A stale dependency event overwrites a newer decision.** Dropping a source stops future events, but
does not remove events already queued in the service inbox. Guard the handler on the current model,
especially after a configuration mode switch.

**A dependent service cycles back to its producer.** Keep the dependency graph acyclic and visible in
the composition root. If two services need each other's state, extract the smallest shared read-only
model or give one side an event boundary; do not add a registry or hidden resolver.

## A service takes the daemon down

It should not — the runtime catches a panicking handler, reports `Stopped` with the panic message,
and returns rather than unwinding into the daemon. `stop` is skipped, because unwinding past a
`&mut self` the handler was midway through mutating leaves state nobody can reason about.

**If it does:** something added `panic = "abort"` to a profile. Per-service panic isolation depends
on unwinding. Never add it.

## Compile problems

**`the trait bound `(): From<&Config>` is not satisfied`.** Use `NoConfig`. `impl From<&Config> for
()` puts a foreign trait on a foreign type and the orphan rules refuse it — that is the entire reason
`NoConfig` exists.

**`future cannot be sent between threads safely` around state.** `Service::State` must be
`Clone + PartialEq + Send + Sync + 'static`, because the runtime shares it through a Tokio watch
cell with every cloneable handle.

**`missing SubKey in implementation`.** Every service declares one; `NoConfig`-style services use
`type SubKey = ();`. Associated type defaults are still unstable, so there is no way to omit it.

**`very complex type used`.** Clippy runs `-D warnings`. Extract a `type` alias rather than allow it.

**A `Ctx` method wants `'static` and your stream borrows a proxy.** Build a `'static` proxy: in zbus
5, `PropertyStream<'a, T>` *owns* its `Proxy<'a>`, so a `'static` proxy yields a `'static` stream and
keeps the match rule alive by itself. No self-referential struct needed. See the `zbus` skill.
