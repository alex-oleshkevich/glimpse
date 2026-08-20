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

The socket lives under `XDG_RUNTIME_DIR` at mode 0600, and there is no `/tmp` fallback: a
predictable world-writable path invites pre-creation and symlink hijack. A second instance connects
first and exits rather than unlinking a socket a live daemon may still own.

A command that could not be delivered returns an error. Reporting success for a command that never
reached its service is worse than reporting failure.

`wl_` objects appear only under `wayland/`. Services reach Wayland through `trait WaylandEdge`,
which is what keeps every service test headless.

Never add `panic = "abort"`. Per-service panic isolation depends on unwinding.

Spec: [`specs/003_daemon.md`](../../specs/003_daemon.md) ·
configuration: [`specs/010_configuration.md`](../../specs/010_configuration.md) ·
units: [`specs/009_systemd.md`](../../specs/009_systemd.md)
