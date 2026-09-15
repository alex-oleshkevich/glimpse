# glimpse-config

Layered TOML load, drop-ins, merge, validate and watch. Owns where glimpse files live and the one
place that asks the environment a regional question.

## What it does

`load(config_path)` resolves the layer stack, reads it, merges it and types it:

| #   | Layer           | Source                                                    |
| --- | --------------- | --------------------------------------------------------- |
| 1   | defaults        | the `Default` impls under `schema/`                       |
| 2   | system          | `/etc/glimpse/config.toml`                                |
| 3   | system drop-ins | `/etc/glimpse/config.d/*.toml`, lexical order             |
| 4   | user            | `$XDG_CONFIG_HOME/glimpse/config.toml`                    |
| 5   | user drop-ins   | `$XDG_CONFIG_HOME/glimpse/config.d/*.toml`, lexical order |

`--config <PATH>` replaces layers 2 through 5 with that one file, drop-ins included.
`resolved_files(config_path)` returns the same ordered list without reading any of it.

**`user_dir()` is `~/.config/glimpse`, and this crate owns it.** Anything that needs a glimpse path
asks here rather than rebuilding `dirs::config_dir().join("glimpse")`. It is the directory, not a
file: what a caller joins onto it is that caller's business.

**Merging is per key: tables merge, scalars replace, and arrays replace rather than append** — an
appending array could never be shortened by a later layer.

`data/config.default.toml` is a reference nothing reads, kept honest the way `cargo fmt` keeps
formatting honest: `default_document()` renders it from `Config::default()` and a test fails if the
checked-in file differs. `data/config.schema.json` is generated the same way, for editor tooling.

## Key naming

Every key and every enum value is kebab-case, through `rename_all = "kebab-case"` on each
`schema/*.rs` type. The convention comes from the stack this file sits beside — GSettings, the XDG
portal, niri's `config.kdl`, CSS — not from Rust.

The attribute is easy to forget on a new table and forgetting it is silent, so
`every_key_and_enum_value_is_kebab_case` walks the generated schema and fails on any underscore.

`schemars` does not surface `#[serde(alias = ...)]` in the schema's enum values, so an alias parses
but an editor flags it.

## One file, one schema

Every long-lived binary reads `config.toml`, links the whole schema and validates the whole
document, then acts on only the tables it owns. An unknown key is an error wherever it lands, and
the set of top-level table names is closed — `deny_unknown_fields` on `Config` is what catches a
misspelled `[panle]` that every reader would otherwise ignore.

## Reading a file

- Symlinks are followed — a `config.toml` pointing into a dotfile repository is the ordinary case.
- **The descriptor is inspected after the open, never a path before it**: between a `stat` and an
  `open` the path can be replaced.
- Regular files only, capped at 1 MiB. A FIFO is **not** defended against: the open is what blocks.
- `config.d/` is read one level deep, one file at a time, at most 64 entries.
- **A missing file is an absent layer everywhere in the stack.** A file that exists and is wrong —
  wrong type, too large, a syntax error — still fails the whole load.

Loading is synchronous and every binary calls `load` once at startup; only watching is async, and it
puts each re-read on `spawn_blocking`.

## Errors

`load` reports every problem it found, not the first. **No `ConfigError` renders any of a file's
content**: `toml::de::Error`'s `Display` prints the offending source line, and a `config.toml` aimed
at an SSH key would echo it into the journal, so only the message and the span are taken.

A syntax error names file, line and column, and names the drop-in rather than the base file it
merges over. A schema error names the key path instead — it is found in the merged document, which
has no lines to name.

The caller decides what a failure means: at startup, log it and come up on `Config::default()`; on
reload, drop the update and keep what is running. Neither exits.

**A value the runtime cannot use is refused here, where the key can be named.** `[night-light]`
`start-time` and `end-time` are checked against `%H:%M` during deserialization, so a misspelling
fails on the document rather than reaching the night light, which could only report "a schedule
needs start-time and end-time" — which reads as "you did not set them" when they are set and merely
wrong. They stay `Option<String>`, so `config show` prints the spelling the user wrote.

## Themes

A theme is a directory of stylesheets under `<root>/<name>/`. The roots are `user_dir()/themes` then
`DATA_DIR/themes`, unless `GLIMPSE_THEMES_DIR` names one, which replaces both — an explicit override
replaces the stack rather than joining it, the same rule `--config` follows.

`GLIMPSE_THEME` chooses the name over `appearance.theme`. It is applied in `stylesheet`,
`theme_dir_for` and `watch_theme` rather than in `load`, so `Config` keeps reporting the document on
disk — a binary told to render one theme does not start claiming the user configured it.

**Resolution picks one directory, not one file at a time.** `theme_dir_for(theme)` returns the first
of `user/<theme>`, `data/<theme>`, `user/adwaita`, `data/adwaita` that is a directory, and every
sheet comes from it. The directory is the unit because CSS makes it one: GTK resolves a relative
`@import` against the importing file's own directory, so a theme assembled from two roots cannot
import across them. A theme is all or nothing; copy the whole directory to customise one rule.

