# glimpse-config

Layered TOML configuration, shared by the daemon and every UI binary.

## What it does

`load(config_path)` resolves the layer stack, reads it, merges it and types it:

| #   | Layer           | Source                                                    |
| --- | --------------- | --------------------------------------------------------- |
| 1   | defaults        | the `Default` impls under `schema/`                       |
| 2   | system          | `/etc/glimpse/config.toml`                                |
| 3   | system drop-ins | `/etc/glimpse/config.d/*.toml`, lexical order             |
| 4   | user            | `$XDG_CONFIG_HOME/glimpse/config.toml`                    |
| 5   | user drop-ins   | `$XDG_CONFIG_HOME/glimpse/config.d/*.toml`, lexical order |

`--config <PATH>` replaces layers 2 through 5 with that one file, drop-ins included: no `config.d/`
beside it is read.

`resolved_files(config_path)` returns the same ordered file list `load` reads, without reading any
of them — what `glimpsectl config path` prints.

`user_dir()` is `~/.config/glimpse`, or `None` on a platform that names no config directory. This
crate owns where a user's glimpse files live, so anything else that needs to look one up asks here
instead of rebuilding `dirs::config_dir().join("glimpse")` — `glimpse-compositors` finds
`language-codes.json` that way. It is the directory, not a file: what a caller joins onto it is that
caller's business.

Only files that exist are merged. Layer 1 is not a document — a table absent from every file keeps
the value from its `Default` impl. `data/config.default.toml` is a reference nothing reads, kept
honest the way `cargo fmt` keeps formatting honest: `default_document()` renders it from
`Config::default()`, `just gen-config-default` writes the result, and a test fails if the checked-in
file and that rendering differ.

Merging is per key: **tables merge, scalars replace, and arrays replace rather than append** — an
appending array could never be shortened by a later layer.

## The JSON Schema

`data/config.schema.json` is `Config`'s shape for editor tooling — Even Better TOML and other
`taplo`-based editors read it for completion and inline validation. Every `schema/*.rs` type derives
`schemars::JsonSchema` alongside `Serialize`/`Deserialize`; `json_schema_document()` renders it,
`just gen-config-schema` writes the result, and a test keeps it honest the same way as the default
document. `Applet.settings` is described as an open object (`#[schemars(with = "...")]`), since its
shape belongs to the applet type, not this schema.

`default_document()`'s header carries a `#:schema /usr/share/glimpse/config.schema.json` directive —
the path `just install` puts the schema at — so a config file that starts from the shipped default
gets completion for free. One caveat: `schemars` does not surface a `#[serde(alias = ...)]` in the
schema's enum values, so `night-light.schedule = "manual"` still parses but an editor will flag it;
`"schedule"` is the spelling the schema expects.

## Key naming

Every key and every enum value is kebab-case. The document contains no underscores, and
`rename_all = "kebab-case"` on each `schema/*.rs` type is what translates the snake-case Rust fields
behind them — `pub background_dark` is `background-dark` on the page.

The convention comes from the stack this file sits beside rather than from Rust: GSettings spells it
`color-scheme`, the XDG portal's `org.freedesktop.appearance` namespace spells it `color-scheme` and
`reduced-motion`, niri's own `config.kdl` spells it `focus-follows-mouse`, and CSS spells it
`--accent-bg-color`. A user editing `config.toml` in the same session as `config.kdl` should not have
to switch conventions between them.

The attribute is easy to forget on a new table, and forgetting it is silent — that table would ship
snake-case keys while everything around it is kebab. `every_key_and_enum_value_is_kebab_case` walks
the generated schema and fails on any key or enum value holding an underscore, naming the offenders,
so the omission is a build failure rather than something review has to catch.

## One file, one schema

All four binaries read `config.toml`, and all four link the whole schema and validate the whole
document. Each acts on only the tables it owns; a binary that needs a value from elsewhere gets it
as a topic, not by reading someone else's table.

An unknown key is an error wherever it lands, and the set of top-level table names is closed —
`deny_unknown_fields` on `Config` is what catches a misspelled `[panle]` that every reader would
otherwise ignore. `[applets.<name>]` is no exception: its settings are written flat alongside
`extends`, and they are checked, because `Applet` is an enum with one variant per applet rather than
a free-form table.

