# glimpsed

The daemon. Broker, service host, and the only process that talks to backends.

Owns every topic, serves every client from one socket, and holds the two D-Bus names that have no
backing store anywhere else: `org.freedesktop.Notifications` and `org.kde.StatusNotifierWatcher`.

## Contents

- `broker/` — the single task holding topic values, pattern matching, per-client coalescing
- `socket.rs` — listener, per-client reader and writer tasks, the NDJSON codec
- `registry.rs` — service registration, DAG validation, supervision
- `wayland/` — the `WaylandEdge` implementation: gamma, idle, clipboard

## Rules

Nothing depends on this crate. It is a leaf.

The broker routes and nothing else. No icon work, no image decoding, no synchronous writes to
clients — anything slow inline hits every client's latency.

`wl_` objects appear only under `wayland/`. Services reach Wayland through `trait WaylandEdge`,
which is what keeps every service test headless.

Never add `panic = "abort"`. Per-service panic isolation depends on unwinding.

Spec: [`specs/003_glimpsed.md`](../../specs/003_glimpsed.md) ·
configuration: [`specs/010_configuration.md`](../../specs/010_configuration.md) ·
units: [`specs/009_systemd.md`](../../specs/009_systemd.md)
