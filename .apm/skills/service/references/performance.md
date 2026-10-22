# Cost

These are long-lived session services on a laptop. The budget that matters is wakeups while nothing
is happening and work done on each service's ordered handler, not throughput.

## The two things that are actually expensive

**A wakeup on an idle system.** A timer that fires when nothing changed costs battery for nothing,
and it costs it forever. One polling thread in this tree once produced ~240 wakeups per second and
nobody noticed until it was measured:

```bash
perf stat -e sched:sched_wakeup -p "$(pgrep -x glimpse-panel)" -- sleep 10
```

**Anything slow in a service handler.** One task owns that service's events, commands, and
configuration. A blocking call delays every consumer of that service, so backend work must be async
and unrelated expensive work must leave the handler.

## Prefer signals to polling, always

Mirror services enumerate once and then follow change signals. An interval is for something with no
signal at all — a clock, a sensor with no notification. If you are writing `ctx.interval` against a
backend that has a `PropertiesChanged`, that is a bug, not a tuning choice.

When an interval is genuinely right, `MissedTickBehavior::Skip` is already set: a tick still running
when the next is due does not stack them up, so a slow handler falls behind rather than building a
backlog it can never clear.

## The equality gates are load-bearing

`Publisher::set` and `Publisher::update` use the watch cell's current value as an equality gate. An
unchanged state produces no notification, so take the publisher once in `start` and hold it.

This is why state implements `PartialEq`, and why a state carrying a timestamp or sequence number of
its own defeats the gate: every update differs even when the domain value did not.

## Where allocation happens

| | Frequency | Notes |
| --- | --- | --- |
| `subscriptions()` — a `Vec` plus a boxed closure per source | after **every** input | noise at the handful of sources any service here declares |
| `S::Config::from(document)` | once per service per reload | plus one `Clone`, not a second projection |
| cloning a watch state in `Publisher::update` | only when updating in place | prefer `set` when the complete next state already exists |

None of these is worth restructuring for today. Measure before you assume otherwise — the ~240
wakeups/second above was found with `perf`, not by reading code.

## Inbox pressure

One inbox, 128 deep, carrying events, commands and configuration together. One channel means one
order, and the cost of that is a shared budget:

- A service flooding its own inbox with events makes `ServiceEndpoint::command` refuse commands
  with `Unavailable` — the honest answer, but a coarse one.
- `reconfigure` drops the update with a warning rather than awaiting, because the reloader task
  serves every service and must not park behind one.

If a backend can produce events faster than the handler consumes them, coalesce **before** the inbox
— in the source — not after.

## Do not hold the handler

Handlers run serially on `&mut self`. A handler that awaits a backend freezes every other thing the
service owns, including its commands and its configuration.

```rust
Input::Command(Command::Slow { reply }) => {
    let outcome = slow(ctx).await;
    let _ = reply.send(outcome);
}
```

Await when command ordering matters. Use `ctx.spawn` when asynchronous work must later re-enter the
handler as an event, and `ctx.spawn_detached` only for work with no reply and no state transition.

## Cap what comes off a backend

Tray titles, notification bodies, MPRIS metadata and SSIDs are unbounded and attacker-controlled.
Capping length is a security rule first, but it is also what stops one hostile application making
every state update large. Cap before publication, not at the widget.

## Measuring

```bash
just run-panel
perf stat -e sched:sched_wakeup -p "$(pgrep -x glimpse-panel)" -- sleep 10
```

Observe a concrete handle's watch receiver in a headless test to catch unchanged republishing. For a
live process, use its exact PID so another session instance is not affected.
