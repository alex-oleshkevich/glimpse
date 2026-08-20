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
| `$XDG_CONFIG_HOME/glimpse/glimpse.toml`           | user configuration       |
| `/etc/glimpse/glimpse.toml`                       | system configuration     |

### Configuration

`$XDG_CONFIG_HOME/glimpse/glimpse.toml`, read by all four binaries. Each reads only the tables it
owns.

| Table               | Owner             | Contents                                   |
| ------------------- | ----------------- | ------------------------------------------ |
| `[<service>]`       | glimpsed          | one per service, named for the service     |
| `[panel]`           | glimpse-panel     | layout, applet placement, behaviour        |
| `[wallpaper]`       | glimpse-wallpaper | source, mode, effect, per-output overrides |
| `[lock]`            | glimpse-lock      | appearance, widgets, grace period          |

Service tables sit at the top level, named for the service: `[weather]`, `[nightlight]`. A service
can never collide with a binary table, because `001_architecture.md` lists wallpaper, panel layout
and lock among the things glimpsed does not hold — so no service of those names will exist.

Stylesheets stay separate files — `panel.css`, `lock.css`. CSS is not TOML.

#### Ownership and validation

Two rules that pull in opposite directions, on purpose:

- **A reader rejects an unknown key inside a table it owns** (`deny_unknown_fields`). A typo in a
  hand-edited file must be loud.
- **A reader ignores the contents of tables it does not own.** glimpsed never learns the panel's
  schema, so the two version independently.

The set of _top-level_ table names is closed and lives in `glimpse-config`, which all four binaries
already link. That is what catches `[panle]`: without it a misspelled top-level table is ignored by
every reader and silently does nothing, which is the worst outcome for a file people edit by hand.

#### Layers

Later wins, per key:

| #   | Layer       | Source                                                            |
| --- | ----------- | ----------------------------------------------------------------- |
| 1   | defaults    | compiled in, mirrored for reference in `data/config/glimpse.toml` |
| 2   | system      | `/etc/glimpse/glimpse.toml` and its includes                      |
| 3   | user        | `$XDG_CONFIG_HOME/glimpse/glimpse.toml` and its includes          |
| 4   | environment | `GLIMPSE_<TABLE>__<KEY>`, double underscore between table and key |
| 5   | CLI         | flags                                                             |

There is no `.d/` drop-in layer. Includes cover the same need, and one mechanism beats two.

`--config <PATH>` replaces layers 2 and 3 with that file; its includes still resolve, and layers 1,
4 and 5 still apply. A `--config` run is a merged configuration, not a raw file read.

Merge semantics: **tables merge, scalars replace, arrays replace.** Appending would make a list
impossible to shorten from a later layer — an include could add an idle step but never remove one.

#### Includes

```toml
include = ["appearance.toml", "conf.d/*.toml"]
```

- Top level only, never inside a table. Include resolution stays a flat pre-pass.
- Paths resolve relative to the including file's directory; absolute paths are allowed.
- Globs expand in lexical order.
- Includes merge in listed order and **before** the including file's own keys, so the includer wins.
- Cycles are detected by canonical path and are a validation error naming both files.
- Nesting is capped at 8 levels.
- Every resolved include is watched, so editing a shared `appearance.toml` reloads the binaries that
  include it.

This is what lets the panel and the lock screen share appearance settings without either one owning
them, which is the only real cost of keeping the configuration in a single file.

#### Reload and per-service diffing

On `SIGHUP` the whole document is re-read, re-included, re-merged and re-validated. On failure the
running configuration survives untouched and the error location goes out on `config.reloaded`.

On success glimpsed does **not** restart services. For each registered service it deserializes that
service's table into its associated `Config` type, compares it against the running value, and calls
`Ctx::apply` only where the two differ. A service whose table did not change never learns that a
reload happened.

```rust
trait Service {
    type Config: DeserializeOwned + PartialEq + Default;
    fn apply(&mut self, config: &Self::Config) -> Result<(), ConfigError>;
}
```

This is the equality gate the daemon already applies to payloads, one level up. It is what makes a
shared file safe: editing `[panel]` cannot perturb the nightlight schedule, because nightlight's
subtree is unchanged and `apply` is never called.

A service with no table of its own gets `Config::default()`.

#### `[geolocation]`

| Key         | Type  | Default  | Meaning                           |
| ----------- | ----- | -------- | --------------------------------- |
| `source`    | enum  | `"auto"` | `auto`, `geoclue`, `manual`       |
| `latitude`  | float | —        | required when `manual`, −90..90   |
| `longitude` | float | —        | required when `manual`, −180..180 |

