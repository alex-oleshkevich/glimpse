---
state: draft
---

# 010 — Configuration

One TOML file, shared by every binary, with a top-level table per owner.

## Problem

Four binaries need settings, and the obvious layout — one file each — was the original design. It
duplicates every cross-cutting choice: the panel and the lock screen both render an accent colour, a
font and a clock, and neither owns those values. It also gives a user four places to look and four
files to keep in step.

Merging them creates the opposite problem. If one file is parsed by four programs, either each
program has to know all four schemas in order to reject a typo, or nobody rejects typos and a
misspelled key silently does nothing. And a single file means one binary's mistake reaches the
others — including the daemon, which owns tray and notifications for the whole session.

## Goals

- One file to find and edit, with drop-ins for what a machine or a package needs to add on top.
- A typo is an error the user sees, without any binary learning another's schema.
- Editing one binary's settings does not disturb another's, and does not restart a service whose
  own settings did not change.
- An invalid edit never replaces a working configuration, and never stops a binary starting.
- Every value has a documented type, default and range, so `--check-config` can be exhaustive.

## Non-goals

- No configuration writing by any glimpse process. The file belongs to the user.
- No settings UI. Hot reload plus a text editor is the interaction model.
- No daemon runtime settings. Log level and format come from `--log`, `--log-format` or `RUST_LOG`,
  persisted with a systemd drop-in; the socket path is fixed by the unit's `RuntimeDirectory=`.
  Every top-level table belongs to a service or to a UI binary.
- No per-binary config files. That is what this spec replaces.

## Tech

### The file

`$XDG_CONFIG_HOME/glimpse/config.toml`, read by all four binaries. Each reads only the tables it
owns.

| Entry                            | Owner             | Contents                                   |
| -------------------------------- | ----------------- | ------------------------------------------ |
| `theme`, `theme_mode`            | glimpsed          | the two top-level scalars                  |
| `[<service>]`                    | glimpsed          | one per service, named for the service     |
| `[monitors]`                     | glimpsed          | session hardware, read by several services |
| `[[panels]]`, `[applets.<name>]` | glimpse-panel     | bars, zones, applet instances              |
| `[wallpaper]`, `[backdrop]`      | glimpse-wallpaper | image, fit, transition, overview backdrop  |
| `[lock]`                         | glimpse-lock      | PAM service, background, clock, controls   |

Service tables sit at the top level, named for the service as existing configurations already spell
it — `[night_light]` for the nightlight service and `[location]` for geolocation, both kept because
renaming them would break every file in the wild. A service
can never collide with a binary table, because `001_architecture.md` lists wallpaper, panel layout
and lock among the things glimpsed does not hold — so no service of those names will exist.

Stylesheets stay separate files under `themes/`, not in the configuration: `themes/panel.css` is a
user override layered over the active theme pack's own, and the locker's is wherever `lock.css_path`
points, `themes/lock.css` by default. CSS is not TOML.

### Ownership and validation

Two rules that pull in opposite directions, on purpose:

- **A reader rejects an unknown key inside a table it owns** (`deny_unknown_fields`). A typo in a
  hand-edited file must be loud.
- **A reader ignores the contents of tables it does not own.** glimpsed never learns the panel's
  schema, so the two version independently.

The set of _top-level_ table names is closed and lives in `glimpse-config`, which all four binaries
already link. That is what catches `[panle]`: without it a misspelled top-level table is ignored by
every reader and silently does nothing, which is the worst outcome for a file people edit by hand.

### Layers

Later wins, per key:

| #   | Layer           | Source                                                            |
| --- | --------------- | ----------------------------------------------------------------- |
| 1   | defaults        | compiled in, mirrored for reference in `data/config.default.toml` |
| 2   | system          | `/etc/glimpse/config.toml`                                        |
| 3   | system drop-ins | `/etc/glimpse/config.d/*.toml`, lexical order                     |
| 4   | user            | `$XDG_CONFIG_HOME/glimpse/config.toml`                            |
| 5   | user drop-ins   | `$XDG_CONFIG_HOME/glimpse/config.d/*.toml`, lexical order         |
| 6   | CLI             | flags                                                             |

