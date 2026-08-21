---
state: draft
---

# 010 — Configuration

One TOML file, shared by every binary, with a top-level table per owner.

## Problem

One `config.toml` has always held every setting, and it was read by one process. The split into a
daemon and three UI binaries turns it into a file with four readers, and that is what needs
deciding: one file with four readers, or four files with one each.

Four files remove the shared blast radius and add duplication in its place — the panel and the lock
screen both render an accent colour, a font and a clock, and neither owns those values — plus four
places for a user to look. They would also break every configuration that exists.

Keeping one file raises the question four readers create. Either every reader knows all four schemas
in order to reject a typo, or nobody rejects typos and a misspelled key silently does nothing. And
one file means one binary's mistake reaches the others, including the daemon that owns tray and
notifications for the whole session.

## Goals

- One file to find and edit, with drop-ins for what a machine or a package needs to add on top.
- A typo is an error the user sees, without any binary learning another's schema.
- Editing one binary's settings does not disturb another's, and does not restart a service whose
  own settings did not change.
- An invalid edit never replaces a working configuration, and never stops a binary starting.
- Every value has a documented type and default, and a range wherever one applies, so
  `--check-config` can be exhaustive.

## Non-goals

- No configuration writing by any glimpse process. The file belongs to the user.
- No settings UI. Hot reload plus a text editor is the interaction model.
- No daemon runtime settings. Log level and format come from `--log`, `--log-format` or `RUST_LOG`,
  persisted with a systemd drop-in; the socket path is fixed by the unit's `RuntimeDirectory=`.
- No second configuration file per binary, and no migration to one. There has only ever been
  `config.toml`, and it stays that way.

## Tech

### The file

`$XDG_CONFIG_HOME/glimpse/config.toml`, read by all four binaries. Each reads only the tables it
owns.

| Entry                            | Owner             | Contents                                   |
| -------------------------------- | ----------------- | ------------------------------------------ |
| `[appearance]`                   | glimpsed          | theme pack and light/dark scheme           |
| `[<service>]`                    | glimpsed          | one per service, named for the service     |
| `[monitors]`                     | glimpsed          | session hardware, read by several services |
| `[[panels]]`, `[applets.<name>]` | glimpse-panel     | bars, zones, applet instances              |
| `[wallpaper]`, `[backdrop]`      | glimpse-wallpaper | image, fit, transition, overview backdrop  |
| `[lock]`                         | glimpse-lock      | PAM service, background, clock, controls   |

Service tables sit at the top level, named for the service as existing configurations already spell
it — `[night_light]` for the nightlight service and `[location]` for geolocation, both kept because
renaming them would break every file in the wild.

A service can never collide with a UI table, because `001_architecture.md` lists wallpaper, panel
layout and lock among the things glimpsed does not hold, so no service of those names will exist.
`appearance` and `monitors` are reserved instead: they are glimpsed's and belong to no service.

Stylesheets stay separate files under `themes/`, not in the configuration: `themes/panel.css` is a
user override layered over the active theme pack's own, and the locker's is wherever `lock.css_path`
points, `themes/lock.css` by default. CSS is not TOML.

### Ownership and validation

The whole schema lives in `glimpse-config`, which all four binaries link, and every binary validates
the whole document:

- **An unknown key is an error** (`deny_unknown_fields`), in whichever table it lands.
- **The set of top-level table names is closed**, which is what catches `[panle]`: a misspelled
  top-level table would otherwise be ignored by every reader and silently do nothing.

A binary still _acts on_ only the tables it owns — the panel does nothing with `[night_light]` — but
it parses and checks all of them, because the types are already linked into it.

Schemas that version independently was never a property this project could have: the four binaries
are built from one workspace at one version and released together. What one shared schema buys
instead is that `--check-config` is exhaustive whichever binary runs it, and that a table two owners
read is one type rather than two that drift. The cost is the one already under "The cost of a single
file", now extended from syntax to schema.

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
- **Regular files only.** The descriptor's type is checked after the open; anything that is not a
  regular file is refused. A character device such as `/dev/zero` reads without end; a directory or
  a socket is never what the user meant.
- **A FIFO is not defended against.** The open is what blocks, so no check after it helps, and
  refusing one needs `O_NONBLOCK` and the constant from a C ABI crate. That dependency costs more
  than the case is worth. A file symlinked at a FIFO hangs the binary until a writer appears.
