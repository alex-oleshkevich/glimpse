---
name: ipc-client
description: Consuming glimpse-ipc from a UI binary or a script — Client::open versus connect, why a request issued while the daemon is unreachable fails rather than queues, subscription refcounting and pattern dedup, the in-flight and subscription caps, and what Event.stale does and does not mean. Use for any code that constructs or holds a glimpse_ipc::Client, subscribes to a topic, or calls a command from outside the daemon. Trigger on the dependency, not the wording. The broker and server side belong to the daemon rules; this is the client half.
---

# ipc-client

`glimpse-ipc` is the wire between `glimpsed` and everything else. Both ends compile against it, so a
renamed topic is a compile error rather than a runtime surprise. This covers the client half — what a
panel, a lock screen or `glimpsectl` has to know.

**Verified against `crates/glimpse-ipc/src/client.rs`.** Line references are to that file. When this
disagrees with it, it is right and this is a bug.

## The one thing that catches everyone

**A request issued while the daemon is unreachable fails. It does not queue.**

`Connection::idle` (`Connection::idle`) drains the inbox during backoff and answers every request with
`ErrorCode::Unavailable`. So `subscribe` and `call` both return `Err` before `glimpsed` is up — and
a UI binary starting before the daemon is *ordinary*, not a failure: the units carry
`Wants=glimpsed.service` and never `Requires=`, and `glimpse-lock` carries no relation at all.

A caller that treats the first `Err` as terminal is dead for the session. Retry on connection-state
transitions:

```rust
let mut states = client.watch_state();
loop {
    match client.subscribe(T::NAME).await {
        Ok(mut subscription) => { /* pump until next() returns None, then return */ }
        Err(error) => {
            tracing::debug!(%error, "subscribe refused, waiting");
            if states.changed().await.is_err() {
                return;                     // the last Client was dropped
            }
        }
    }
}
```

Wait on `changed()` rather than `wait_for(Connected)`: `wait_for` checks the current value first, so
a request that failed in the window where the state still reads `Connected` retries immediately and
spins. `changed()` always waits for a real transition.

## `open` versus `connect`

Both dial once before returning; they differ only in what a failed dial means.

- `open` keeps the failure and returns a client anyway, leaving the reconnect loop to reach the
  daemon whenever it appears. **This is what every UI binary uses.**
- `connect` returns `NotListening`, which a one-shot caller such as `glimpsectl` maps to an exit
  code.

Both are `async` because the dial is. There is no synchronous constructor, so a client cannot be
built inside a relm4 `init` — open it in a spawned task and deliver it as a message. Do not delay
rendering on it: `dial` bounds the handshake with `REQUEST_TIMEOUT` (5s), and a socket file left
behind by a crashed daemon takes the full five seconds.

## One client per process

Share a single `Client`; it is cheap to clone and every handle addresses the same connection task.

Duplicate subscriptions to one pattern are safe and are what makes sharing work:

- The daemon registers a pattern once however many subscribers hold it (`Outbox::add_pattern`).
- `Request::Unwatch` prunes dead mailboxes and only sends `Unsubscribe` when **no live
  `Subscription` still holds the pattern** (`Connection::dispatch`, the `Request::Unwatch` arm).
- `resubscribe` dedups after a reconnect (`Connection::resubscribe`).

So nineteen applets across three monitors collapse to at most nineteen daemon-side patterns rather
than fifty-seven — which matters against `MAX_SUBSCRIPTIONS = 64` per connection. Each subscriber
still gets its own copy of every event.

## Lifetimes and caps

- **Dropping a `Subscription` releases its pattern.** A caller that subscribes and drops in a loop —
  a popover opening and closing — would otherwise walk into the per-connection cap.
- **`MAX_INFLIGHT = 32`**, held as a semaphore permit that travels with the request. Over it, `ask`
  returns `LimitExceeded` rather than queueing.
- **`REQUEST_TIMEOUT` belongs to the connection, not the caller.** Wrapping a call in
  `tokio::time::timeout` abandons the request while it still holds its in-flight slot; 32 of those
  lock the client out.
- **A subscription survives a reconnect** and the next value after one is a fresh snapshot.
  `next()` returning `None` means the connection stopped, which happens when the last `Client` is
  dropped — that is the only terminal case.

Known gap: a `subscribe` whose task is aborted between the `Subscribe` frame and the reply never
constructs a `Subscription`, so no `Unsubscribe` is ever sent and the daemon holds the pattern until
the next reconnect. `prune` is local-only. Tracked in beads.

## Reading events

`Subscription::matched()` is how many declared topics the pattern matched *when it was registered*.
Zero is not an error — a topic can be declared later — but it is exactly what a typo looks like, and
a caller that never reports it turns one into a silent forever-wait. Log it.

`Event.stale` means the producing **service** is not running. It does not mean the daemon is gone: a
dead daemon delivers no events at all, so nothing carries the flag. `degraded` has no wire carrier;
a service that is running badly keeps publishing current values.

Payloads carry attacker-controlled text — tray titles, MPRIS metadata, SSIDs — and are unbounded.
Cap before rendering, and keep `event.data` out of logs.

## Typing

`Frame.data` is `serde_json::Value`, because the broker routes topics it has no compile-time
knowledge of. Type at the edge:

```rust
if event.topic == T::NAME {
    T::Payload::deserialize(&event.data)   // borrow; do not clone the Value
}
```

The topic check is load-bearing for wildcard subscriptions — a sibling topic's payload will often
deserialize into the wrong type without complaint.

The wire is not versioned, and payloads accept unknown fields on purpose, so a newer daemon and an
older client survive a skew instead of failing to deserialize.
