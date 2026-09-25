# glimpse-config

Layered TOML load, drop-ins, merge, validate and watch. Owns where glimpse files live and the one
place that asks the environment a regional question.

## Layers

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

**`user_dir()` (`~/.config/glimpse`) is owned by this crate** — anything needing a glimpse path asks
here rather than rebuilding `dirs::config_dir().join("glimpse")`. **Merging is per key: tables
merge, scalars replace, arrays replace rather than append** — an appending array could never be
shortened by a later layer.

## Schema

Every key and enum value is kebab-case via `rename_all = "kebab-case"` on each `schema/*.rs` type;
`every_key_and_enum_value_is_kebab_case` fails the generated schema on any underscore, since the
attribute is easy to forget and forgetting it is silent. Every long-lived binary links the whole
schema and validates the whole document, then acts on only the tables it owns.
`deny_unknown_fields` on `Config` closes the set of top-level tables, so a misspelled `[panle]`
errors instead of being silently ignored.

**An applet a panel names has to resolve, or the load fails.** `left`/`center`/`right` are plain
strings serde cannot check; `named_applets_exist` checks them against `[applets]` and the kind
names, so a misspelling is a load error rather than a `warn` and a quietly missing bar entry.

## Generated references

`default_document()`, `json_schema_document()` and `commented_document()` render
`data/config.{default,schema,commented}.{toml,json}` from the types, checked by a test against the
file in the tree; none is read at runtime. `seed_user_config` writes the commented document to
`user_dir()/config.toml` once and never replaces it; `--config`/`GLIMPSE_CONFIG_PATH` skip seeding.
**A `[table]` header in the seed stays uncommented only when an empty table means the same as no
table** — an `[[array]]` header never qualifies, since an empty element replaces rather than merges
as nothing; `[applets]` is documentation only.

## Reading and errors

Symlinks are followed; regular files only, capped at 1 MiB. **The descriptor is inspected after the
open, never a path before it** — between a `stat` and an `open` the path can be replaced. **A
missing file is an absent layer; a file that exists and is wrong fails the whole load.**
`load` reports every problem found, not the first. **No `ConfigError` renders any file content** —
only the message and span are taken from `toml::de::Error`, so a `config.toml` aimed at an SSH key
is never echoed into the journal. A syntax error names file, line, column and the drop-in it came
from; a schema error names the key path, since it is found only in the merged document. The caller
decides what a failure means: log and fall back to `Config::default()` at startup, drop the update
on reload.
**A value the runtime cannot use is refused here, where the key can be named.** `[night-light]`
`start-time`/`end-time` are checked against `%H:%M` at deserialization, so a typo fails on the
document rather than reaching the night light as an indistinguishable "not set".

## Themes

A theme is a directory of stylesheets under `<root>/<name>/`, resolved from `user_dir()/themes`
then `DATA_DIR/themes` unless `GLIMPSE_THEMES_DIR` replaces both. `GLIMPSE_THEME` overrides
`appearance.theme` at read time, so `Config` keeps reporting the document on disk. **Resolution
picks one directory, not one file at a time.** `theme_dir_for` returns the first of
`user/<theme>`, `data/<theme>`, `user/adwaita`, `data/adwaita` that exists, because GTK resolves a
relative `@import` against the importing file's own directory — a theme assembled from two roots
could not import across them.
`dark.css` is a theme directory's fourth member, loaded only while the effective scheme is dark.
`appearance.theme-variant` is a CSS class on every window, not a file; a name outside
letters/digits/`-`/`_` or starting with a digit is dropped with a warning.

## Watching

Every binary watches and re-reads its own files. `watch_dirs(config_path)` is the layer stack's
directories, existing or not; `--config` replaces the whole stack and so the whole watch set.

- **Watches go on directories, never files** — a per-file watch cannot see a drop-in that does not
  exist yet, and creation is exactly the change that needs noticing. Events coalesce after 250 ms
  of quiet, since one editor save is a write, a rename, and sometimes a delete and a create; one
  inotify instance covers the whole set and the timer arms only on the first event, so an idle
  session costs zero wakeups.
- **A missing directory is watched through its nearest existing ancestor within the watched set**,
  descending when the component appears; an absent `glimpse/` falls back onto nothing, since
  `$XDG_CONFIG_HOME`/`/etc` churn from unrelated software — creating the config directory needs a
  restart. Every watched directory is made absolute with `std::path::absolute` before arming, or a
  relatively-armed watch matches none of its own events.
- **A watch re-arms when its directory is replaced**, since it is bound to an inode, not a name;
  the gap produces no events, so a re-arm means "read everything again." The watch set itself is
  derived once, at construction — re-pointing a symlinked base file is caught, but edits at the new
  target are not until restart.

`reread` answers with a new document only if it parsed **and differs**. `watch_config` merges the
filesystem watch with `SIGHUP` — neither replaces the other. Nothing is filtered by extension,
which would drop the creation of `config.d/` itself.

## Applets

`Applet` is a `Common` plus a `Kind`, an internally-tagged enum on `extends`, both in the schema so
a misspelled setting is a load error naming the table and key. The table name supplies the tag
when `extends` is absent — `[applets.clock]` is the clock; only a second instance needs `extends`.
**Common settings are split off the table by `take_common`, not `#[serde(flatten)]`** — `flatten`
collects unclaimed keys and `deny_unknown_fields` rejects them, so serde refuses both on one type.
Both halves still deny unknown fields, so a misspelled common key survives into the kind's table
and fails by name.

**`settings-command` is a list, never one string** — no quoting, no word splitting, no shell to
inject through. A label without a command, or a command without a label, is a load error.
**A variant with no settings is written `Clock {}`, never `Clock`** — `deny_unknown_fields` has
nothing to deny on a unit variant, so it silently swallows every key under it.

## The `locale` convention

A setting whose correct value the system already knows takes an enum with a `locale` variant, the
default. `[regional]` is the only place glimpse asks the environment anything.

| Key           | Asks             | Resolved by                                | Live reload |
| ------------- | ---------------- | ------------------------------------------ | ----------- |
| `language`    | `LANGUAGE`       | each UI binary, before `init_translations` | no          |
| `hour-format` | `LC_TIME`        | each UI binary, at load                    | yes         |
| `units`       | `LC_MEASUREMENT` | `glimpse-weather`                          | yes         |

`environment.rs` holds both resolvers; `libc` is the only way it asks. **`TWELVE` is `%-I:%M %p`,
not `%l:%M %p`**, since `%l` is space-padded and carries a leading space into the middle of a
sentence.
**These resolvers answer `C` — 24-hour, metric — unless `setlocale(LC_ALL, "")` has already run**,
each binary's own job, since `setlocale` mutates process-global state read from tokio worker
threads. **`language` is the one key whose unknown value is a warning, not a load error** — the
others name a closed set of behaviours; this names a catalog that may not be installed.

## Not here

A default naming an external command names one the system already has — `loginctl`, `systemctl` —
never a helper this repository would have to install, since `scripts/` is not installed. Semantic
validation across fields is not written; where a rule fits the type it belongs there instead of in
a pass that has to remember to run.
