---
state: draft
---

# 007 — glimpsectl

The command-line and TUI client: read topics, watch them, invoke commands, inspect the daemon.

## Problem

Everything the daemon knows is reachable only over a socket that speaks a private protocol. Without
a client, debugging means `socat` and hand-written JSON, and scripting the desktop is not possible
at all. The daemon also needs an inspection surface — which services are degraded, why a config
reload failed — that is not a UI feature.

## Goals

- Read or watch any topic from a shell script with no JSON assembly.
- Invoke any command with readable arguments.
- Make the daemon's health legible without reading logs.
- Be the first working client, so the protocol is exercised before any GTK exists.

## Non-goals

- No configuration editing. It reads and validates; the user's editor writes.
- No daemon lifecycle management. That is `systemctl --user`.

## Tech

### Invocation

```
glimpsectl [GLOBAL OPTIONS] <COMMAND> [ARGS]
```

### Global flags

| Flag              | Default                                  | Purpose                                          |
| ----------------- | ---------------------------------------- | ------------------------------------------------ |
| `--socket <PATH>` | `$XDG_RUNTIME_DIR/glimpse/glimpsed.sock` | Daemon socket                                    |
| `--timeout <MS>`  | `5000`                                   | Per-request timeout                              |
| `--no-color`      | auto                                     | Disable colour; also honours `NO_COLOR`          |
| `-V`, `--version` |                                          | Version and protocol version                     |
| `-h`, `--help`    |                                          |                                                  |

### Subcommands

| Command                              | Arguments                        | Behaviour                                                                 |
| ------------------------------------ | -------------------------------- | ------------------------------------------------------------------------- |
| `get <TOPIC>`                        | one topic, exact                 | Print the current value, exit. `--field <PATH>` prints one field; `--json` prints the payload verbatim |
| `watch <PATTERN>`                    | one pattern, `audio.*`, `tray.**`| Print the snapshot then every update, one per line, until interrupted. `--count <N>` exits after N events; `--json` prints each event frame verbatim |
| `call <METHOD> [KEY=VALUE]...`       | method name then arguments       | Invoke a command, print the result. Values parse as JSON when possible, otherwise as strings |
| `topics [PATTERN]`                   | optional filter                  | List known topics with their owning service and whether a value is present. `--owner <SERVICE>` keeps only that service's topics |
| `services`                           | none                             | List services with state, health and the reason for `degraded`            |
| `config show`                        | none                             | Print the merged configuration — the same layered stack `config path` resolves and `config validate` checks |
| `config validate [PATH]`             | optional file                    | Validate a file, or the layered stack, and report the exact error location |
| `config path`                        | none                             | Print which files the layered stack resolved to, in order, drop-ins included |
| `doctor`                             | none                             | Check environment: socket, compositor, Wayland protocols, session bus, backends. Report what is missing and what degrades as a result |
| `monitor`                            | none                             | Interactive TUI: topic browser, live values, service health                |

`call` argument parsing, worked example:

```bash
glimpsectl call audio.set_volume volume=0.42
glimpsectl call nightlight.set_mode mode=auto
glimpsectl call tray.activate item=nextcloud
glimpsectl get battery.status --field percentage
glimpsectl watch 'network.**' | while read -r topic values; do ...; done
```

### Environment

| Variable          | Use                                       |
| ----------------- | ----------------------------------------- |
| `XDG_RUNTIME_DIR` | default socket path                        |
| `NO_COLOR`        | disable colour                             |
| `GLIMPSE_SOCKET_PATH` | default socket path, overridden by `--socket` |
| `CLICOLOR`, `CLICOLOR_FORCE`, `TERM` | colour detection, per the `anstream` conventions |

### Output conventions

- Human output is aligned and coloured when stdout is a terminal; piping switches to plain.
- Rendered output is for people. `get` and `watch` also take `--json`, which prints what the daemon
  sent and nothing else: the payload for `get`, one event frame per line for `watch`. It is not a
  formatting choice but a passthrough, so `jq` sees exactly what crossed the socket. No other
  command has it — `topics` and `services` render the daemon's own introspection, and a consumer
  wanting those as data should read `system.topics` and `system.services` with `get --json`.
- Errors go to stderr; only requested data goes to stdout.

### Exit codes

| Code | Meaning                                          |
| ---- | ------------------------------------------------ |
| 0    | success                                           |
| 1    | the command failed (daemon returned an error)     |
| 2    | usage error                                       |
| 3    | daemon unreachable                                 |
| 4    | unknown topic or method                            |
| 5    | timeout                                            |
| 6    | protocol version mismatch                          |

A failed `call` propagates the daemon's `retryable` flag into the message, so a script can tell a
transient failure from a permanent one without parsing prose.

## Risks

- **Technical** — `watch` is the one place where a slow consumer matters on the client side. A
  script that reads slowly must not force the daemon to buffer without bound; the coalescing rules
  apply the same as for any client.

## Changelog

- 2026-08-20 — created, split out of `001_architecture.md`.
- 2026-08-20 — `config path` lists every file in the resolved stack, drop-ins included; with one shared `config.toml` it is the only way to see where a value came from.
- 2026-08-20 — example updated: `nightlight.set_mode` takes `auto`; `solar` no longer exists.
- 2026-08-20 — `GLIMPSE_SOCKET` renamed to `GLIMPSE_SOCKET_PATH`, matching `003_daemon.md` and `004_panel.md`; it also pairs with `GLIMPSE_CONFIG_PATH`.
- 2026-08-21 — colour detection follows the `anstream` conventions, so `CLICOLOR`, `CLICOLOR_FORCE` and `TERM` are honoured alongside `NO_COLOR`.
- 2026-08-21 — `config show` reads the layered stack from disk, the same as `config path` and `config validate`; it never needed the daemon running.
- 2026-08-21 — `config validate`, running against a real config on this machine, is what surfaced that `[applets.<name>.settings]` nesting (see `010`) was never how `_old/` actually worked.
- 2026-08-26 — `--json` removed; `glimpsectl` renders for people only, and `get --field` is the scripting primitive. See the decision log.
- 2026-08-26 — `--json` returns as a per-command flag on `get` and `watch`, printing the daemon's bytes verbatim rather than a second rendering. See the decision log.
- 2026-08-26 — `topics --owner <SERVICE>` filters by owning service, the other axis of the table it already prints.
