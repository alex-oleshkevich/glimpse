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
schema's enum values, so `night_light.schedule = "manual"` still parses but an editor will flag it;
`"schedule"` is the spelling the schema expects.

## One file, one schema

All four binaries read `config.toml`, and all four link the whole schema and validate the whole
document. Each acts on only the tables it owns; a binary that needs a value from elsewhere gets it
as a topic, not by reading someone else's table.

An unknown key is an error wherever it lands, and the set of top-level table names is closed —
`deny_unknown_fields` on `Config` is what catches a misspelled `[panle]` that every reader would
otherwise ignore. The exception is any key besides `extends` in `[applets.<name>]`, whose shape
belongs to the applet type rather than to this schema — written flat, alongside `extends`, not
nested under a `.settings` sub-table.

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

`reread(config_path, current)` is the step every consumer takes when something moved: load the
stack off the runtime threads, and answer with the new document only if it parsed **and differs**.
A read that failed is logged and yields nothing, so the caller keeps what is already running. Both
the logging and the equality gate live here rather than once per binary.

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

## Not here

Semantic validation — `HH:MM` parsing, `provider = "manual"` without coordinates, duplicate idle
timeouts, a panel zone naming an applet that does not exist — which is not written yet.