The shipped `adwaita` theme is three empty files — component rules and the token vocabulary live in
`glimpse-widgets`, so a theme that redefines nothing still renders correctly.

`user_stylesheet()` locates the user's own `styles.css`, optional and not part of any theme, which
always loads on top.

`watch_theme(theme)` watches `user_dir()`, then every root and every `<root>/<theme>`, then the
directory resolution chose. **The roots are not decoration**: `nearest_existing` walks up only as far
as directories in the requested set, so a watch armed on the theme directory alone reports
`Unavailable` on a machine where the user never created one. Arming a root that does not exist costs
nothing — `rearm` reports `Unavailable` only when *every* arm fails.

## Watching

Every binary watches its own files and re-reads them itself, so hot reload does not depend on another
glimpse process being alive.

`watch_dirs(config_path)` is the layer stack's *directories*, existing or not. `--config` replaces
the whole stack with one file, so it replaces the whole watch set too.

**Watches go on directories, never on files.** A per-file watch cannot see a drop-in that does not
exist yet, and creating one is exactly the change that has to be noticed. A symlinked base file adds
the directory holding its resolved target: editors write a new file and rename it over the old one,
so a watch on the link alone goes quiet after the first save.

- Only create, modify and remove events. Access events fire for every read, this process's own
  included, and describe nothing that changed.
- **Events are coalesced until the directory has been quiet for 250 ms**, because one editor save is
  a write, a rename, and sometimes a delete and a create. Waiting for quiet also means a file
  rewritten in place is read once it has finished — a burst of non-atomic writes costs one reload
  and no parse errors.
- **One inotify instance for the whole set, and no timer.** The coalescing is a `tokio` timeout
  armed only once an event arrives, so an idle session costs zero wakeups. This is why
  `notify-debouncer-full` is not used: its worker is a `loop { sleep(tick); flush }` that cannot be
  woken early.
- **A directory that does not exist is watched through its nearest existing ancestor *within the set
  being watched***, and the watch descends when the missing component appears. An absent `config.d/`
  falls back onto the `glimpse/` beside it. An absent `glimpse/` falls back onto nothing:
  `$XDG_CONFIG_HOME` and `/etc` are written constantly by unrelated software, and a watch on either
  wakes us for every one of those. Creating the configuration directory therefore needs a restart —
  the one moment a restart costs nothing, because there was nothing configured to reload.
- **Every wanted directory is made absolute before it is armed.** `notify` resolves a relative path
  against the working directory and reports every event under that absolute root, so a
  relatively-named directory arms on the right inode and matches none of its own events — reporting
  nothing for the rest of its life and looking exactly like a directory where nothing happens.
  `std::path::absolute` is what agrees with `notify`, because it is lexical and leaves symlinks
  alone exactly as `notify`'s own `current_dir().join(path)` does.
- **The watch re-arms when its directory is replaced.** A watch is bound to an inode rather than a
  name, so deleting and recreating a directory otherwise leaves one armed on something nobody can
  reach. The inode is compared, not just the path, and `Update::Rearmed` says "read everything
  again" — whatever happened during the gap produced no events and cannot be inferred.
- A re-arm places the new watch before releasing the old one; a kernel that refuses the new one
  would otherwise cost the working watch as well as the descent.

**The watch set is derived once, at construction.** Re-pointing a symlinked base file at a different
directory is caught, but edits at the new target are not until restart. Re-deriving on every event
would mean stat-ing the whole stack and rebuilding every watch, for a case that arises only when
someone moves their dotfile repository.

`reread` loads the stack off the runtime threads and answers with the new document only if it parsed
**and differs**. A read that failed yields the reason instead of logging it: a watched directory
goes on producing events while it stays broken, and an editor's swap files land beside it, so the
reader keeps the last failure and reports one at `error` only when the message changes. A document
that loads clears it, whether or not it moved.

`watch_config(config_path, current)` merges two triggers, and neither replaces the other: the
filesystem watch, and `SIGHUP`. An editor whose write inotify never saw still has a way to apply the
change, and a session whose watches the kernel refused still reloads on request. `SIGHUP` is not an
`Update` — "a human asked" is not something a directory did, so the two merge one level up.

**Nothing is filtered by extension.** That would drop the creation of `config.d/` itself, which is
one of the changes that most needs noticing. An unrelated write costs one debounced re-read, which
the equality gate then absorbs.

## Applets

`Applet` is one applet's whole configuration: a `Common` and a `Kind`, the internally-tagged enum on
`extends`. Both halves are in the schema rather than a free-form table, so a misspelled setting is a
load error naming the table and the key.

**The table name supplies the tag when `extends` is absent.** `[applets.clock]` is the clock; only a
second instance needs `extends`. A hand-written `Deserialize` injects the key before handing the
table to serde.

