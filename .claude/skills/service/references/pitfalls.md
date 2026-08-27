# Troubleshooting, by symptom

## A topic never reaches anyone

**`just ctl get` says "no service declares `x`".** The name is missing from `TOPICS`, or the
service was excluded by `--only` / `--without` — an excluded service is absent from
`system.services` rather than listed as failed.

**The log says "a service published a topic it never declared".** `Publisher` was built for a topic
outside `TOPICS`. The broker drops the publish (`store.rs:81`). `TOPICS` and the `Message::NAME`
you publish under must be the same string.

**Published once, then silence.** The equality gate. `Publisher::set` drops a value equal to the last
one it sent, and the broker's store drops one whose serialized form matches the current cell. Both
are working as designed — if the payload genuinely did not change, nothing should be sent. If it did
change and nothing arrived, check `PartialEq` on the payload actually distinguishes the field you
changed.

## A command does not work

**"no service declares `x`".** Missing from `METHODS`. The broker routes by that map alone.

**"`svc` does not answer `x`".** In `METHODS` but not handled in `decode`. Nothing makes these agree
at compile time — `const { assert!(...) }` and an associated-const trick were both tried and neither
fires; an unconditional `assert!(false)` survived a full build. `assert_declarations::<S>()` in one
test per service is what catches it instead; if that test is missing, add it. The `_ =>` arm in
`decode` is unreachable through the broker and exists to turn the drift into a clean error rather
than a non-exhaustive match.

**A command answers, but the wrong service handled it.** Two services declared the same name and the
last one registered took it. `Store::declare` logs `two services declare one method` at `error` —
grep the daemon log. Topic names collide the same way.

**`InvalidArgs`.** `decode_args` rejected the payload. Wire payloads accept *unknown* fields on
purpose; what this catches is a missing or mistyped one.

**`Unavailable`, and the service looks fine.** `dispatch` is a `try_send` — the broker must never
await. A full 128-deep inbox means the service is not keeping up, usually because a handler is
blocking.

**The caller times out with nothing in the log.** A `Responder` dropped unanswered logs and replies
`Unavailable` from its `Drop` impl, so silence means the command never reached the service at all.

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
`degraded`; check `system.services` for the reason. Uncaught, the task would simply stop.

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

## A per-entity topic cannot be declared

`TOPICS` is `&'static [&'static str]`, and the broker drops a publish to anything it does not hold an
owner for. There is no way to declare `tray.item.{id}.menu` or `mpris.player.{bus}.metadata` today.

**What works now:** one declared topic holding a collection, which is what the broker's own
`system.topics` does with `BTreeMap<String, TopicReport>`.

```rust
topics! {
    #[name = "mpris.players"]
    pub struct MprisPlayers { players: BTreeMap<String, Player> }
}
```

The cost is honest and worth stating: one player's change republishes the whole map, and a subscriber
cannot watch a single player. Pattern subscription (`*` for one segment, `**` for trailing segments)
exists in `glimpse-ipc/src/pattern.rs` and would make per-entity topics genuinely useful — but the
declaration side has to grow first. Do not work around it by publishing undeclared topics; the broker
drops them and logs an error.

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

**`future cannot be sent between threads safely` around a payload.** `Message::Payload` is bound
`Clone + Serialize + DeserializeOwned + PartialEq + Send + Sync + 'static`. `Sync` is there because a
subscriber reads the payload out of a `watch` cell shared with the broker.

**`missing SubKey in implementation`.** Every service declares one; `NoConfig`-style services use
`type SubKey = ();`. Associated type defaults are still unstable, so there is no way to omit it.

**`very complex type used`.** Clippy runs `-D warnings`. Extract a `type` alias rather than allow it.

**A `Ctx` method wants `'static` and your stream borrows a proxy.** Build a `'static` proxy: in zbus
5, `PropertyStream<'a, T>` *owns* its `Proxy<'a>`, so a `'static` proxy yields a `'static` stream and
keeps the match rule alive by itself. No self-referential struct needed. See the `zbus` skill.