`--config <PATH>` replaces layers 2 through 5 with that file. Layers 1 and 6 still apply, so a
`--config` run is still a merged configuration and not a raw file read.

Merge semantics: **tables merge, scalars replace, arrays replace.** Appending would make a list
impossible to shorten from a later layer — a drop-in could add an idle step but never remove one.

### Drop-ins

```
~/.config/glimpse/
├── config.toml
└── config.d/
    ├── 10-laptop.toml
    └── 20-work.toml
```

A drop-in is an ordinary config file holding a fragment. It merges over the base file at the same
level, by the same per-key rules, so a drop-in overrides a key without restating the table around
it.

- `*.toml` only; other extensions are ignored, so editor backups and `.disabled` files cost nothing.
- Lexical order by filename, which is why the systemd-style numeric prefix is the convention.
- One level deep. There is no `config.d/*/`.
- The directory is watched for files appearing and disappearing, not only for edits, so adding a
  drop-in reloads without a restart.
- A missing `config.d/` is normal, not an error. So is an empty one.

Drop-ins exist for the overlay case: a machine-specific tweak, a package shipping a default, or a
setting a user wants outside the file their dotfiles manage. They are not a way to split a large
configuration by topic — that is what the base file's tables are for.

### Path resolution

Symlinks are followed. `~/.config/glimpse/config.toml` pointing into a dotfile repository is the
common case — stow, chezmoi and hand-rolled symlink farms all produce it — so refusing links would
break more setups than it protects. The same goes for `config.d/` itself being a link, and for
individual drop-ins inside it.

What is checked is what the link lands on, and how:

- **Open first, then inspect the descriptor.** Never stat a path and then open it: between the two
  the path can be replaced, and the file that was checked is not the file that is read.
- **Regular files only.** After resolution, anything else is refused. A FIFO is the one that matters:
  opening it blocks until a writer appears, which would hang startup past the unit's timeout and
  breaks the rule that nothing in the daemon blocks. A character device such as `/dev/zero` reads
  without end. Directories and sockets are refused for the same reason — they are never what the
  user meant.
- **A size cap of 1 MiB per file**, applied to the descriptor rather than to a prior `stat`. No
  legitimate configuration approaches it, and it bounds what a mistaken link can pull into memory.
- **Symlink loops** surface as `ELOOP` and are reported as an unreadable file, not retried.

A dangling or unresolvable drop-in is **skipped with a warning**, not a failure. A stale link left by
an uninstalled package must not cost the user their session. The base file is different: if it
resolves to nothing readable, that is a load failure and the defaults rule applies.

Errors name the path as written and the path it resolved to, and never any of the file's content. A
link aimed at a private file — an SSH key, a token — must not echo that file into the journal, into
the log, or into `--print-config` output. This is the reason resolution is specified at
all: glimpsed runs unprivileged, so a link cannot reach anything the user could not already read,
but it can trick the daemon into reprinting it somewhere more public than where it started.

### Bounds

Nothing in the stack traverses without a bound, and nothing holds descriptors it is not using.

- **No directory recursion.** `config.d/` is read exactly one level deep. A subdirectory inside it is
  ignored rather than descended into, and a `config.d/` that resolves to a link pointing at another
  directory is still read one level. No configuration shape needs a tree, so there is no traversal
  to bound in the first place.
- **One file open at a time.** Each drop-in is opened, read to the cap, and closed before the next is
  opened. Peak descriptor use for a whole load is two — the directory being listed and the file
  being read — no matter how many drop-ins exist. The merged document is built in memory; the files
  are not held.
- **At most 64 drop-ins per directory.** Past that the load fails rather than silently applying a
  prefix of them, because a user who wrote a drop-in must never have it quietly ignored. With the
  1 MiB per-file cap this bounds a load at 64 MiB read sequentially, and a real one at a few KiB.
