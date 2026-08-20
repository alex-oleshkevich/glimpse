# glimpse-config

Layered TOML configuration, shared by the daemon and every UI binary.

## What it does

- Resolves the layered stack: built-in defaults, system, system drop-ins, user, user drop-ins, CLI
- Merges `config.d/*.toml` drop-ins over the base file at each level, in lexical order
- Merges layers and validates the result
- Watches every resolved file and the drop-in directories, and reports changes for hot reload
- Follows symlinks, then checks what they land on: regular files only, inspected through the open
  descriptor rather than by a prior `stat`, capped at 1 MiB
- Reads drop-in directories one level deep, one open file at a time, at most 64 per directory
- Watches directories rather than files, at most six, and degrades to no hot reload rather than
  failing when the kernel refuses another watch
- Reports the exact location of a parse or validation error

An invalid edit never costs the user a working session. At startup the binary logs the error and
comes up on defaults; on reload the update is dropped and the running configuration stays. Neither
exits — only `--check-config` does. Both outcomes are reported with the error's location.

## One file, one owner per table

All four binaries read `config.toml`. Each reads only the tables it owns — one table per service,
named for the service, for the daemon; `[panel]`, `[wallpaper]` and `[lock]` for the UI binaries —
and ignores the rest. A binary never reads another binary's tables; if it needs a
value from elsewhere, that value is a topic.

Two rules pull in opposite directions on purpose. An unknown key **inside an owned table** is an
error, so a typo is loud. The contents of a table someone else owns are ignored, so the schemas
version independently. The set of top-level table names is closed and lives here, which is what
catches a misspelled `[panle]` that would otherwise be ignored by every reader.

Merging is per key: tables merge, scalars replace, and **arrays replace rather than append** — an
appending array could never be shortened by a later layer.

Spec: [`specs/010_configuration.md`](../../specs/010_configuration.md)
