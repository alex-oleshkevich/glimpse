---
state: draft
---

# 003 — glimpsed

The daemon: broker, service host, and the only process that talks to backends.

## Problem

Every other binary needs the same session state and none of them should fetch it. Without a single
host, four processes open four PipeWire connections, and the two bus names with no backing store —
`org.freedesktop.Notifications` and `org.kde.StatusNotifierWatcher` — live or die with whichever UI
process happens to own them.

## Goals

- Own every topic; be the only writer for each.
- Serve every client from one socket with snapshot-on-subscribe semantics.
- Keep the two unbacked bus names alive for the whole session regardless of UI restarts.
- Do no backend work for topics nobody is watching.
- Report its own health honestly rather than hiding a failed service.

## Non-goals

- No GTK, no icon theme, no rendering.
- No configuration writing. The config file belongs to the user.

## Tech

### Invocation

```
glimpsed [OPTIONS]
```

No subcommands. One instance per session; a second instance fails on the socket rather than
stealing it.

### Flags

| Flag                    | Default                                  | Purpose                                                        |
| ----------------------- | ---------------------------------------- | -------------------------------------------------------------- |
| `-c`, `--config <PATH>` | the layered stack, see Configuration     | Use exactly this file; skip the system and user layers         |
| `--socket <PATH>`       | `$XDG_RUNTIME_DIR/glimpse/glimpsed.sock` | Override the listening socket                                  |
| `--check-config`        | off                                      | Load and validate configuration, print problems, exit          |
| `--print-config`        | off                                      | Print the merged configuration as TOML and exit                |
| `--only <SERVICES>`     | all                                      | Comma-separated allowlist; everything else stays unregistered  |
| `--without <SERVICES>`  | none                                     | Comma-separated denylist                                       |
| `--log <FILTER>`        | `info`                                   | `tracing-subscriber` filter, same syntax as `RUST_LOG`         |
| `--log-format <FMT>`    | `auto`                                   | `auto`, `plain`, `json`; `auto` drops timestamps under journal |
| `-V`, `--version`       |                                          | Version and protocol version                                   |
| `-h`, `--help`          |                                          |                                                                |

`--only` and `--without` are debugging aids: they make it possible to run the daemon with just
`audio` while working on the audio service. A service excluded this way is absent from
`system.services` rather than reported as failed. The two are mutually exclusive; supplying both is
a usage error.

`--log-format auto` resolves to `plain` without timestamps or colour when `JOURNAL_STREAM` is set,
because the journal stamps its own lines, and to `plain` with both otherwise.

### Arguments

None.

### Environment

| Variable                                     | Use                                                                |
| -------------------------------------------- | ------------------------------------------------------------------ |
| `XDG_RUNTIME_DIR`                            | socket and tray icon directory; required                           |
| `XDG_CONFIG_HOME`, `XDG_CONFIG_DIRS`         | configuration lookup                                               |
| `GLIMPSE_CONFIG_PATH`                        | default for `--config`, overwrites config path                     |
| `GLIMPSE_SOCKET_PATH`                        | default for `--socket`, overwrites socket path                     |
| `WAYLAND_DISPLAY`                            | required for the `WaylandEdge` capabilities; absent means degraded |
| `DBUS_SESSION_BUS_ADDRESS`                   | session bus                                                        |
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

### Configuration

`$XDG_CONFIG_HOME/glimpse/config.toml`, shared with the UI binaries. glimpsed owns one table per
service; it ignores the tables the panel, wallpaper and lock own. The schema, the
layer stack, drop-ins, merge semantics and validation rules are all in
[`010_configuration.md`](010_configuration.md).

`--config`, `--check-config` and `--print-config` operate on the merged document defined there.

An invalid configuration never stops the daemon starting: it logs, falls back to defaults, and
logs the error. An invalid reload is dropped and the running configuration stays.

### Signals

| Signal    | Effect                                                                                                                                                   |
| --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SIGTERM` | graceful shutdown: stop accepting, drain writers, stop services in reverse DAG order                                                                     |
| `SIGINT`  | same as `SIGTERM`                                                                                                                                        |
| `SIGHUP`  | reload configuration per `010`; only services whose table changed are re-applied, and an invalid config is logged and dropped, keeping the running one |

### systemd integration

`Type=notify`. `READY=1` is sent once the socket is listening and every `OnBoot` service has reached
`ready` or `degraded`; `STOPPING=1` on shutdown; `WATCHDOG=1` from the broker loop, which means a
wedged broker is detected rather than silently stopping delivery. Details in `009_systemd.md`.

### Exit codes

| Code | Meaning                                                     |
| ---- | ----------------------------------------------------------- |
| 0    | clean shutdown, or `--check-config` / `--print-config` fine |
| 1    | configuration invalid, reported by `--check-config`         |
| 2    | usage error                                                 |
| 3    | socket already in use by a live daemon                      |
| 4    | `XDG_RUNTIME_DIR` unset or unusable                         |

Invalid configuration is not an exit. It logs and falls back to defaults at startup, and is
dropped on reload — see `010_configuration.md`.

### Observability

- `system.services` is a topic: the health of every registered service, so `glimpsectl services`
  needs no side channel.
- A configuration load that falls back to defaults, or a reload that is dropped, is logged at warn
  with the file, line and column of the problem. `glimpsectl config validate` re-checks on demand,
  so diagnosing a bad edit needs no extra topic.
- Backend call durations are logged separately from internal handling, so a slow tray is
  attributable to the application rather than to the daemon.

## Risks

- **Operational** — owning two well-known bus names means a crash loop removes tray and
  notifications for every application in the session. Backoff, health reporting and the watchdog are
  load-bearing, not decoration.

## Changelog

- 2026-08-20 — created, split out of `001_architecture.md`.
- 2026-08-20 — added the Configuration section: one `config.toml`, table ownership, per-service diffing, validation rules.
- 2026-08-20 — fixed `--config`'s default, which pointed at spec `004`.
- 2026-08-20 — clarified `--log-format auto`: plain without timestamps under a journal stream, not a separate `journal` value.
- 2026-08-20 — `--only` and `--without` are mutually exclusive; supplying both is a usage error.
- 2026-08-20 — configuration schema moved out to `010_configuration.md`; this spec keeps the flags, the file paths and the reload signal.
- 2026-08-20 — invalid configuration no longer exits at startup; exit 1 is now only `--check-config`.
