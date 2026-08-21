---
state: draft
---

# 002 — Repository and Module Structure

Where every kind of file lives, which crate owns it, and which way the dependency arrows point.

## Problem

Eleven crates share five libraries and one wire format. Without an agreed layout, the service
framework and the broker end up depending on each other, the payload crate accumulates a tokio
dependency that closes off SDK generation, and shared widgets get copy-pasted between the panel and
the lock screen until they drift apart.

## Goals

- A new file has one obvious home.
- The dependency graph is acyclic and stays that way without discipline.
- Every layer except the Wayland backends runs headless in CI.
- Adding a service or a topic touches exactly one crate.

## Non-goals

- No directory-per-spec or nested crate hierarchies. `crates/*` is flat.
- No crate boundary drawn for its own sake. A module suffices until compile times or a dependency
  cycle argue otherwise.

## Tech

### Repository

```
glimpse/
├── Cargo.toml              workspace: members = ["crates/*"], workspace.package, workspace.dependencies
├── justfile                task runner
├── crates/                 all Rust code
├── data/                   installed assets: systemd units, D-Bus service files, pam.d,
│                           portal definitions, default config (data/config.default.toml)
├── specs/                  this document set
├── scripts/                development helpers, not installed
├── wallpapers/             bundled wallpapers
└── var/                    scratch, not installed
```

### Crates

Four libraries, five shipped binaries, one development binary.

| Crate               | Kind          | Contains                                                                   |
| ------------------- | ------------- | -------------------------------------------------------------------------- |
| `glimpse-ipc`       | lib           | the wire format and both ends of the transport: frames, codec, topics, errors, client, server |
| `glimpse-config`    | lib           | layered TOML load, drop-ins, merge, validate                              |
| `glimpse-services`  | lib           | service framework and every service implementation                         |
| `glimpse-widgets`   | lib           | GObject subclasses, Blueprint templates, shared CSS                        |
| `glimpsed`          | bin           | broker, `WaylandEdge` implementation, wiring, `main`                       |
| `glimpse-panel`     | bin `glimpse` | panel, applets, popovers, notification popups                              |
| `glimpse-wallpaper` | bin           | background layer surface, decode cache, transitions                        |
| `glimpse-lock`      | bin           | `ext-session-lock-v1` surfaces, PAM                                        |
| `glimpsectl`        | bin           | CLI and TUI                                                                |
| `glimpse-devtools`  | bin           | widget previewer with hot reload, not installed                            |

### Dependency direction

```
                    glimpse-ipc              (what both ends need; no zbus, no GTK)
                     ↑         ↑
                     │         └ glimpse-services
                     │                  ↑
              glimpse-config            │
                     ↑                  │
   ┌─────────────────┴──────────────────┘
   │
   ├── glimpsed                          (+ zbus, wayland-client, pipewire)
   ├── glimpse-panel ────┐
   ├── glimpse-wallpaper ┼── glimpse-widgets   (+ gtk4, adw, relm4)
   ├── glimpse-lock ─────┘
   ├── glimpsectl
   └── glimpse-devtools ─── glimpse-widgets
```

Two rules carry the whole layout.

**Nothing depends on `glimpsed`.** It is a leaf. Anything two crates need lives in ipc, config,
services or widgets.

**`glimpse-ipc` holds the transport, `glimpsed` holds the routing.** Frames, codec, client and
server all describe how bytes cross the socket, and both ends have to agree on every one of them; a
second implementation of any of them could only drift. The broker decides *which* client gets
*which* value, which is a daemon decision and stays in `glimpsed`.

A dependency belongs in `glimpse-ipc` only if both ends of the socket need it, plus `tracing` for
diagnostics. No zbus, no GTK, and no backend type in `topics/` — a payload that names one cannot be
generated for Python, TypeScript or Go. `topics/` and `frame.rs` are the input to `schemars` for the
Python, TypeScript and Go SDK types, and a generator reads those modules rather than the whole
crate. The rule is stated as a test rather than a list because two earlier list forms were both
wrong: the first barred tokio on the grounds that it broke schema generation, which is not true, and
the second read "serde and tokio, nothing else" while the manifest already carried `dirs`,
`serde_json` and `tracing`.

### The services and daemon split

`glimpse-services` owns the framework and the implementations; `glimpsed` owns the broker. The arrow points from `glimpsed` to `glimpse-services` and never back, so anything the
framework needs from the daemon is declared as a trait in `glimpse-services` and implemented in
`glimpsed`:

| Declared in `glimpse-services`                                     | Implemented in `glimpsed` |
| ------------------------------------------------------------------ | ------------------------- |
| `trait BrokerHandle` — publish, claim dynamic topic, report health | the broker task           |
| `trait WaylandEdge` — gamma, idle, clipboard                       | the real Wayland client   |