## Reading a file

- Symlinks are followed — a `config.toml` pointing into a dotfile repository is the ordinary case.
- The descriptor is inspected after the open, never a path before it: between a `stat` and an `open`
  the path can be replaced.
- Regular files only, capped at 1 MiB. A FIFO is **not** defended against: the open is what blocks,
  so a file symlinked at one hangs the binary.
- `config.d/` is read one level deep, one file open at a time, at most 64 entries. Past that the
  load fails rather than applying a prefix.
- A missing file is an absent layer everywhere in the stack — the base file, `--config`, and a
  drop-in whose symlink target is gone are all optional in the same way. A file that exists and is
  wrong somehow (wrong type, too large, a syntax error) still fails the whole load.

Loading is synchronous. Every binary calls `load` once during startup, before anything else is
running; only the watching below is async, and it puts each re-read on `spawn_blocking` so a caller
holding a stream never has to know that `load` touches the filesystem the blocking way.

## Errors

`load` reports every problem it found, not the first. No `ConfigError` renders any of a file's
content: `toml::de::Error`'s own `Display` prints the offending source line as a snippet, and a
`config.toml` aimed at an SSH key would echo it into the journal, so only the message and the span
are taken and the position is translated in `error.rs`.

A syntax error names file, line and column, and names the drop-in it is in rather than the base file
it merges over. A schema error names the key path instead — it is found in the merged document,
which has no lines to name.

The caller decides what a failure means. At startup that is to log it and come up on
`Config::default()`; on reload it is to drop the update and keep what is running. Neither exits.

## Themes

A theme is a directory of stylesheets under `<root>/<name>/`. The roots are `user_dir()/themes` then
`DATA_DIR/themes`, unless `GLIMPSE_THEMES_DIR` names one, which replaces both — an explicit override
replaces the stack rather than joining it, the same rule `--config` follows for configuration.

Two environment variables sit above the configuration, and both replace rather than join:
`GLIMPSE_THEMES_DIR` chooses the root, and `GLIMPSE_THEME` chooses the name, taking precedence over
`appearance.theme`. The name is applied in `stylesheet`, `theme_dir_for` and `watch_theme` rather
than in `load`, so `Config` keeps reporting the document on disk — a binary told to render one theme
does not start claiming the user configured it. An empty `GLIMPSE_THEME` means the default theme,
exactly as an empty `appearance.theme` does.

**Resolution picks one directory, not one file at a time.** `theme_dir_for(theme)` returns the first
of `user/<theme>`, `data/<theme>`, `user/adwaita`, `data/adwaita` that is a directory, and every sheet
then comes from it. `stylesheet(theme, name)` is that directory joined with the name, kept only if it
is a regular file. Both checks are `Path::is_dir`/`Path::is_file`, which go through `metadata` rather
than `symlink_metadata`, so a symlinked theme directory or a symlinked sheet inside a package
resolves.

The directory is the unit because CSS makes it one. A theme may split its sheets with
`@import url("shared.css")`, and GTK resolves a relative import against the importing file's own
directory, never through this resolver. A theme assembled from two roots therefore cannot import
across them: `user/nord/panel.css` looks for `user/nord/shared.css` and fails, whatever `data/nord`
holds. Per-file resolution was tried and promised that a theme could ship one sheet and inherit the
rest; the import makes that unreachable, so a theme is now all or nothing. Copy the whole directory
to customise one rule.

The shipped `adwaita` theme is three empty files. Component rules and the token vocabulary live in
`glimpse-widgets/styles/glimpse.css`, compiled into every UI binary at `APPLICATION` priority, so a
theme that redefines nothing still renders correctly and a theme that redefines one token changes
only that.

`user_stylesheet()` locates the user's own `styles.css`, which is optional, absent by default, and
not part of any theme — it lives in `user_dir()` and always loads on top of the theme.