- **Watches are per directory, never per file**, and the `watcher` service of `011_watcher.md`
  places them; a directory missing at start or recreated later is its problem, defined there. Four
  cover the whole stack: `/etc/glimpse/`,
  `/etc/glimpse/config.d/`, `$XDG_CONFIG_HOME/glimpse/` and its `config.d/`. A hundred drop-ins cost
  the same as one. Per-file watches would also be wrong on their own terms — they cannot see a
  drop-in that does not exist yet, and adding one is exactly the event that has to trigger a reload.
- **A symlinked base file adds one more watch**, on the directory holding its resolved target.
  Editors write a new file and rename it over the old one, so a watch that followed the link to the
  original inode would go quiet after the first save. Total watches are bounded at six.
- **Symlink chains** are bounded by the kernel, surfacing as `ELOOP`. The daemon adds no depth limit
  of its own because it never resolves links by hand.

If watch registration fails — `fs.inotify.max_user_watches` is a shared and commonly exhausted
resource — the daemon logs it once and runs without hot reload. Configuration still loads at start
and `SIGHUP` still works. Losing automatic reload is a degradation; refusing to start over it would
not be.

### Reload and per-service diffing

On `SIGHUP` the whole stack is re-read, re-merged and re-validated. On failure the
running configuration survives untouched and the error location is logged.

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
shared file safe: editing `[[panels]]` cannot perturb the night light schedule, because its
subtree is unchanged and `apply` is never called.

A service with no table of its own gets `Config::default()`.

### `[appearance]`

| Key      | Type   | Default  | Meaning                                   |
| -------- | ------ | -------- | ----------------------------------------- |
| `pack`   | string | `""`     | theme pack name, resolved under `themes/` |
| `scheme` | enum   | `"auto"` | `light`, `dark`, `auto`                   |

| Key          | Type   | Default  | Meaning                                     |
| ------------ | ------ | -------- | ------------------------------------------- |
| `theme`      | string | `""`     | theme pack name, resolved under `themes/`   |
| `theme_mode` | enum   | `"auto"` | `light`, `dark`, `auto`                     |

They stay scalars because that is what existing configurations contain. Folding them into a `[theme]`
table would be tidier and would break every file in the wild, which is the trade this spec refuses.

`theme_mode = "auto"` is what the `theme` service resolves against `night_light.state`; the other
two pin it. A `[[panels]]` entry may override it per bar.

### `[location]`

| Key         | Type  | Default  | Meaning                           |
| ----------- | ----- | -------- | --------------------------------- |
| `source`    | enum  | `"auto"` | `auto`, `geoclue`, `manual`       |
| `latitude`  | float | —        | required when `manual`, −90..90   |
| `longitude` | float | —        | required when `manual`, −180..180 |

`auto` uses GeoClue2 and falls back to the manual pair when it is absent. That fallback is what keeps
nightlight and weather working on a machine with no location service.

### `[nightlight]`

| Key                  | Type    | Default  | Meaning                                          |
| -------------------- | ------- | -------- | ------------------------------------------------ |
| `mode`               | enum    | `"auto"` | `auto`, `manual`, `off`                          |
| `day_temperature`    | integer | `6500`   | kelvin, 1000..10000                              |
| `night_temperature`  | integer | `4000`   | kelvin, 1000..10000, must be ≤ `day_temperature` |
| `transition_minutes` | integer | `30`     | 0..240                                           |
| `activate_at`        | string  | —        | `HH:MM` local, required when `mode = "manual"`   |
| `deactivate_at`      | string  | —        | `HH:MM` local, required when `mode = "manual"`   |

| Mode     | Behaviour                                                   |
| -------- | ----------------------------------------------------------- |
| `auto`   | Follows the sun, from location service                      |
| `manual` | Follows the two configured times, with no location involved |
| `off`    | Never warms the screen and never actuates gamma             |

`auto` needs location, which `[location]` supplies from GeoClue2 or from a manual
coordinate pair. Without one the service reports `degraded` rather than guessing a location.

`activate_at` and `deactivate_at` have no defaults. `manual` means the user states both, and
inventing times would warm the screen on a schedule nobody chose. The pair wraps midnight, which is
the ordinary case: `activate_at = "20:00"` with `deactivate_at = "07:00"` is one night, not an empty
window.

