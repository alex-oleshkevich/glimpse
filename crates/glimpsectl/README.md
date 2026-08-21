# glimpsectl

Command-line and TUI client for `glimpsed`.

```bash
glimpsectl get battery.status --field percentage
glimpsectl watch 'network.**' --json
glimpsectl call audio.set_volume volume=0.42
glimpsectl services
glimpsectl doctor
```

## Contents

- `main.rs` — global-flag resolution, subcommand dispatch, exit codes
- `cli.rs` — the argument surface and `KEY=VALUE` splitting

Only the command line layer exists so far: nothing connects to the socket. Every subcommand parses
its arguments, resolves the globals and reports that it is not implemented, exiting 1 rather than 0
so a script cannot read "did nothing" as "worked".

## Subcommands

`get`, `watch`, `call`, `topics`, `services`, `config show|validate|path`, `doctor`, `monitor`.

## Rules

`--socket` names one outright; otherwise `glimpse_ipc::socket_path` discovers the first socket that
is on disk. `config validate` and `config path` resolve no socket at all — they read the stack from
disk, which is what makes them work when the daemon is what will not start.

`exit` holds only the codes something returns today; the rest arrive with the code that returns
them. 2 will never be there, because clap owns it.

Human output is aligned and colored on a terminal and plain when piped. The choice comes from
`anstream`, so `NO_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE` and `TERM` are honored without any detection
of our own; `--no-color` sets the global override before anything writes. `--json` emits exactly the
daemon's payload, one object per line for `watch`, so `jq` works without unwrapping. Errors go to
stderr; only requested data goes to stdout.

Exit codes distinguish a failed command from an unreachable daemon from a usage error, so scripts
branch without parsing prose.

This is the first client to build. It exercises the protocol before any GTK exists.

Spec: [`specs/007_glimpsectl.md`](../../specs/007_glimpsectl.md)
