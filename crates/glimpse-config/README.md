# glimpse-config

## Layers

**Each file owns its tables**: `config.toml` holds every table
except `PANEL_TABLES` (`[[panels]]`, `[applets]`), which belong to `panel.toml` and `panel.d/*.toml`.
A table in the wrong file, and any schema error, fails the load with an error naming the file.
`Documents::Config` reads `config.toml` only, so a broken `panel.toml` never sends a daemon to
defaults; `Documents::ConfigAndPanel` (the panel and `glimpsectl`) reads both.

`roots()` is the stack, highest first: `--config-dir`/`GLIMPSE_CONFIG_DIR` (stored once by `init`),
`$XDG_CONFIG_HOME/glimpse`, `/etc/glimpse`. `DATA_DIR` (`$PREFIX/share/glimpse`, from the same
`PREFIX` `scripts/glimpse-paths.sh` reads) holds built-in assets and is never a config layer.
`config_files` lists every root's `config.toml`, then each root's `panel.toml` and
`panel.d/*.toml` (lexical order), lowest root first; a later file wins and a missing one is skipped.
`load_files` decides ownership by file name, so `glimpsectl config validate PATH` never reaches `roots()`.

**Anything needing a glimpse path asks here.** `find(relative)` returns the first hit across the
roots and then `DATA_DIR`; **`writable_file(name)` is `--config-dir` when given, else the user
directory, and `None` with neither — never `/etc/glimpse`.** **Merging is per key: tables merge,
scalars replace, arrays replace rather than append** — an appending array could never be shortened
by a later layer. `runtime_dir()` resolves to `$XDG_RUNTIME_DIR/glimpse` without creating it (`None`
when unset); regenerable derived data, such as processed wallpaper rasters, goes under `cache_dir()`.

## Schema

Every key and enum value is kebab-case via `rename_all = "kebab-case"` on each `schema/*.rs` type;
`every_key_and_enum_value_is_kebab_case` fails the generated schema on any underscore, since the
attribute is easy to forget and forgetting it is silent. Every long-lived binary links the whole
schema and validates the whole document, then acts on only the tables it owns.
`deny_unknown_fields` on `Config` closes the set of top-level tables, so a misspelled `[panle]`
errors instead of being silently ignored.

**An applet a panel names that does not resolve is reported, never fatal.** `left`/`center`/`right`
are plain strings serde cannot check; `unknown_applets` checks them against `[applets]` and the kind
names and returns one message per miss, so the panel starts without it and tells the user why.

## Generated references

`default_document`, `json_schema_document` and `commented_document` render
`data/{config,panel}.{default.toml,commented.toml,schema.json}` from the types, each restricted to
its file's tables; none is read at runtime.
**A `[table]` header in a commented document stays uncommented only when an empty table means the
same as no table** — an `[[array]]` header never qualifies, since an empty element replaces rather
than merges as nothing; `[applets]` is documentation only.

`seed_user_config` creates the user directory and writes a stub `config.toml` and `panel.toml` (a
`#:schema` line and the docs link) into it, each only when absent, and seeds nothing when
`--config-dir` is given. A stub `panel.toml` sets nothing, so the default layout applies.

## Reading and errors

Symlinks are followed; regular files only, capped at 1 MiB. **The descriptor is inspected after the
open, never a path before it** — between a `stat` and an `open` the path can be replaced. **A
missing file is an absent layer; a file that exists and is wrong fails the whole load.**
**No `ConfigError` renders any file content** —
only the message and span are taken from `toml::de::Error`, so a `config.toml` aimed at an SSH key
is never echoed into the journal. A syntax error names file, line, column and the drop-in it came
from; a schema error names the file and the key path. **No
config error stops a daemon:** `load_or_default` logs and starts `Config::default()`; `glimpse-idle` and `glimpse-sunset` turn
their feature off on that default, since the document may have set it. A reload
drops the update, and only the panel notifies, so one broken document is one notification.
**A value the runtime cannot use is refused here, where the key can be named.** `[night-light]`
`start-time`/`end-time` are checked against `%H:%M` at deserialization, so a typo fails on the
document rather than reaching the night light as an indistinguishable "not set".

## Writing

