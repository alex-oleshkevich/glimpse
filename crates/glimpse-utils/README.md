# glimpse-utils

The argument structs and logging setup every binary repeats, written once.

## Contents

- `args.rs` — `LogArgs`, `ConfigArg`, `SocketArg`, flattened into each binary's clap `Cli`
- `log.rs` — `LogFormat` and `init_app_tracing`

## What it holds

Three `clap::Args` structs, all `global = true` so they may follow a subcommand, each carrying the
environment variable that is its default:

| Struct      | Flag             | Environment            |
| ----------- | ---------------- | ---------------------- |
| `SocketArg` | `-s`, `--socket` | `GLIMPSED_SOCKET_PATH` |
| `ConfigArg` | `-c`, `--config` | `GLIMPSE_CONFIG_PATH`  |
| `LogArgs`   | `--log`          | `RUST_LOG`             |
|             | `--log-format`   |                        |

The environment variable is declared on the flag rather than read separately, so it shows in
`--help`, is testable without mutating the process environment — `unsafe` under edition 2024 — and
cannot acquire a second spelling somewhere else.

`init_app_tracing(level, format)` builds the subscriber. An invalid filter warns and falls back to
`info` rather than aborting: the value is inherited from `RUST_LOG`, and a stale entry in somebody's
profile must not stop a binary from starting.

## Rules

**Color is resolved, not detected.** `init_app_tracing` asks `anstream` what stderr should do, so
`NO_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE` and `TERM` are honored without any detection here. It asks
about stderr specifically, because that is where logs go — asking about stdout would strip color
from logs the moment output is redirected to a file.

**Call `write_global()` before `init_app_tracing`.** A `fmt` subscriber fixes its ANSI setting when
it is built, so a color override applied afterwards reaches nothing.

**Nothing here is domain logic.** No config schema, no topics, no socket. This crate exists so six
binaries agree on what `--log` means, not as a place for code that has no other home.

Spec: [`specs/002_structure.md`](../../specs/002_structure.md)