`auto` uses GeoClue2 and falls back to the manual pair when it is absent. That fallback is what keeps
nightlight and weather working on a machine with no location service.

#### `[nightlight]`

| Key                  | Type    | Default   | Meaning                                          |
| -------------------- | ------- | --------- | ------------------------------------------------ |
| `enabled`            | bool    | `true`    |                                                  |
| `mode`               | enum    | `"solar"` | `solar`, `manual`, `always`, `off`               |
| `day_temperature`    | integer | `6500`    | kelvin, 1000..10000                              |
| `night_temperature`  | integer | `4000`    | kelvin, 1000..10000, must be ≤ `day_temperature` |
| `transition_minutes` | integer | `30`      | 0..240                                           |
| `sunrise`            | string  | `"07:00"` | `HH:MM` local, used when `mode = "manual"`       |
| `sunset`             | string  | `"20:00"` | `HH:MM` local, used when `mode = "manual"`       |

`solar` needs `geolocation.position`. Without it the service reports `degraded` rather than guessing
a location.

#### `[theme]`

| Key    | Type | Default        | Meaning                       |
| ------ | ---- | -------------- | ----------------------------- |
| `mode` | enum | `"nightlight"` | `nightlight`, `light`, `dark` |

`nightlight` derives `theme.mode` from `nightlight.state`; the other two pin it. Panel, wallpaper and
lock all read the resulting topic, which is why this is daemon configuration rather than UI
configuration.

#### `[weather]`

| Key            | Type    | Default        | Meaning                                         |
| -------------- | ------- | -------------- | ----------------------------------------------- |
| `provider`     | enum    | `"open-meteo"` |                                                 |
| `units`        | enum    | `"metric"`     | `metric`, `imperial`                            |
| `poll_minutes` | integer | `30`           | 10..1440                                        |
| `location`     | enum    | `"auto"`       | `auto` follows `geolocation.position`           |
| `latitude`     | float   | —              | required when `location = "manual"`             |
| `longitude`    | float   | —              | required when `location = "manual"`             |
| `api_key_file` | path    | —              | file holding the key, for providers needing one |

The key is a path, never an inline string: configuration files land in dotfile repositories, and
`--print-config` writes to stdout. `open-meteo` is the default precisely because it needs no key.

`poll_minutes` has a floor because the service is `OnDemand`. A panel that subscribes on hover must
not be able to drive a request per second.

#### `[notifications]`

| Key                      | Type    | Default | Meaning                                              |
| ------------------------ | ------- | ------- | ---------------------------------------------------- |
| `default_expiry_seconds` | integer | `5`     | applied when the sender passes `expire_timeout = -1` |
| `max_stored`             | integer | `100`   | history cap, 0..1000                                 |
| `do_not_disturb`         | bool    | `false` | the value at boot only                               |

`do_not_disturb` is a boot default, not live state. Toggling it at runtime is a command and the
override lands in the state directory: glimpsed never writes this file back.

```toml
[[notifications.rules]]
app = "Spotify"       # matches the sender's app_name, exact, case-insensitive
suppress = false
expiry_seconds = 2
```

`app_name` is attacker-controlled, so a rule is a display and expiry hint only. Nothing in a rule may
grant a notification more privilege than it started with.

#### `[idle]`

| Key                  | Type | Default | Meaning                           |
| -------------------- | ---- | ------- | --------------------------------- |
| `enabled`            | bool | `true`  |                                   |
| `respect_inhibitors` | bool | `true`  | honour logind and idle inhibitors |

```toml
[[idle.steps]]
timeout_seconds = 300
action = "dim"        # dim, screen_off, lock, suspend
```

Steps sort by `timeout_seconds` on load; duplicate timeouts are a validation error. `lock` starts
`glimpse-lock.service` and `suspend` calls logind. Neither shells out.

#### `[clipboard]`

| Key                        | Type    | Default   | Meaning                                                 |
| -------------------------- | ------- | --------- | ------------------------------------------------------- |
| `enabled`                  | bool    | `true`    |                                                         |
| `max_entries`              | integer | `100`     | 0..1000                                                 |
| `max_entry_bytes`          | integer | `1048576` | larger entries are recorded by size only                |
| `ignore_password_managers` | bool    | `true`    | drop selections carrying the password-manager MIME hint |

There is no `persist` key and there will not be one. History lives in `$XDG_RUNTIME_DIR/glimpse/` and
dies with the session, because glimpsed has nowhere else to write.

#### `[sysstats]`