`write_table` edits only the named TOML table — setting the given values and removing the given
keys — while preserving comments, ordering, quoting, CRLF and the path's symlink. It refuses an
invalid document, a dangling symlink, a read-only file or a table value. Before replacing the
target, it compares mtime, size and inode and retries a concurrent edit against a fresh read, up to
three times. Callers pass it a `writable_file`.

## Themes

A theme is a directory of stylesheets named `<name>`: the first `<root>/themes/<name>` across the
roots, then `DATA_DIR/themes/<name>`, then the same for `adwaita`. `GLIMPSE_THEMES_DIR` replaces all
of those roots. `GLIMPSE_THEME` overrides `appearance.theme` at read time, so `Config` keeps
reporting the document on disk. **Resolution picks one directory, not one file at a time**, so a
theme's stylesheet and its `dark.css` always come from the same place. A user's `styles.css` and
`dark.css` are the first hit across the roots; at startup an unreadable one leaves only its own layer empty. `dark.css` is loaded only while the effective scheme
is dark. `appearance.theme-variant` is a CSS class on every window, not a file; a name outside
letters/digits/`-`/`_` or starting with a digit is dropped with a warning.

`Theme` carries stylesheet contents, read off the GTK thread. CSS loads with `load_from_string`, so
a relative `url()` or `@import` in a stylesheet resolves against nothing.

## Watching

**One watch per process, shared by every reader.** The first `watch_config` or `watch_theme`
subscription starts it over the writable roots (`--config-dir` and the user directory, never
`/etc/glimpse`): each root, its `panel.d/`, `themes/` and every directory directly under `themes/`,
`DATA_DIR/themes` the same way,
the parent of a symlinked `config.toml` or `panel.toml` target, and `GLIMPSE_THEMES_DIR` with its
immediate subdirectories. SIGHUP signals the same stream. Each subscriber rereads on every signal
and yields a `Reload<T>` when the result differs from what it holds:

- `Changed(T)` is a new value.
- `Recovered` is a broken document that loads again unchanged.
- `Rejected(reason)` keeps the running value and is reported once per distinct failure. For a theme
  it is an unreadable file or an unknown name, never a CSS syntax error.

`watch_theme` reads once after it subscribes, so a renamed theme arrives without a file event.
`watch_dirs` is a separate watch for service watchers (calendar, places, exec applications).

- **Watches go on directories, never files** — a per-file watch cannot see a drop-in that does not
  exist yet. Nothing is filtered by extension, which would drop the creation of `panel.d/` itself.
  Events coalesce after 250 ms of quiet, since one editor save is a write, a rename, and sometimes
  a delete and a create.
- **A missing directory is watched through its nearest existing ancestor within the watched set**,
  descending when the component appears; an absent `glimpse/` falls back onto nothing, since
  `$XDG_CONFIG_HOME`/`/etc` churn from unrelated software — creating the config directory needs a
  restart. Every watched directory is made absolute with `std::path::absolute` before arming, or a
  relatively-armed watch matches none of its own events.
- **A watch re-arms when its directory is replaced**, since it is bound to an inode, not a name;
  the gap produces no events, so a re-arm means "read everything again." The watch set is a
  function recomputed after each change or re-arm, so a theme directory created later joins it and
  re-pointing a symlinked base file also watches edits at the new target.

## Applets

`Applet` is `{ kind, regional }`; `kind` is an internally-tagged enum on `extends`, and both are in
the schema so a misspelled setting is a load error naming the table and key. Kind names use their canonical
spelling. The table name supplies the tag when `extends` is absent; custom instances set `extends`.
**A variant with no settings is written `Clock {}`, never `Clock`** — `deny_unknown_fields` has
nothing to deny on a unit variant, so it silently swallows every key under it.

**A desktop-file id in a zone resolves to an `exec` applet**, but only after no kind matched the
name: `center = ["me.example.Pomodoro"]`, two or more `.`-separated segments of
`[A-Za-z_][A-Za-z0-9_-]*`. A table named by the id sets `options` without `extends`; a second
instance is `[applets.pomo]` with `extends = "exec"` and a required `applet` id. `options` reaches
the applet as written, unvalidated. Exec
applets run unconfined as systemd units; their table takes `applet`, `options` and `env`.

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
never a helper this repository would have to install, since `scripts/` is not installed. Rules that
fit a type are checked at deserialization; `Config::validate` reports cross-field problems after
merging, and the loader warns without rejecting the document.