- **A size cap of 1 MiB per file**, applied to the descriptor rather than to a prior `stat`. No
  legitimate configuration approaches it, and it bounds what a mistaken link can pull into memory.
- **Symlink loops** surface as `ELOOP` and are reported as an unreadable file, not retried.

A dangling or unresolvable drop-in is **skipped with a warning**, not a failure. A stale link left by
an uninstalled package must not cost the user their session. The base file is different: if it
resolves to nothing readable, that is a load failure and the defaults rule applies.

Errors name the path as written and the path it resolved to, and never any of the file's content. A
link aimed at a private file — an SSH key, a token — must not echo that file into the journal or
into `glimpsectl config show` output. This is the reason resolution is specified at all: glimpsed runs
unprivileged, so a link cannot reach anything the user could not already read, but it can trick the
daemon into reprinting it somewhere more public than where it started.

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
- **Watches are per directory, never per file**, placed by the `watcher` service of
  `011_watcher.md`; a directory missing at start or recreated later is its problem, defined there.
  Four cover the whole stack: `/etc/glimpse/`, `/etc/glimpse/config.d/`,
  `$XDG_CONFIG_HOME/glimpse/` and its `config.d/`. A hundred drop-ins cost the same as one.
  Per-file watches would also be wrong on their own terms — they cannot see a drop-in that does not
  exist yet, and adding one is exactly the event that has to trigger a reload.
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

On `SIGHUP`, and on a change the `watcher` service reports, the whole stack is re-read, re-merged
and re-validated. On failure the running configuration survives untouched and the error location is
logged.

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

This is one of three deliberate breaks with existing configurations, alongside dropping `include`
and rejecting unknown keys.

`scheme = "auto"` resolves against `solar.daylight` — light while the sun is up, dark once it is
down. `light` and `dark` pin it. This is independent of night light: both services consume the same
solar data as siblings, so `[night_light] schedule = "off"` leaves automatic light and dark
untouched.

A `[[panels]]` entry may override `scheme` for one bar.

### `[location]`

| Key         | Type  | Default     | Meaning                           |
| ----------- | ----- | ----------- | --------------------------------- |
| `provider`  | enum  | `"geoclue"` | `geoclue`, `manual`               |
| `latitude`  | float | —           | required when `manual`, −90..90   |
| `longitude` | float | —           | required when `manual`, −180..180 |

The table is a tagged enum keyed on `provider`, which is what makes `manual` an addition rather than
a break: an existing `provider = "geoclue"` keeps parsing untouched.

`manual` is what keeps solar and weather working on a machine with no location service.

### `[night_light]`

| Key                  | Type    | Default       | Meaning                                      |
| -------------------- | ------- | ------------- | -------------------------------------------- |
| `schedule`           | enum    | `"automatic"` | `off`, `automatic`, `schedule`               |
| `temperature`        | integer | `4200`        | kelvin applied while active                  |
| `start_time`         | string  | —             | `HH:MM` local, used when `schedule` is fixed |
| `end_time`           | string  | —             | `HH:MM` local, used when `schedule` is fixed |
| `transition_minutes` | integer | `15`          |                                              |

| Value       | Behaviour                                                      |
| ----------- | -------------------------------------------------------------- |
| `off`       | never warms the screen, never actuates gamma                   |
| `automatic` | follows `solar.daylight`                                       |
| `schedule`  | follows `start_time` and `end_time`, with no location involved |

The value `manual` is accepted as an alias for `schedule`, which is the spelling existing
configurations use.

`automatic` needs `solar.daylight`, which the solar service derives from `location.position`.
Without it the service reports `degraded` rather than guessing a location.

`start_time` and `end_time` have no defaults and are read only when `schedule = "schedule"`. The
pair wraps midnight, which is the ordinary case: `20:00` to `07:00` is one night, not an empty
window. A `schedule` selection with either missing leaves the service `degraded` rather than
inventing a time.

There is no `enabled` key. `schedule = "off"` is what disabling looks like.

### `[idle]`

| Key                  | Type | Default | Meaning                           |
| -------------------- | ---- | ------- | --------------------------------- |
| `enabled`            | bool | `true`  |                                   |
| `respect_inhibitors` | bool | `true`  | honour logind and idle inhibitors |

Listeners are grouped into two profiles, chosen by whether the machine is on mains power:

```toml
[[idle.profiles.ac.listeners]]
timeout = 600                  # seconds
on_idle = "…"                  # command line run when the timeout elapses
on_resume = "…"                # command line run on activity; empty means nothing
# respect_inhibitors = true    # optional, overrides the table-level setting

[[idle.profiles.battery.listeners]]
timeout = 300
on_idle = "…"
on_resume = "…"
```

`on_idle` and `on_resume` are command lines the user supplies. That is the user's choice and not the
daemon reaching for a subprocess of its own: the rule against shelling out governs how glimpsed
talks to logind and systemd, not what a user may run on their own timer.

The shipped defaults are three listeners per profile — screens off, lock, suspend — at 600/900/3600
on mains and 300/900/1800 on battery. Supplying `listeners` replaces the list rather than adding to
it, per the merge rules.

### `[power]`

| Key                 | Type | Default | Meaning                                                |
| ------------------- | ---- | ------- | ------------------------------------------------------ |
| `lock_before_sleep` | bool | `true`  | hold a logind delay inhibitor and lock before suspend  |
| `lock_on_request`   | bool | `true`  | start the locker on logind's `Lock` signal             |

`lock_before_sleep` is the only route to a locked screen across a suspend; `009_systemd.md` explains
why nothing else can hold the inhibitor. Turning it off is for a machine that suspends somewhere
physically trusted.

`lock_on_request` is what answers `loginctl lock-session`. logind's `Unlock` signal has no key here
and will not get one — `006_lock.md` refuses it, and a configurable refusal is not a refusal.

### `[keyboard]`

| Key        | Type          | Default    | Meaning                                   |
| ---------- | ------------- | ---------- | ----------------------------------------- |
| `remember` | enum          | `"window"` | `global`, `app`, `window`                 |
| `labels`   | map of string | `{}`       | layout name to the short label to display |

`remember` is the scope at which the last layout is retained: one for the session, one per
application, or one per window.

### `[calendar]`

| Key             | Type    | Default | Meaning |
| --------------- | ------- | ------- | ------- |
| `poll_interval` | integer | `600`   | seconds |

```toml
[[calendar.sources]]
id = "work"                    # required, stable identifier
type = "ical"                  # ical or directory
uri = "https://…"              # required
# name = "Work"                # optional display name
# poll_interval = 300          # optional, overrides the table-level interval
# color = "#89b4fa"            # optional
```

A URI is fetched, so it is attacker-adjacent input: the calendar service caps response size and
treats every field of a fetched event as hostile text.

### `[monitors]`

| Key                 | Type   | Default | Meaning                                |
| ------------------- | ------ | ------- | -------------------------------------- |
| `builtin_connector` | string | —       | connector name of the internal display |

One of two tables glimpsed owns that belong to no single service, `[appearance]` being the other. It
describes the session's hardware rather than any one service's behaviour, and brightness reads it to
know which backlight is the internal panel's.

The panel does not read this table — it owns none of glimpsed's — so a UI binary that needs to know
which output is built in learns it from a topic, as it does everything else.

Absent means the internal display is detected rather than declared; `"eDP-1"` is the usual override.

### `[wallpaper]`

| Key             | Type    | Default     | Meaning                      |
| --------------- | ------- | ----------- | ---------------------------- |
| `color`         | string  | `"#101010"` | drawn when no image resolves |
| `path`          | path    | —           | image to draw                |
| `fit`           | enum    | `"cover"`   | `cover`, `contain`, `fill`   |
| `transition_ms` | integer | `800`       | crossfade length on a change |

### `[backdrop]`

| Key           | Type    | Default | Meaning                              |
| ------------- | ------- | ------- | ------------------------------------ |
| `enabled`     | bool    | `true`  |                                      |
| `path`        | path    | —       | image; falls back to the wallpaper's |
| `blur_radius` | integer | `24`    |                                      |

Owned by `glimpse-wallpaper` alongside `[wallpaper]`. The backdrop is what the compositor's overview
shows behind the workspaces, which is why it is blurred and separately configurable.

### `[lock]`

| Key           | Type   | Default             | Meaning                                 |
| ------------- | ------ | ------------------- | --------------------------------------- |
| `pam_service` | string | `"glimpse-lock"`    | the `pam_start` service name            |
| `css_path`    | path   | `"themes/lock.css"` | relative to the configuration directory |