Both ship a mock beside the declaration, so `just test-crate glimpse-services` runs every service
with no display, no session bus and no broker.

### Module layout

#### glimpse-ipc

```
src/
├── lib.rs           re-exports, PROTOCOL_VERSION, SOCKET_RELATIVE_PATH, socket_path
├── frame.rs         Frame, Body, Status
├── codec.rs         LinesCodec + serde_json; over-length and malformed lines close
├── topic.rs         trait Topic, pattern matching rules
├── error.rs         CallError { code, message, retryable }
├── client/          connect, reconnect with backoff, resubscribe, typed topic cache
├── server/          listener, per-client reader and writer tasks, byte caps
└── topics/          one module per domain, payload types only
    ├── audio.rs   battery.rs   bluetooth.rs   brightness.rs
    ├── clipboard.rs   idle.rs   keyboard.rs   mpris.rs
    ├── network.rs   nightlight.rs   notifications.rs   power.rs
    ├── sysstats.rs   theme.rs   tray.rs   watch.rs   weather.rs
    ├── workspaces.rs
    └── system.rs    system.services
```

#### glimpse-services

```
src/
├── lib.rs           register(), the service registry entry point
├── service.rs       trait Service, associated Command, Source and Config types
├── ctx.rs           Ctx: publish, subscribe, call, spawn, interval, health, apply
├── publisher.rs     Publisher<T>, dynamic topic claims, equality gate
├── lifecycle.rs     Start × Stop classes, the state machine, backoff
├── broker_handle.rs trait BrokerHandle + mock
├── wayland_edge.rs  trait WaylandEdge + mock
├── dbus/            shared zbus helpers: property streams, signal streams
└── services/        one module per service
    ├── audio.rs   battery.rs   bluetooth.rs   brightness.rs
    ├── clipboard.rs   geolocation.rs   idle.rs   keyboard.rs
    ├── mpris.rs   network.rs   nightlight.rs   notifications.rs
    ├── power.rs   sysstats.rs   theme.rs   tray/   weather.rs
    └── watcher.rs   workspaces.rs
```

Each service declares its own `Config`. On reload the framework deserializes that service's table
from `config.toml`, compares it against the running value, and calls `apply` only where the two
differ — the same equality gate `publisher.rs` applies to payloads, one level up. A service whose
table did not change never learns a reload happened.

`tray/` is a directory because it is the largest service: `mod.rs`, `watcher.rs` (serves
`org.kde.StatusNotifierWatcher`), `item.rs` (per-item SNI proxy), `menu.rs` (dbusmenu bridge),
`icons.rs` (pixmap decode into `$XDG_RUNTIME_DIR`).

#### glimpsed

```
src/
├── main.rs          argument parsing, tracing setup, sd_notify, shutdown
├── broker/
│   ├── mod.rs       the single broker task and its mailbox
│   ├── store.rs     latest value per topic, stale flag
│   ├── subscribers.rs  pattern registry, per-client coalescing, byte caps
│   └── handle.rs    impl BrokerHandle
├── registry.rs      service registration, DAG validation, supervision
└── wayland/
    ├── mod.rs       impl WaylandEdge
    ├── gamma.rs     wlr-gamma-control and hyprland-ctm backends
    ├── idle.rs      ext-idle-notify-v1
    └── clipboard.rs ext-data-control-v1
```

#### glimpse-panel

```
src/
├── main.rs          GTK application, layer-shell setup, per-output panels
├── panel.rs         panel window, zones, applet placement
├── applets/         one module per applet, each a client of one or more topics
├── dialogs/         GTK dialogs, prompts
└── popups/          notification popups, OSD
```

An applet renders topics and sends commands. It never opens a D-Bus connection, never reaches a
backend directly, and holds no state that outlives its own widget.

#### glimpse-widgets

```
src/
├── lib.rs
└── <widget>/        one directory per widget: mod.rs + imp.rs (GObject boilerplate)
blueprints/          .blp templates, compiled by build.rs via glib-build-tools
```

A widget moves here as soon as a second binary needs it. Copy-paste between panel and lock is what
this crate exists to prevent.

### File placement

| Kind of file                                                  | Goes in                          |
| ------------------------------------------------------------- | -------------------------------- |
| Wire payload type                                             | `glimpse-ipc/src/topics/`        |
| Service implementation                                        | `glimpse-services/src/services/` |
| Anything touching a `wl_` object                              | `glimpsed/src/wayland/`          |
| Anything touching GTK                                         | a UI crate or `glimpse-widgets`  |
| systemd unit, D-Bus service file, pam.d entry, default config | `data/`                          |
| Anything not installed                                        | `scripts/` or `var/`             |

### Testing

