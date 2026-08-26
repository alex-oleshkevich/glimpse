# glimpse-sunset

The night-light binary: applies a color temperature to every output on a schedule the daemon
computes.

## Contents

- `main.rs` — `run(cli) -> anyhow::Result<()>`, with `main` turning the outcome into an `ExitCode`
- `cli.rs` — the argument surface, flattening the shared structs from `glimpse-utils`

## Status

A stub. `run` loads the configuration and resolves the socket path, then returns — it opens no
connection to the daemon, subscribes to nothing, and touches no gamma control. Nothing here does
what the name says yet.

**The specs and the tree disagree about whether this crate should exist at all.**
`specs/002_structure.md` records `2026-08-20 — dropped glimpse-sunset`, but the crate is still in
the workspace and `glimpsed`'s packaging assets still ship the binary. Resolve that before building
anything on it: either the crate goes and night light becomes a daemon service, or the spec entry is
wrong and needs reverting. Do not grow this crate while the question is open.

## Rules

Gamma control is exclusive — one client at a time. A user already running `wlsunset`, `gammastep`
or `hyprsunset` is the case to detect and step aside from, rather than flicker against.

The schedule is not this binary's to compute. `solar` derives sunrise and sunset from the location
and publishes them as a topic; this binary renders that decision and holds no schedule of its own.

Spec: [`specs/001_architecture.md`](../../specs/001_architecture.md)
