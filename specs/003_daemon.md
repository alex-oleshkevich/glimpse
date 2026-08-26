---
state: draft
---

# 003 — Daemon

The broker, the service host, and the only process that talks to a backend. `glimpsed` is the
artifact that runs it.

## Problem

Every other binary needs the same session state and none of them should fetch it. Without a single
host, four processes open four PipeWire connections, and the two bus names with no backing store —
`org.freedesktop.Notifications` and `org.kde.StatusNotifierWatcher` — live or die with whichever UI
process happens to own them.

The previous implementation shows the shape of the problem from the other side. Every service was
constructed and spawned at start, unconditionally, so a session that never opened a Bluetooth menu
still ran a BlueZ client all day. Dependencies were wired by passing one service's handle into the
next one's constructor, which makes the dependency graph a function body: adding a service means
editing it, and the start order is whatever the call order happens to be. And a configuration change
broadcast the whole configuration to every service at once, so editing a panel colour woke the
network service.

## Goals

- Own every topic; be the only writer for each.
- Serve every client from one socket with snapshot-on-subscribe semantics.
- Keep the two unbacked bus names alive for the whole session regardless of UI restarts.
- Do no backend work for topics nobody is watching.
- Report its own health honestly rather than hiding a failed service.
- Survive one bad service. A panicking handler must not take the session's tray with it.

## Non-goals

- No GTK, no icon theme, no rendering.
- No configuration writing. The config file belongs to the user.
- No policy of its own on top of a backend — no auto-connect, no reconnect loops, no retries layered
  over NetworkManager.

## Tech

### Topics

A topic is a **state cell, not an event log**. Every event carries the whole value, so reconnecting
equals resubscribing equals a fresh snapshot, and coalescing is lossless: dropping an intermediate
value loses nothing a later one does not already carry.

Names are `domain.name` in lower snake case: `audio.volume`, `tray.item.{id}.menu`. Patterns are
`audio.*` for one level and `tray.**` for a subtree. Commands are `domain.verb_object`.

Payload types derive `PartialEq`, and that equality gate is what stops a service republishing a
value that did not change. It is the same gate configuration reload uses, one level down.

### The broker

One task owns every topic value and every subscription. It **routes and nothing else**: no image
decoding, no icon work, no filesystem access, and no synchronous write to a client. Anything slow
inline is paid by every client's latency, not just the one that caused it.

Writes go to per-client channels. A client over its buffered-byte cap is disconnected rather than
allowed to stall the loop — a slow reader is the one failure that would otherwise convert into
everyone's failure.

No `unwrap()` and no `expect()` anywhere in it. A panic in the broker takes every client's
connection with it.

### Services

A service owns a slice of state and the backend behind it. Handlers run serially on `&mut self`, so
a handler that can await a backend moves its responder into a spawned task; otherwise one wedged
application freezes every other item the service owns.

Three kinds, and the kind determines the rules:

| Kind         | Rule                                                                   |
| ------------ | ---------------------------------------------------------------------- |
| **owned**    | the state exists nowhere else, so the daemon is the source of truth    |
| **mirror**   | the backend owns the state; enumerate once, then follow change signals |
| **computed** | derived from other topics and configuration                            |

For a mirror, the backend is right when they disagree — converge, do not arbitrate — and a command
is a thin pass-through. `network.connect { uuid }` calls `ActivateConnection` and lets the backend's
state machine produce the result.

**Panic isolation is a design constraint, not a nicety.** A panicking handler takes down its own
service and cascades `degraded` to dependants; it does not take down the daemon. This is why no
profile may ever set `panic = "abort"`: unwinding is what makes the isolation real, and aborting
turns one bad handler into a dead session with no tray and no notifications.

### Demand and lifecycle

A service declares when it starts and when it stops. `OnDemand` services do no backend work until
something wants their topics; `WhenIdle` starts a grace timer when the last demand disappears, and
new demand cancels it.

Demand is any of: a client pattern matching one of the service's topics, an in-process
`Ctx::subscribe` on one, a `call` naming one of its commands, or a dependent service starting.
Because `Ctx::subscribe` counts, a computed service keeps its inputs alive without special-casing.

Dependencies are **declared, not wired**. The registry validates the graph as a DAG, orders boot
along it, and cascades `degraded` downward, so a consumer can render the daemon's honesty about
stale upstream data instead of guessing.