They are named for the feature, not for the sun. Nightlight is active _between_ sunset and sunrise,
so a `sunrise`/`sunset` pair has to be read inverted at every use.

There is no `enabled` key and no `always` mode. `off` is what disabling looks like, and a schedule
that never lapses is `manual` with a window covering the day.

### `[theme]`

| Key    | Type | Default        | Meaning                       |
| ------ | ---- | -------------- | ----------------------------- |
| `mode` | enum | `"nightlight"` | `nightlight`, `light`, `dark` |

`nightlight` derives `theme.mode` from `nightlight.state`; the other two pin it. Panel, wallpaper and
lock all read the resulting topic, which is why this is daemon configuration rather than UI
configuration.

### `[weather]`

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

### `[notifications]`

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

### `[idle]`

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

### `[clipboard]`

| Key                        | Type    | Default   | Meaning                                                 |
| -------------------------- | ------- | --------- | ------------------------------------------------------- |
| `enabled`                  | bool    | `true`    |                                                         |
| `max_entries`              | integer | `100`     | 0..1000                                                 |
| `max_entry_bytes`          | integer | `1048576` | larger entries are recorded by size only                |
| `ignore_password_managers` | bool    | `true`    | drop selections carrying the password-manager MIME hint |

There is no `persist` key and there will not be one. History lives in `$XDG_RUNTIME_DIR/glimpse/` and
dies with the session, because glimpsed has nowhere else to write.

### `[sysstats]`

| Key            | Type    | Default | Meaning                |
| -------------- | ------- | ------- | ---------------------- |
| `poll_seconds` | integer | `2`     | 1..60                  |
| `disks`        | array   | `["/"]` | mount points to report |

### `[brightness]`

| Key      | Type   | Default  | Meaning                                        |
| -------- | ------ | -------- | ---------------------------------------------- |
| `device` | string | `"auto"` | backlight device name, or `auto` for the first |

One of only two mirror services with configuration. Choosing among several backlights on a laptop
with a discrete GPU is not a decision logind or sysfs makes for us.

### `[power]`

| Key             | Type | Default | Meaning                               |
| --------------- | ---- | ------- | ------------------------------------- |
| `lock_on_sleep` | bool | `false` | start the locker on `PrepareForSleep` |

Off by default. `009_systemd.md` prefers `systemd-lock-handler`, which keeps locking working while
glimpsed is down. This is the fallback for a system without it; running both locks twice.

### Services with no configuration

`tray`, `audio`, `network`, `bluetooth`, `battery`, `mpris`, `workspaces`, `keyboard`. Every one is a
mirror whose backend owns the state and the policy. A key here would mean reimplementing a decision
NetworkManager, BlueZ, PipeWire or the compositor already makes.

These names are not in the closed set of top-level tables, so writing `[tray]` is a validation error
rather than a table that is silently ignored. The message says the service takes no configuration,
not that the table is unknown — the user has named something real.

### Validation

`--check-config` reports every problem it finds rather than the first, and exits 1 if there are
any. The same checks run at startup and on reload, where they feed the load-failure rule instead:

- unknown top-level table, with the nearest known one as a suggestion
- unknown key inside an owned table, likewise
- wrong type, or a value outside a documented range
- a conditional requirement unmet, such as `source = "manual"` with no coordinate pair
- `night_temperature` above `day_temperature`
- duplicate `timeout_seconds` across `[[idle.steps]]`
- `api_key_file` missing or unreadable
- a drop-in that cannot be read
- a path that resolves to something other than a regular file, or past the 1 MiB cap
- more than 64 drop-ins in one directory

Each problem carries file, line and column, naming the drop-in the error is in rather than the base
file it merges over.

### Load failure

A bad edit never costs the user their session. Two cases, one rule each:

| When            | Behaviour                                                              |
| --------------- | ---------------------------------------------------------------------- |
| **Fresh start** | Log the error with its location, start with defaults for every table   |
| **Reload**      | Log the error with its location, drop the update, keep what is running |

Neither case exits. A binary that refuses to start because of a typo leaves the user with no shell
and no obvious way to fix it — the panel is where they would have read the error. Defaults always
produce a working session, which is the state a user can recover from.

