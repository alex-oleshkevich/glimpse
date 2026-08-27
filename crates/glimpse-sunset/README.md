# glimpse-sunset

The night-light binary: applies a color temperature to every output on a schedule the daemon
computes.

## Contents

- `main.rs` — `run(cli) -> anyhow::Result<()>`, with `main` turning the outcome into an `ExitCode`
- `cli.rs` — the argument surface, flattening the shared structs from `glimpse-utils`

## Status

A stub. `run` loads the configuration and opens a client with `Client::open`, which reconnects on
its own and survives a daemon that is not up yet. Nothing reads a topic through it: the schedule
this binary renders is `solar.status`, and subscribing to it is pointless until there is gamma
control to apply the phase to. The client is bound to `_client` so it lives to the end of `run` —
the connection task stops when the last handle drops.

What it does do is reload: the document is re-read through `glimpse_config::watch_config`, so both
`SIGHUP` and a change under the configuration directory apply it, and `glimpse-sunset.service`
carries `ExecReload=`. That is parity with the other four long-lived binaries rather than growth —
the loop is the same handful of lines each of them has, and it goes with the crate if the crate
goes.

**Whether this crate should exist at all is an open question.** It was once decided that night light
becomes a daemon service and this binary goes, but the crate is still in the workspace and
`glimpsed`'s packaging assets still ship the binary. Resolve that before building anything on it.
Do not grow this crate while the question is open.

## Rules

Gamma control is exclusive — one client at a time. A user already running `wlsunset`, `gammastep`
or `hyprsunset` is the case to detect and step aside from, rather than flicker against.

The schedule is not this binary's to compute. `solar` derives sunrise and sunset from the location
and publishes them as a topic; this binary renders that decision and holds no schedule of its own.