### Configuration

`$XDG_CONFIG_HOME/glimpse/config.toml`, shared with the UI binaries. The daemon owns one table per
service and ignores the tables the panel, wallpaper and lock own. Schema, layers, drop-ins, merge
semantics and validation are all in [`010_configuration.md`](010_configuration.md).

A reload re-applies **only the services whose own table changed**, compared by `PartialEq`. Editing
`[[panels]]` does not perturb the night light schedule.

An invalid configuration never stops the daemon starting: it logs with the error's location and
falls back to defaults. An invalid reload is dropped and the running configuration stays.

### The socket

Newline-delimited JSON over a Unix socket at `$XDG_RUNTIME_DIR/glimpse/glimpsed.sock`, mode 0600.
Frames, the codec, the handshake, the limits and both ends of the transport are `012_ipc.md`; the
daemon uses `glimpse-ipc`'s server rather than implementing one.

Two consequences belong to the daemon rather than to the wire:

- **A second instance fails rather than stealing the socket.** Connect before binding; a socket that
  answers means a live daemon and exit 3. Unlinking first is what lets a second daemon silently
  displace a live one and leave every connected client talking to nothing.
- **A command that could not be delivered returns an error.** A full or closed service channel means
  the command did not take effect, and reporting success for it is worse than reporting failure.

### Health

`system.services` is a topic carrying the state and health of every registered service, with the
reason for any `degraded`. That is what makes `glimpsectl services` and `doctor` possible without a
side channel, and what surfaces a bus name lost to another notification daemon — a failure that is
otherwise invisible until someone sends a notification.

Backend call durations are logged separately from internal handling, so a slow tray is attributable
to the application rather than to the daemon.

## The binary

```
glimpsed [OPTIONS]
```

No subcommands and no arguments. One instance per session.

| Flag                    | Default                                  | Purpose                                                        |
| ----------------------- | ---------------------------------------- | -------------------------------------------------------------- |
| `-c`, `--config <PATH>` | the layered stack                        | Use exactly this file; skip the system and user layers         |
| `--socket <PATH>`       | `$XDG_RUNTIME_DIR/glimpse/glimpsed.sock` | Override the listening socket                                  |
| `--only <SERVICES>`     | all                                      | Comma-separated allowlist; everything else stays unregistered  |
| `--without <SERVICES>`  | none                                     | Comma-separated denylist                                       |
| `--log <FILTER>`        | `info`                                   | `tracing-subscriber` filter, same syntax as `RUST_LOG`         |
| `--log-format <FMT>`    | `auto`                                   | `auto`, `plain`, `json`; `auto` drops timestamps under journal |
| `-V`, `--version`       |                                          | Version and protocol version                                   |
| `-h`, `--help`          |                                          |                                                                |

Configuration inspection is not the daemon's. `glimpsectl config show` prints what the running
daemon merged; `config validate` and `config path` re-read the stack themselves, so neither needs a
daemon — which is the case a user actually hits, because the daemon is what will not start. See
`007_glimpsectl.md`.

`--only` and `--without` are debugging aids: they make it possible to run the daemon with just
`audio` while working on the audio service. A service excluded this way is absent from
`system.services` rather than reported as failed. The two are mutually exclusive; supplying both is
a usage error.

`--log-format auto` resolves to `plain` without timestamps or colour when `JOURNAL_STREAM` is set,
because the journal stamps its own lines, and to `plain` with both otherwise.

### Environment

| Variable                                     | Use                                                                |
| -------------------------------------------- | ------------------------------------------------------------------ |
| `XDG_RUNTIME_DIR`                            | socket and tray icon directory; required                           |
| `XDG_CONFIG_HOME`, `XDG_CONFIG_DIRS`         | configuration lookup                                               |
| `GLIMPSE_CONFIG_PATH`                        | default for `--config`                                             |
| `GLIMPSE_SOCKET_PATH`                        | default for `--socket`                                             |
| `WAYLAND_DISPLAY`                            | required for the `WaylandEdge` capabilities; absent means degraded |
| `DBUS_SESSION_BUS_ADDRESS`                   | session bus; unreachable degrades services, not a failed start     |
| `NIRI_SOCKET`, `HYPRLAND_INSTANCE_SIGNATURE` | compositor IPC discovery                                           |
| `RUST_LOG`                                   | fallback when `--log` is absent                                    |
| `NOTIFY_SOCKET`                              | set by systemd; enables readiness and watchdog notification        |