| Key            | Type    | Default | Meaning                |
| -------------- | ------- | ------- | ---------------------- |
| `poll_seconds` | integer | `2`     | 1..60                  |
| `disks`        | array   | `["/"]` | mount points to report |

#### `[brightness]`

| Key      | Type   | Default  | Meaning                                        |
| -------- | ------ | -------- | ---------------------------------------------- |
| `device` | string | `"auto"` | backlight device name, or `auto` for the first |

One of only two mirror services with configuration. Choosing among several backlights on a laptop
with a discrete GPU is not a decision logind or sysfs makes for us.

#### `[power]`

| Key             | Type | Default | Meaning                               |
| --------------- | ---- | ------- | ------------------------------------- |
| `lock_on_sleep` | bool | `false` | start the locker on `PrepareForSleep` |

Off by default. `009_systemd.md` prefers `systemd-lock-handler`, which keeps locking working while
glimpsed is down. This is the fallback for a system without it; running both locks twice.

#### Services with no configuration

`tray`, `audio`, `network`, `bluetooth`, `battery`, `mpris`, `workspaces`, `keyboard`. Every one is a
mirror whose backend owns the state and the policy. A key here would mean reimplementing a decision
NetworkManager, BlueZ, PipeWire or the compositor already makes.

These names are not in the closed set of top-level tables, so writing `[tray]` is a validation error
rather than a table that is silently ignored. The message says the service takes no configuration,
not that the table is unknown — the user has named something real.

#### Validation

`--check-config` reports every problem it finds rather than the first, and exits 1 if there are any:

- unknown top-level table, with the nearest known one as a suggestion
- unknown key inside an owned table, likewise
- wrong type, or a value outside a documented range
- a conditional requirement unmet, such as `source = "manual"` with no coordinate pair
- `night_temperature` above `day_temperature`
- duplicate `timeout_seconds` across `[[idle.steps]]`
- `api_key_file` missing or unreadable
- an include cycle, a missing include, or nesting past 8 levels

Each problem carries file, line and column, naming the file the error is actually in rather than the
one that included it.

#### The cost of a single file

A TOML **syntax** error anywhere in the merged document fails every binary's parse, not only the
owner's. At boot that is exit 1 with a precise location; on reload every binary keeps what it is
already running. This is the price of one file and it is accepted deliberately: per-key ownership
and per-service diffing contain everything above the syntax layer, and a syntax error is the one
class of mistake `--check-config` catches before a restart.

### Signals

| Signal    | Effect                                                                                                                                                   |
| --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SIGTERM` | graceful shutdown: stop accepting, drain writers, stop services in reverse DAG order                                                                     |
| `SIGINT`  | same as `SIGTERM`                                                                                                                                        |
| `SIGHUP`  | reload configuration; only services whose table changed are re-applied, and an invalid config keeps the running one and is reported on `config.reloaded` |

### systemd integration

`Type=notify`. `READY=1` is sent once the socket is listening and every `OnBoot` service has reached
`ready` or `degraded`; `STOPPING=1` on shutdown; `WATCHDOG=1` from the broker loop, which means a
wedged broker is detected rather than silently stopping delivery. Details in `009_systemd.md`.

### Exit codes

| Code | Meaning                                                     |
| ---- | ----------------------------------------------------------- |
| 0    | clean shutdown, or `--check-config` / `--print-config` fine |
| 1    | configuration invalid                                       |
| 2    | usage error                                                 |
| 3    | socket already in use by a live daemon                      |
| 4    | `XDG_RUNTIME_DIR` unset or unusable                         |

### Observability

- `system.services` is a topic: the health of every registered service, so `glimpsectl services`
  needs no side channel.
- `config.reloaded` carries the outcome of the last reload, including the exact location of a
  parse or validation error.
- Backend call durations are logged separately from internal handling, so a slow tray is
  attributable to the application rather than to the daemon.

## Risks

- **Operational** — owning two well-known bus names means a crash loop removes tray and
  notifications for every application in the session. Backoff, health reporting and the watchdog are
  load-bearing, not decoration.

## Changelog

- 2026-08-20 — created, split out of `001_architecture.md`.
- 2026-08-20 — added the Configuration section: one `glimpse.toml`, includes, table ownership, per-service diffing, validation rules.
- 2026-08-20 — fixed `--config`'s default, which pointed at spec `004`.
- 2026-08-20 — clarified `--log-format auto`: plain without timestamps under a journal stream, not a separate `journal` value.
- 2026-08-20 — `--only` and `--without` are mutually exclusive; supplying both is a usage error.