**The common settings are split off the table before the kind sees it.** The obvious way —
`#[serde(flatten)]` on a `Common` field — does not compile: `flatten` collects the keys a struct did
not claim and `deny_unknown_fields` rejects them, so serde refuses both on one type. `take_common`
removes them from the raw table instead, and both halves keep `deny_unknown_fields` — so a
misspelled common key is *not* taken, survives into the table the kind is denying against, and fails
by name. The cost is a `COMMON` list that must agree with `Common`'s fields, compared against the
struct's own schema in both directions by a test.

**The common settings are inlined into every schema branch, not shared by `$ref`.** Factoring them
into an `allOf` would break validation rather than tidy it: each branch carries
`additionalProperties: false`, and `additionalProperties` does not see properties contributed by an
`allOf` sibling, so every common key would be rejected as unknown.

**`settings-command` is a list, never one string.** No quoting rules, no word splitting, no shell —
and a shell is an injection surface for something with no reason to have one. Setting a label
without a command, or a command without a label, is a load error: one is a row that does nothing and
the other a row nobody can see.

**A variant with no settings is written `Clock {}`, never `Clock`.** `deny_unknown_fields` has
nothing to deny on a unit variant, so it silently swallows every key written under it.

Resolving is not the same as being implemented. A name resolving to an applet no binary builds is an
ordinary state logged at `debug`, not a bad document. No name is reserved for "some applet, later".

## Asking the system: the `locale` convention

A setting whose correct value the system already knows takes an enum with a `locale` variant, and
`locale` is the default. `[regional]` is the only place in glimpse that asks the environment
anything.

| Key           | Asks             | Resolved by                                | Live reload |
| ------------- | ---------------- | ------------------------------------------ | ----------- |
| `language`    | `LANGUAGE`       | each UI binary, before `init_translations` | no          |
| `hour-format` | `LC_TIME`        | each UI binary, at load                    | yes         |
| `units`       | `LC_MEASUREMENT` | `glimpse-weather`                          | yes         |

`environment.rs` holds both resolvers and `libc` is the only way it asks. Nothing else in the
workspace calls `nl_langinfo`, `setlocale` or `strftime` — the same rule as `user_dir()`.

**`TWELVE` is `%-I:%M %p`, not `%l:%M %p`** — `%l` is space-padded and carries a leading space into
the middle of a sentence. Both patterns are reached through `clock(twelve_hour)`.

**These resolvers answer `C` — 24-hour, metric — unless `setlocale(LC_ALL, "")` has already run.**
That call belongs to the binary: `init_translations` for a UI binary, `init_locale` for a non-UI
process that resolves regional settings. It is deliberately not done here, because `setlocale`
mutates process-global state and services read the result from tokio worker threads.

`units` is one bit. `en_US` is the only locale in glibc's database declaring `measurement 2`, so
`locale` is metric for everyone else — including `en_GB`, which gets km/h rather than mph. That is
accepted, not an oversight: splitting speed from temperature is a wire change to `UnitSystem` and
both providers.

**`language` is the one key whose unknown value is a warning rather than a load error.** The others
name a closed set of behaviours; this one names a catalog that may or may not be installed.

Two conditions keep the convention honest:

- **A setting only gets `locale` if the system can actually be asked.** A `locale` that falls back
  to a hardcoded value is worse than naming that value, because the user sets nothing, gets an
  answer, and cannot tell which of the two produced it.
- **Resolution is measured per setting, not assumed.** For `hour-format`, only the rendered `%X`
  separates the cases: `T_FMT_AMPM` returns `%I:%M:%S %p` even under `C`, because it reports whether
  a locale *has* a twelve-hour form rather than whether it prefers one, and `T_FMT` answers `%r` for
  `en_US`, which contains neither `%I` nor `%p`.

`first-day` has **no** `locale` variant, and that is the convention working. GTK's translated
`calendar:week_start:0` came back as the untranslated msgid — meaning Sunday — under an `LC_TIME`
whose answer is Monday, and `_NL_TIME_FIRST_WEEKDAY` has never been measured here. The first
condition forbids adding a variant on the strength of an assumption; measuring it is the work that
would justify one.

Two enums and this section are the entire mechanism. There is no `Localized<T>`.

## Not here

A default that names an external command names one the system already has — `loginctl`,
`systemctl`. Pointing at a helper this repository would have to install is a default that is broken
on every machine until that install lands, and `scripts/` is not installed.

Semantic validation — duplicate idle timeouts, a panel zone naming an applet nothing provides — is
not written yet. `[geolocation]` needs none: the table is one internally tagged enum, so a
half-filled table is a `missing field` from serde before any reader sees it. Where a rule can be
expressed in the type it belongs there rather than in a pass that has to remember to run.

`Schedule::as_str` and `Schedule::parse` are the one spelling table for `[night-light] schedule`,
here because a mode named on a command line and a mode written in the document are the same
vocabulary. `manual` stays a document-only alias: nothing prints it, so accepting it from a caller
would add a spelling with no way back out.