`watch_theme(theme)` watches `user_dir()` for that `styles.css`, then every root and every
`<root>/<theme>` beneath it, then the directory resolution actually chose. The roots are not
decoration. `nearest_existing` walks up only as far as directories that are themselves in the
requested set, so a watch armed on the theme directory alone reports `Unavailable` on a machine where
the user has never created one, and a directory created later is never noticed. Passing the roots is
what buys fall-back-and-descend, and it is the shape `watch_dirs_from` already uses for `config.d/`.
Arming a root that does not exist costs nothing: `rearm` reports `Unavailable` only when *every* arm
fails, so an uninstalled `DATA_DIR/themes` leaves the rest of the set working.

The set is fixed at construction, so a caller changing `appearance.theme` drops the stream and builds
another. Watching `user_dir()` for `styles.css` also means a `config.toml` write reports a theme
change, since both live in that directory.

## Watching

Every binary watches its own files and re-reads them itself. No process learns about a change from
another one, so hot reload does not depend on the daemon being alive — which `glimpse-lock`
requires, and which means there is one loading path rather than one for a live daemon and one for a
dead one.

`watch_dirs(config_path)` is the layer stack's *directories*, existing or not — the counterpart to
`resolved_files`, which is its files that do. Normally four: `/etc/glimpse/`, its `config.d/`,
`$XDG_CONFIG_HOME/glimpse/`, and its `config.d/`. `--config` replaces the whole stack with one
file, so it replaces the whole watch set too: that file's directory, and no `config.d/`.

Watches go on directories, never on files. A per-file watch cannot see a drop-in that does not exist
yet, and creating one is exactly the change that has to be noticed. A symlinked base file adds one
more directory, the one holding its resolved target: editors write a new file and rename it over the
old one, so a watch on the link alone goes quiet after the first save.

`watch_all(dirs)` is the primitive, and `watch(dir)` is its one-directory case. Neither is
configuration-specific, which is what will make stylesheets a caller rather than a subsystem.

- Only create, modify and remove events. Access events fire for every read in the directory, this
  process's own included, and describe nothing that changed.
- **Events are coalesced until the directory has been quiet for 250 ms**, because one editor save is
  a write, a rename over the target, and sometimes a delete and a create. Waiting for quiet rather
  than flushing on a fixed window also means a file being rewritten in place is read once it has
  finished, not partway through — a burst of non-atomic writes costs one reload and no parse errors.
- **One inotify instance for the whole set, and no timer.** The coalescing is a `tokio` timeout
  armed only once an event arrives, so an idle session costs nothing: measured at zero wakeups on
  the watching thread over ten seconds. This is why `notify-debouncer-full` is not used — its worker
  is a `loop { sleep(tick); flush }` that cannot be woken early, and the file-identity tracking it
  offers in exchange is something nothing here reads.
- Two directories that are both missing collapse onto the same ancestor and therefore share one
  watch. Releasing a watch addresses it by path, so one is released only once no directory is
  resolving to it.
- A directory that does not exist is watched through its nearest existing ancestor **within the set
  being watched**, and the watch descends when the missing component appears. An absent `config.d/`
  falls back onto the `glimpse/` beside it in the set, which is the ordinary case and not an error.
  An absent `glimpse/` falls back onto nothing: `$XDG_CONFIG_HOME` and `/etc` are written constantly
  by software with no connection to this session, and a watch on either wakes us for every one of
  those writes to report a file that did not change. The configuration directory being created
  therefore needs a restart to be picked up — which is the one moment a restart costs nothing,
  because there was nothing configured to reload. Deleting and restoring it wholesale is caught only
  when both happen inside one debounce window, which is what `rm -rf` immediately followed by a
  re-stow looks like; a restore that arrives seconds later needs a restart too.
- **The watch re-arms when its directory is replaced.** A watch is bound to an inode rather than to
  a name, so `rm -rf ~/.config/glimpse` followed by a fresh clone otherwise leaves one armed on a
  directory nobody can reach: it reports nothing again, ever, and looks exactly like a directory
  where nothing happens. The inode is compared, not just the path, and `Update::Rearmed` says
  "read everything again" — whatever happened during the gap produced no events and cannot be
  inferred.