| Level                                   | Where                                                                      | Needs                |
| --------------------------------------- | -------------------------------------------------------------------------- | -------------------- |
| Payload round-trip, pattern matching    | `glimpse-ipc` unit tests                                                   | nothing              |
| Client against server, end to end       | `glimpse-ipc` unit tests: encode, decode, respond, decode                  | nothing              |
| Service behaviour                       | `glimpse-services` unit tests against `BrokerHandle` / `WaylandEdge` mocks | nothing              |
| Broker fan-out, coalescing, client caps | `glimpsed` unit tests                                                      | nothing              |
| Wayland backends                        | `glimpsed/tests/`, `#[ignore]` by default                                  | a running compositor |
| Widgets                                 | `glimpse-devtools`, by eye                                                 | a display            |

### Naming

- Crate `glimpse-panel` builds binary `glimpse`. Every other crate's binary matches its name.
- Topics are `domain.name` in lower snake case with dots as separators: `audio.volume`,
  `tray.item.{id}.menu`.
- Commands are `domain.verb_object`: `audio.set_volume`, `tray.menu_about_to_show`.
- One configuration file, `config.toml`, with a top-level table per owner: one table per service
  for the daemon, `[panel]`, `[wallpaper]` and `[lock]` for the UI binaries. A
  binary reads only the tables it owns and ignores the rest. Stylesheets stay separate files,
  `panel.css` and `lock.css`, because CSS is not TOML. See `010_configuration.md`.

## Alternatives considered

- **Services as modules inside `glimpsed`** — rejected: it makes the daemon crate the home of both
  the broker and every backend integration, so a service test pulls in the socket server. The trait
  indirection that keeps them apart costs two small files.
- **`WaylandEdge` as its own crate** — rejected: the trait is what buys headless tests, and a crate
  boundary adds nothing until smithay compile times argue for it. The trait sits in
  `glimpse-services` with the services that use it; only the implementation is in `glimpsed`.
- **A crate per service** — rejected: twelve more manifests, and every service needs the framework
  anyway, so the boundaries would be nominal.
- **A separate `glimpse-client` crate** — rejected after being specified: the client and the server
  are two ends of one wire format, and splitting them meant the codec had two homes and the only
  place a real client met a real server was an integration test over a temporary socket. Merged, a
  round trip is a unit test. The cost is that every UI binary compiles a socket server it never
  runs, which is a few hundred lines the linker drops.
- **A shared `glimpse-core` holding everything non-UI** — rejected: it becomes the crate everything
  depends on and nothing can be tested without, which is the outcome the proto/client/config split
  exists to avoid.

## Changelog

- 2026-08-20 — created.
- 2026-08-20 — dropped `glimpse-sunset`.
- 2026-08-20 — collapsed the four per-binary config files into one `config.toml` with a table per owner; naming rule restated around what a file configures rather than which binary reads it.
- 2026-08-20 — config file named `config.toml`; no `[daemon]` table.
- 2026-08-20 — dropped the `config.reloaded` topic; config load outcomes are logged and `glimpsectl config validate` reports them on demand.
- 2026-08-20 — added `services/watcher.rs` and `topics/watch.rs`; watching moved from `glimpse-config` to the daemon's watcher service, per `011_watcher.md`.
- 2026-08-20 — `glimpse-client` owns resolving the socket path and `GLIMPSE_SOCKET_PATH`; `glimpsed` binds what it resolves rather than computing the same path a second time.
- 2026-08-20 — the socket's name under `$XDG_RUNTIME_DIR` is a constant in `glimpse-proto`, so the daemon that binds it and the client that connects to it derive the same path without either depending on the other. `glimpse-client` exposes a connection, never a path.
- 2026-08-20 — socket path resolution is `glimpse_proto::socket_path`, not `glimpse-client`: both ends must agree on it and neither may depend on the other.
- 2026-08-20 — the NDJSON codec moves from `glimpsed/src/socket.rs` to `glimpse-proto/src/codec.rs`. It is the one part both ends execute identically, and a second implementation in `glimpse-client` could only diverge. The client and the socket server stay where they are: both need tokio, which proto does not take, and a merged crate would make every UI binary compile a socket server it never runs.
- 2026-08-20 — `glimpse-proto` and `glimpse-client` merge into `glimpse-ipc`, holding the wire format and both ends of the transport; the broker stays in `glimpsed`, so the split is transport against routing rather than client against server. The serde-only rule is replaced: it barred tokio on the grounds that it broke `schemars`, which is not true.
- 2026-08-21 — `glimpse-ipc`'s dependency rule is stated as a test — what both ends of the socket need — rather than a list. The "serde and tokio, nothing else" form was already false: the manifest carried `dirs`, `serde_json` and `tracing` before `thiserror` was added. Error-type convention recorded in `decisions.md`.