```toml
[lock.background]
# color = "#101010"
# path = "/path/to/image"
# fit = "cover"
blur_radius = 0
dim = 0.35                     # 0.0 to 1.0

[lock.clock]
enabled = true
time_format = "%H:%M"
date_format = "…"

[lock.controls]
buttons = ["wifi", "input", "weather", "battery", "power"]
```

`pam_service` is configurable but changing it is how a working locker becomes an unlockable session.
`006_lock.md` carries the warning that belongs with it.

### `[[panels]]`

An array of tables, one per bar. An empty array means no panel.

| Key        | Type    | Default   | Meaning                                      |
| ---------- | ------- | --------- | -------------------------------------------- |
| `size`     | integer | `36`      | thickness in logical pixels                  |
| `monitor`  | string  | —         | connector name; absent means every output    |
| `position` | enum    | `"top"`   | `left`, `top`, `right`, `bottom`             |
| `margin`   | table   | all `0`   | `left`, `right`, `top`, `bottom`             |
| `scheme`   | enum    | `"dark"`  | overrides `[appearance] scheme` for this bar |
| `left`     | array   | non-empty | applet names, in order                       |
| `center`   | array   | non-empty | applet names, in order                       |
| `right`    | array   | non-empty | applet names, in order                       |

The three zone arrays hold applet names, and each ships a populated default rather than an empty
one, so a configuration that sets none still produces a usable bar. `__dynamic__` expands to the
applets not named elsewhere, so a user who lists a few keeps the rest without enumerating them.

### `[applets.<name>]`

| Key        | Type  | Default | Meaning                                     |
| ---------- | ----- | ------- | ------------------------------------------- |
| `extends`  | enum  | —       | the applet type this instance is built from |
| `settings` | table | `{}`    | free-form, interpreted by the applet type   |

`<name>` is the name used in a panel zone, so several instances of one type coexist under different
names. `extends` names the type: `audio`, `battery`, `brightness`, `bluetooth`, `display`,
`clipboard`, `clock`, `command`, `dynamic`, `exec`, `idle`, `keyboard`, `mpris`, `network`,
`next_event`, `notifications`, `pager`, `privacy`, `printing`, `removable`, `session`, `tray`,
`weather`, `window`, `workspace`.

`settings` is the one place unknown keys are not an error, because its shape belongs to the applet
type rather than to this schema.

### Services with no configuration

`tray`, `audio`, `network`, `bluetooth`, `battery`, `mpris`, `workspaces`, `brightness`,
`watcher`, `solar`. Every mirror among them has a backend that owns the state and the policy;
`watcher` watches the paths this spec already fixes; and `solar` derives sunrise and sunset from
`location.position`, with nothing left to decide.

`weather`, `sysstats`, `notifications` and `clipboard` take their settings from the applet that
displays them, under `[applets.<name>.settings]`, which is where existing configurations already
put them.

### Validation

`--check-config` reports every problem it finds rather than the first, and exits 1 if there are
any. The same checks run at startup and on reload, where they feed the load-failure rule instead:

- unknown top-level table, with the nearest known one as a suggestion
- unknown key inside an owned table, likewise
- wrong type, or a value outside a documented range
- a conditional requirement unmet: `[location] provider = "manual"` with no coordinate pair
- a time that is not `HH:MM`, or `start_time` equal to `end_time`, which describes no window
- a `[[calendar.sources]]` entry missing `id`, `type` or `uri`
- a `[[panels]]` zone naming an applet that no `[applets.<name>]` and no built-in type provides
- duplicate `timeout` values within one `[[idle.profiles.*.listeners]]` list
- an `[[idle.profiles.*.listeners]]` entry with no `on_idle`
- a `[[panels]]` `margin` or `size` below zero
- `dim` outside 0.0..1.0
- a drop-in that cannot be read
- a path that resolves to something other than a regular file, or past the 1 MiB cap
- more than 64 drop-ins in one directory

A syntax error carries file, line and column, naming the drop-in it is in rather than the base file
it merges over. A schema error carries the key path instead: it is found in the merged document, and
no file owns a line of that.

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

A TOML **syntax or schema** error anywhere in the document fails every binary's load, not only the
owner's: one table's stray bracket or misspelled key costs every binary its configuration, where a
file per binary would have cost one.

What that is worth is bounded by the load-failure rule above. At boot every binary starts on
defaults instead of the user's settings; on reload every binary keeps what it is already running.
Nothing exits and nothing is lost. The remaining cost is that the blast radius of a syntax error is
the whole document rather than one table, which `--check-config` catches before a restart and which
the editor catches before that.