Fallback is whole-document, not per table. A partially applied configuration is harder to notice
than a wholly ignored one: with defaults everywhere the failure is unmistakable at a glance, and the
user fixes one thing instead of hunting for which half took effect.

Both cases log at warn with the file, line and column of the problem, which is the channel
`009_systemd.md` already commits to: `journalctl --user -u glimpsed` is enough to diagnose a
failure. `glimpsectl config validate` re-checks the stack on demand and reports the same locations,
so nothing has to be running for a user to find out what is wrong with their file.

`--check-config` is the exception and the reason it exists: it is a validation tool, so it reports
every problem and exits 1. Normal startup never does.

### The cost of a single file

A TOML **syntax** error anywhere in the merged document fails every binary's parse, not only the
owner's — one table's stray bracket costs every binary its configuration, where four files would
have cost one.

What that is worth is bounded by the load-failure rule above. At boot every binary starts on
defaults instead of the user's settings; on reload every binary keeps what it is already running.
Nothing exits and nothing is lost. The remaining cost is that the blast
radius of a syntax error is the whole document rather than one table, which `--check-config` catches
before a restart and which the editor catches before that.

## Alternatives considered

- **One file per binary** — rejected: the panel and the lock screen duplicate every appearance
  value, and a user has four files to keep in step. One file removes the duplication outright.
- **`[services.<name>]` prefixing** — rejected: a service can never collide with a binary table,
  because `001_architecture.md` puts wallpaper, panel layout and lock among the things glimpsed does
  not hold. The prefix bought nothing and cost a level of nesting in every table name.
- **An `include = [...]` key instead of `config.d/`** — rejected: includes were designed to let the
  panel and the lock share an appearance file back when each had its own config file. Merging into
  one `config.toml` made them tables in the same document, which removed the reason. Drop-ins cover
  the overlay case that remains, and need no cycle detection, depth cap or path resolution.
- **Arrays that append across layers** — rejected: an appending array can never be shortened by a
  later layer, so a drop-in could add an idle step but never remove one.
- **Exiting at boot on invalid configuration** — rejected: it leaves the user with no panel, which
  is where they would have seen the error, and a session they cannot fix without another machine or
  a TTY. Defaults plus a logged error keeps the shell usable.
- **Per-table fallback, keeping the tables that did validate** — rejected: half-applied settings are
  harder to spot than none, and the user ends up bisecting their own file. Whole-document fallback
  fails obviously.
- **Environment-variable overrides** (`GLIMPSE_<TABLE>__<KEY>`) — rejected: they cannot express an
  array of tables such as `[[idle.steps]]`, so they would cover scalars only; every value arrives as
  a string and needs coercion rules TOML gives for free; and `GLIMPSE_*` already holds path
  overrides like `GLIMPSE_CONFIG_PATH` that are not config values. A `config.d/` drop-in does the
  same job with the whole schema, real types and validation.
- **Refusing symlinked configuration** — rejected: dotfile managers symlink
  `~/.config/glimpse/config.toml` into a repository, which is the ordinary setup rather than an
  attack. Links are followed; what they resolve to is checked.
- **Inline API keys** — rejected: `--print-config` writes the merged document to stdout. Keys are
  paths to files.

## Changelog

- 2026-08-20 — created, split out of `003_glimpsed.md`.
- 2026-08-20 — added the load-failure rule: invalid config logs and falls back to defaults at boot, and is dropped on reload. Neither exits; `--check-config` still exits 1.
- 2026-08-20 — the file is `config.toml`; dropped `[daemon]`, so every top-level table belongs to a service or a UI binary and logging is configured only by flag, `RUST_LOG` or a unit drop-in.
- 2026-08-20 — replaced `include = [...]` with `config.d/` drop-ins; includes solved the four-file sharing problem, which merging into one file already solved.
- 2026-08-20 — dropped the environment layer; `config.d/` covers the override case with the full schema and real types.
- 2026-08-20 — specified path resolution: symlinks followed, regular files only, open-then-inspect, 1 MiB cap, and no file content in error messages.
