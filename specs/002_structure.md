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
│                           portal definitions, default config
├── specs/                  this document set
├── scripts/                development helpers, not installed
├── wallpapers/             bundled wallpapers
└── var/                    scratch, not installed
```

### Crates

Five libraries, five shipped binaries, one development binary.

| Crate               | Kind          | Contains                                                                   |
| ------------------- | ------------- | -------------------------------------------------------------------------- |
| `glimpse-proto`     | lib           | wire frames, `Topic` trait, payload types, error codes, `PROTOCOL_VERSION` |
| `glimpse-client`    | lib           | async socket client: connect, reconnect, resubscribe, typed topic cache    |
| `glimpse-config`    | lib           | layered TOML load, merge, validate, watch                                  |
| `glimpse-services`  | lib           | service framework and every service implementation                         |
| `glimpse-widgets`   | lib           | GObject subclasses, Blueprint templates, shared CSS                        |
| `glimpsed`          | bin           | broker, socket server, `WaylandEdge` implementation, wiring, `main`        |
| `glimpse-panel`     | bin `glimpse` | panel, applets, popovers, notification popups                              |
| `glimpse-wallpaper` | bin           | background layer surface, live effects                                     |
| `glimpse-lock`      | bin           | `ext-session-lock-v1` surfaces, PAM                                        |
| `glimpsectl`        | bin           | CLI and TUI                                                                |
| `glimpse-devtools`  | bin           | widget previewer with hot reload, not installed                            |

### Dependency direction

```
                    glimpse-proto            (serde only: no tokio, no zbus, no GTK)
                     ↑    ↑     ↑
      glimpse-client ┘    │     └ glimpse-services
             ↑            │              ↑
             │      glimpse-config       │
             │            ↑              │
   ┌─────────┴────────────┴──────────────┘
   │
   ├── glimpsed                          (+ zbus, wayland-client, pipewire)
   ├── glimpse-panel ────┐
   ├── glimpse-wallpaper ┼── glimpse-widgets   (+ gtk4, adw, relm4)
   ├── glimpse-lock ─────┘
   ├── glimpsectl
   └── glimpse-devtools ─── glimpse-widgets
```

Two rules carry the whole layout.

**Nothing depends on `glimpsed`.** It is a leaf. Anything two crates need lives in proto, client,
config, services or widgets.

**`glimpse-proto` takes serde and nothing else.** It is the input to `schemars` for generating the
Python, TypeScript and Go SDK types. A tokio or zbus dependency closes that door.

### The services and daemon split

`glimpse-services` owns the framework and the implementations; `glimpsed` owns the broker and the
socket. The arrow points from `glimpsed` to `glimpse-services` and never back, so anything the
framework needs from the daemon is declared as a trait in `glimpse-services` and implemented in
`glimpsed`:

| Declared in `glimpse-services`                                     | Implemented in `glimpsed` |
| ------------------------------------------------------------------ | ------------------------- |
| `trait BrokerHandle` — publish, claim dynamic topic, report health | the broker task           |
| `trait WaylandEdge` — gamma, idle, clipboard                       | the real Wayland client   |

Both ship a mock beside the declaration, so `just test-crate glimpse-services` runs every service
with no display, no session bus and no broker.

### Module layout

#### glimpse-proto

```
src/
├── lib.rs           re-exports, PROTOCOL_VERSION
├── frame.rs         Frame, Body, Status
├── topic.rs         trait Topic, pattern matching rules
├── error.rs         CallError { code, message, retryable }
└── topics/          one module per domain, payload types only
    ├── audio.rs   battery.rs   bluetooth.rs   brightness.rs
    ├── clipboard.rs   idle.rs   keyboard.rs   mpris.rs
    ├── network.rs   nightlight.rs   notifications.rs   power.rs
    ├── sysstats.rs   theme.rs   tray.rs   weather.rs   workspaces.rs
    └── system.rs    system.services, config.reloaded
```

#### glimpse-services

```
src/
├── lib.rs           register(), the service registry entry point
├── service.rs       trait Service, associated Command and Source types
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
    └── workspaces.rs
```

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
├── socket.rs        listener, per-client reader and writer tasks, NDJSON codec
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
| Wire payload type                                             | `glimpse-proto/src/topics/`      |
| Service implementation                                        | `glimpse-services/src/services/` |
| Anything touching a `wl_` object                              | `glimpsed/src/wayland/`          |
| Anything touching GTK                                         | a UI crate or `glimpse-widgets`  |
| systemd unit, D-Bus service file, pam.d entry, default config | `data/`                          |
| Anything not installed                                        | `scripts/` or `var/`             |

### Testing

| Level                                   | Where                                                                      | Needs                |
| --------------------------------------- | -------------------------------------------------------------------------- | -------------------- |
| Payload round-trip, pattern matching    | `glimpse-proto` unit tests                                                 | nothing              |
| Service behaviour                       | `glimpse-services` unit tests against `BrokerHandle` / `WaylandEdge` mocks | nothing              |
| Broker fan-out, coalescing, client caps | `glimpsed` unit tests                                                      | nothing              |
| Socket protocol                         | `glimpsed/tests/` over a temporary socket                                  | nothing              |
| Wayland backends                        | `glimpsed/tests/`, `#[ignore]` by default                                  | a running compositor |
| Widgets                                 | `glimpse-devtools`, by eye                                                 | a display            |

### Naming

- Crate `glimpse-panel` builds binary `glimpse`. Every other crate's binary matches its name.
- Topics are `domain.name` in lower snake case with dots as separators: `audio.volume`,
  `tray.item.{id}.menu`.
- Commands are `domain.verb_object`: `audio.set_volume`, `tray.menu_about_to_show`.
- Config files are named for their binary: `glimpsed.toml`, `panel.toml`, `lock.toml`,
  `wallpaper.toml`.

## Alternatives considered

- **Services as modules inside `glimpsed`** — rejected: it makes the daemon crate the home of both
  the broker and every backend integration, so a service test pulls in the socket server. The trait
  indirection that keeps them apart costs two small files.
- **`WaylandEdge` as its own crate** — rejected: the trait is what buys headless tests, and a crate
  boundary adds nothing until smithay compile times argue for it. The trait sits in
  `glimpse-services` with the services that use it; only the implementation is in `glimpsed`.
- **A crate per service** — rejected: twelve more manifests, and every service needs the framework
  anyway, so the boundaries would be nominal.
- **A shared `glimpse-core` holding everything non-UI** — rejected: it becomes the crate everything
  depends on and nothing can be tested without, which is the outcome the proto/client/config split
  exists to avoid.

## Changelog

- 2026-08-20 — created.
- 2026-08-20 — dropped `glimpse-sunset`.