- The `Debouncer` is owned by the stream, so dropping the stream drops the watch, and a re-arm
  places the new watch before releasing the old one — a kernel that refuses the new one would
  otherwise cost the working watch as well as the descent.

The watch set is derived once, at construction. Re-pointing a symlinked base file at a **different**
directory is therefore caught — the link's own directory is watched — but edits at the new target
are not, until the process restarts. Re-stowing in place, which keeps the target directory, is
unaffected. Re-deriving the set on every event would mean stat-ing the whole stack and rebuilding
every watch, which is a poor trade for a case that only arises when someone moves their dotfile
repository.

`reread` is the step behind each trigger: load the stack off the runtime threads, and answer with
the new document only if it parsed **and differs**. A read that failed yields the reason instead of
logging it, because the same reason repeated is one document nobody has fixed yet — a watched
directory goes on producing events while it stays broken, and an editor's swap and backup files land
right beside it. So the reader keeps the last failure and reports one at `error` only when the
message changes, dropping the repeats to `debug`. A document that *loads* clears it, whether or not
it moved: an undone edit is a fix, and the next break is news again.

`watch_config(config_path, current)` composes the three into a stream of documents, one per real
change, and is what every binary reloads through. It merges two triggers, and neither replaces the
other: the filesystem watch, and `SIGHUP`. An editor whose write inotify never saw still has a way
to apply the change, and a session whose watches the kernel refused still reloads on request.
Failing to register the signal handler is a warning rather than a failure, because the other half of
the pair is still running. Registration happens when `watch_config` is called, so call it from
inside a runtime.

`SIGHUP` is not an `Update`. `watch_all` is a directory watch and will have callers that are not
configuration at all, and "a human asked" is not something a directory did; the two are merged one
level up, on a stream of bare triggers.

Note what is *not* filtered: an event is anything created, modified or removed directly inside a
watched directory, not just `*.toml`. Filtering by extension would drop the creation of `config.d/`
itself, which is one of the changes that most needs noticing. So an unrelated write in the
configuration directory does cost one debounced re-read — and then the equality gate absorbs it,
which is the whole reason that gate is worth more than a content digest here.

## Applets

`Applet` is an internally-tagged enum on `extends`, one variant per applet, so an applet's settings
are part of this document's schema rather than a free-form table nobody validates. A misspelled
setting is a load error naming the table and the key; an `[applets.*]` table naming an applet that
does not exist is a load error listing the ones that do.

**The table name supplies the tag when `extends` is absent.** `[applets.clock]` is the clock; only a
second instance needs `extends`, as in `[applets.clock-utc] extends = "clock"`. A hand-written
`Deserialize` on the `applets` field injects the key before handing the table to serde, which is why
the common case stays free of a line that only restates the name.

The emitted schema mirrors that exactly. `properties` carries one entry per applet keyed by its
name, so an editor resolves `[applets.clock]` to the clock's own schema with no discrimination step;
`additionalProperties` carries the tagged form, where `extends` is required, for aliases. JSON Schema
applies the second only to keys the first did not match, which is the same rule the loader follows.

**A variant with no settings is written `Clock {}`, never `Clock`.** `deny_unknown_fields` has
nothing to deny on a unit variant, so a unit variant silently swallows every key written under it —
the exact failure this design exists to remove. An empty struct variant refuses them.

Resolving is not the same as being implemented. A name that resolves to an applet no binary builds
is an ordinary state, not a bad document, and the panel says so at `debug` rather than `warn` —
which is why `every_applet_named_by_the_default_panels_resolves` guards the shipped defaults, and
why `__dynamic__` was deleted rather than kept as a reserved name.

## Not here

Semantic validation — `HH:MM` parsing, duplicate idle timeouts — which is not written yet. A panel
zone naming an applet that does not exist is caught by the panel at build time, not here; this crate
owns the name→kind mapping and not the question of whether anything implements it.

`[geolocation]` is the exception, and it needs none: the table is one internally tagged enum, so
`provider = "manual"` carries `latitude` and `longitude` in the variant that selects it. A
half-filled table is a `missing field` from serde, naming the key, before any reader sees it. Where
a rule can be expressed in the type it belongs there rather than in a pass that has to remember to
run.
