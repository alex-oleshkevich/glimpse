---
paths:
  - "crates/glimpsed/**"
  - "crates/glimpse-services/**"
  - "crates/glimpse-ipc/**"
---

# Daemon and service conventions

## The broker

- It routes and nothing else. No image decoding, no icon work, no filesystem access, no synchronous
  writes to clients inside the broker task — anything slow inline hits every client's latency.
- Writes go to per-client channels. A client over its buffered-byte cap is disconnected, never
  allowed to stall the loop.

## Service handlers

- Handlers run serially on `&mut self`. A handler that can await a backend moves its `Responder`
  into `ctx.spawn`, or one wedged application freezes every other item the service owns.
- No `unwrap()` or `expect()`. A panic takes the service down and cascades `degraded` to dependants.
- No blocking calls: no `std::fs`, no `Command::output()`, no `std::sync::Mutex` held across an
  `.await`. Use `tokio::fs` and `ctx.spawn`.
- A service never retries what its backend already retries, and never reimplements a decision the
  backend makes.
- A long-lived source is declared in `subscriptions`, not started in `start`. The runtime diffs the
  declared set after every input, so dropping a guard is never something a handler has to remember.
  Whatever must force a restart belongs in the `SubKey`; `ctx.spawn` stays for effects that fire
  once.

## Payloads and config

These two rules point in opposite directions on purpose:

- **Config** rejects unknown fields (`#[serde(deny_unknown_fields)]`), so a typo is an error the
  user sees rather than a setting silently ignored.
- **Wire payloads** accept unknown fields, so a newer daemon and an older client survive a version
  skew instead of failing to deserialize.

Payload types derive `PartialEq`. That is the equality gate that stops a service republishing an
identical value.

`S::Config` is bound by `From<&glimpse_config::Config>`, so the projection from the whole document
down to one service's slice is an impl beside the slice — never a method on the service. A service
that reads no configuration uses `type Config = NoConfig;` and writes no impl at all; `()` cannot be
used, because `From<&Config> for ()` is a foreign trait on a foreign type. `S::Config: PartialEq` is
what narrows a reload to the services whose own table moved.

## Boundaries

- `wl_` objects appear only under `glimpsed/src/wayland/`. Services reach Wayland through
  `trait WaylandEdge`.
- A dependency belongs in `glimpse-ipc` only if both ends of the socket need it, plus `tracing`.
  Topic and command payloads live in `glimpse-contracts`, bound to their names by `trait Message`
  and `trait Command`. No zbus, no GTK, no backend type in either.
- Nothing depends on `glimpsed`.
