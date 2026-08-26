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

Nothing here is async. Three GTK binaries link this crate with no tokio runtime; the daemon calls
`load` synchronously during startup, before anything else is running.

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

## Not here

Watching, which belongs to the daemon's `watcher` service. Semantic validation — `HH:MM` parsing,
`provider = "manual"` without coordinates, duplicate idle timeouts, a panel zone naming an applet
that does not exist — which is not written yet.
