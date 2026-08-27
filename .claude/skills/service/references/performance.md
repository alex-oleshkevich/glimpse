# Cost

This is a session daemon on a laptop. The budget that matters is **wakeups while nothing is
happening** and **work done on the broker's task**, not throughput.

## The two things that are actually expensive

**A wakeup on an idle system.** A timer that fires when nothing changed costs battery for nothing,
and it costs it forever. One polling thread in this tree once produced ~240 wakeups per second and
nobody noticed until it was measured:

```bash
perf stat -e sched:sched_wakeup -p "$(pgrep -x glimpsed)" -- sleep 10
```

**Anything slow on the broker task.** The broker is one task routing for every client. A blocking
call, an image decode, an icon lookup or a synchronous write there is paid out of every other
client's latency. `BrokerHandle`'s methods are all synchronous and must not block, which is why a
subscription's sink only parks a value and a pump task does the rest.

## Prefer signals to polling, always

Mirror services enumerate once and then follow change signals. An interval is for something with no
signal at all — a clock, a sensor with no notification. If you are writing `ctx.interval` against a
backend that has a `PropertiesChanged`, that is a bug, not a tuning choice.

When an interval is genuinely right, `MissedTickBehavior::Skip` is already set: a tick still running
when the next is due does not stack them up, so a slow handler falls behind rather than building a
backlog it can never clear.

## The equality gates are load-bearing

There are two, and they are the reason a busy backend does not turn into a busy socket:

1. `Publisher::set` drops a value equal to the last one **it** sent — no serialization, no broker
   message.
2. The broker's store drops a publish whose serialized form matches the current cell.

Take the publisher once in `start` and hold it. One rebuilt per call starts from no last value every
time and defeats gate 1 entirely.

This is also why payloads derive `PartialEq`, and why a payload carrying a timestamp or a sequence
number of its own defeats both gates — every publish differs, so every publish goes out. If a
consumer needs to know when a value was set, the broker already stamps `seq` and `ts`.

## Where allocation happens

| | Frequency | Notes |
| --- | --- | --- |
| `subscriptions()` — a `Vec` plus a boxed closure per source | after **every** input | noise at the handful of sources any service here declares |
| `S::Config::from(document)` | once per service per reload | plus one `Clone`, not a second projection |
| `serde_json::to_value` in `Publisher::set` | only past the equality gate | the gate is what keeps this rare |

None of these is worth restructuring for today. Measure before you assume otherwise — the ~240
wakeups/second above was found with `perf`, not by reading code.

## Inbox pressure

One inbox, 128 deep, carrying events, commands and configuration together. One channel means one
order, and the cost of that is a shared budget:

- A service flooding its own inbox with events makes `dispatch` refuse commands with `Unavailable` —
  the honest answer, but a coarse one.
- `reconfigure` drops the update with a warning rather than awaiting, because the reloader task
  serves every service and must not park behind one.

If a backend can produce events faster than the handler consumes them, coalesce **before** the inbox
— in the source — not after.

## Do not hold the handler

Handlers run serially on `&mut self`. A handler that awaits a backend freezes every other thing the
service owns, including its commands and its configuration.

```rust
Input::Command(Command::Slow, responder) => {
    ctx.spawn(|ctx| async move {
        let outcome = slow(&ctx).await;
        responder.ok(outcome);      // the Responder moves into the task
        Event::Finished
    });
}
```

`ctx.spawn` when the task has an event worth reporting, `ctx.spawn_detached` when it has nothing —
not an invented event the handler ignores.

## Cap what comes off a backend

Tray titles, notification bodies, MPRIS metadata and SSIDs are unbounded and attacker-controlled.
Capping length is a security rule first, but it is also what stops one hostile application making
every topic update large. Cap before publication, not at the widget.

## Measuring

```bash
just run-daemon                              # a debug build is fine for wakeup counts
perf stat -e sched:sched_wakeup -p "$(pgrep -x glimpsed)" -- sleep 10
just ctl topics                              # what is declared, and what has a value
just ctl services                            # state, health, and the reason for degraded
just ctl watch 'audio.**'                    # every update, live
```

`just ctl watch` on a broad pattern is the cheapest way to catch a service republishing an unchanged
value: if lines appear while nothing is happening, an equality gate is being defeated.

`pkill glimpsed` will also kill a session daemon you did not start. Run a test daemon on its own
socket and config — `just run-daemon --socket /tmp/t.sock --config /tmp/t.toml` — and kill it by
PID.