### Files

| Path                                              | Role                     |
| ------------------------------------------------- | ------------------------ |
| `$XDG_RUNTIME_DIR/glimpse/glimpsed.sock`          | client socket, mode 0600 |
| `$XDG_RUNTIME_DIR/glimpse/tray/<item>-<hash>.png` | decoded tray pixmaps     |
| `$XDG_CONFIG_HOME/glimpse/config.toml`            | user configuration       |
| `/etc/glimpse/config.toml`                        | system configuration     |

### Signals

| Signal    | Effect                                                                                                                        |
| --------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `SIGTERM` | graceful shutdown: stop accepting, drain writers, stop services in reverse DAG order                                          |
| `SIGINT`  | same as `SIGTERM`                                                                                                             |
| `SIGHUP`  | reload configuration per `010`; only services whose table changed are re-applied, and an invalid config is logged and dropped |

### systemd integration

`Type=notify`. `READY=1` once the socket is listening and every `OnBoot` service has reached `ready`
or `degraded`; `STOPPING=1` on shutdown; `WATCHDOG=1` from the broker loop, so a wedged broker is
detected rather than silently ceasing delivery. Details in `009_systemd.md`.

### Exit codes

| Code | Meaning                                |
| ---- | -------------------------------------- |
| 0    | clean shutdown                         |
| 2    | usage error                            |
| 3    | socket already in use by a live daemon |
| 4    | `XDG_RUNTIME_DIR` unset or unusable    |

Invalid configuration is not an exit. It logs and falls back to defaults at startup, and is dropped
on reload — see `010_configuration.md`.

## Risks

- **Operational** — owning two well-known bus names means a crash loop removes tray and
  notifications for every application in the session. Backoff, health reporting and the watchdog are
  load-bearing, not decoration.

## Alternatives considered

- **Spawning every service at start** — rejected: it is what the previous implementation did, and it
  means a session that never opens a Bluetooth menu still runs a BlueZ client all day. Demand-driven
  lifecycles cost a registry and save every backend nobody is using.
- **Wiring dependencies through constructors** — rejected: passing one service's handle into the
  next one's constructor makes the dependency graph a function body, with start order implied by
  call order and no way to validate it. Declared dependencies give a DAG that can be checked and
  ordered.
- **Broadcasting the whole configuration on reload** — rejected: every service wakes for every edit.
  Per-service comparison means a service whose table did not change never learns a reload happened.
- **Unlinking a stale socket before binding** — rejected: it cannot distinguish a stale socket from a
  live daemon, so a second instance silently displaces the first.

## Changelog

- 2026-08-20 — created, split out of `001_architecture.md`.
- 2026-08-20 — added the Configuration section: one `config.toml`, table ownership, per-service diffing, validation rules.
- 2026-08-20 — fixed `--config`'s default, which pointed at spec `004`.
- 2026-08-20 — clarified `--log-format auto`: plain without timestamps under a journal stream, not a separate `journal` value.
- 2026-08-20 — `--only` and `--without` are mutually exclusive; supplying both is a usage error.
- 2026-08-20 — configuration schema moved out to `010_configuration.md`; this spec keeps the flags, the file paths and the reload signal.
- 2026-08-20 — invalid configuration no longer exits at startup; exit 1 is now only `--check-config`.
- 2026-08-20 — renamed from `003_glimpsed.md` and reorganised around the daemon's concepts — topics, broker, services, demand, socket, health — with the binary and its flags as one section rather than the subject.
- 2026-08-20 — recorded from `_old`: eager service spawning, constructor-wired dependencies and whole-config broadcast as the problems the design answers, plus the socket rules it got right (no `/tmp` fallback, 0600) and wrong (unlinking before bind, and disabling IPC instead of failing).
- 2026-08-20 — the socket section points at `012_ipc.md` for frames, codec, handshake and limits, keeping only what is the daemon's rather than the wire's. The transport moved to `glimpse-ipc`, so `socket.rs` leaves this crate.
- 2026-08-21 — `--check-config` and `--print-config` removed; configuration inspection is `glimpsectl`'s alone, and exit 1 no longer has a cause.
- 2026-08-26 — an unreachable D-Bus bus degrades the services that need it rather than stopping the daemon; see the decision log.
