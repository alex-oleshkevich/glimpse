---
paths:
  - "crates/glimpsed/**"
  - "crates/glimpse-services/**"
  - "crates/glimpse-ipc/**"
---

# Daemon and service conventions

Architecture in `specs/001_architecture.md`, the daemon's own contract in `specs/003_daemon.md`.

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

## Payloads and config

These two rules point in opposite directions on purpose:

- **Config** rejects unknown fields (`#[serde(deny_unknown_fields)]`), so a typo is an error the
  user sees rather than a setting silently ignored.
- **Wire payloads** accept unknown fields, so a newer daemon and an older client survive a version
  skew instead of failing to deserialize.

Payload types derive `PartialEq`. That is the equality gate that stops a service republishing an
identical value.

## Boundaries

- `wl_` objects appear only under `glimpsed/src/wayland/`. Services reach Wayland through
  `trait WaylandEdge`.
- A dependency belongs in `glimpse-ipc` only if both ends of the socket need it, plus `tracing`.
  No zbus, no GTK, no backend type in `topics/`.
- Nothing depends on `glimpsed`.