## Alternatives considered

- **One file per binary** — rejected: the panel and the lock screen duplicate every appearance
  value, and a user has four files to keep in step. One file removes the duplication outright.
- **`[services.<name>]` prefixing** — rejected: a service can never collide with a UI table, because
  `001_architecture.md` puts wallpaper, panel layout and lock among the things glimpsed does not
  hold, and `appearance` and `monitors` are reserved. The prefix bought nothing and cost a level of
  nesting in every table name.
- **An `include = [...]` key instead of `config.d/`** — rejected, and this one breaks existing
  configurations: `_old` supports `include` and resolves cycles for it. Drop-ins cover the same
  overlay case with a directory listing instead of cycle detection, a depth cap and per-entry path
  resolution, and their ordering is visible in the filenames rather than buried in a key. The cost
  is knowingly accepted: a configuration using `include` stops loading and comes up on defaults.
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
- **A `config.reloaded` topic** — rejected: no client needs one. `glimpsectl config validate` and
  `config path` re-read the stack themselves, the panel has no specified use for a load outcome, and
  `009_systemd.md` already makes the journal the diagnostic channel. A topic nobody reads still costs
  a payload type, an SDK type in three languages, and a wire contract to keep compatible.
- **Inline API keys** — rejected: `glimpsectl config show` writes the merged document to stdout. Keys are
  paths to files.

## Changelog

- 2026-08-20 — created, split out of `003_daemon.md`.
- 2026-08-20 — added the load-failure rule: invalid config logs and falls back to defaults at boot, and is dropped on reload. Neither exits; `--check-config` still exits 1.
- 2026-08-20 — the file is `config.toml`; dropped `[daemon]`, so every top-level table belongs to a service or a UI binary and logging is configured only by flag, `RUST_LOG` or a unit drop-in.
- 2026-08-20 — replaced `include = [...]` with `config.d/` drop-ins; includes solved the four-file sharing problem, which merging into one file already solved.
- 2026-08-20 — dropped the environment layer; `config.d/` covers the override case with the full schema and real types.
- 2026-08-21 — `--print-config` is now `glimpsectl config show`; the daemon no longer inspects its own configuration.
- 2026-08-20 — specified path resolution: symlinks followed, regular files only, open-then-inspect, 1 MiB cap, and no file content in error messages.
- 2026-08-20 — added Bounds: no directory recursion, one open descriptor at a time, 64 drop-ins per directory, per-directory watches capped at six, and reload degrades rather than fails when watches cannot be registered.
- 2026-08-20 — dropped the `config.reloaded` topic; load outcomes are logged, and `glimpsectl config validate` reports them on demand.
- 2026-08-20 — watching is performed by the `watcher` service; see `011_watcher.md`.
- 2026-08-20 — `[nightlight]`: dropped `enabled`, replaced `sunrise`/`sunset` with the required `activate_at`/`deactivate_at` pair, and documented what each mode does.
- 2026-08-20 — schema rebased on `_old/glimpse-core/src/config` so existing files keep loading: `[night_light]`, `[location]`, `[idle]` profiles, `[keyboard]`, `[calendar]`, `[monitors]`, `[[panels]]`, `[applets.*]`, `[wallpaper]`, `[backdrop]` and `[lock]` keep their original names and defaults.
- 2026-08-20 — `theme`/`theme_mode` become `[appearance]` `pack`/`scheme`, with no aliases: a deliberate break, because a scalar named `theme` blocks a `[theme]` table forever.
- 2026-08-20 — corrected the claim that theme resolves against night light; both are siblings over `solar.daylight`, as `_old` implements them.
- 2026-08-20 — review pass: dropped validation rules for keys the rebase removed, corrected night light to follow `solar.daylight`, and rewrote the Problem, the include rejection and the single-file cost, which all argued from a four-file history that never existed.
- 2026-08-20 — added `[power]`, which stops being a service with no configuration once it owns locking before sleep.
- 2026-08-21 — the whole schema lives in `glimpse-config` and every binary validates every table; independent per-binary schema versioning was never available to one workspace released at one version.
- 2026-08-21 — the FIFO guard is dropped: refusing one needs `O_NONBLOCK` and a C ABI crate, and the dependency costs more than the case.
- 2026-08-21 — file, line and column are promised for syntax errors only; a schema error is found in the merged document, which has no lines to name, and carries the key path.
