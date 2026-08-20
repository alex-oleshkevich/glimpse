# glimpsectl

Command-line and TUI client for `glimpsed`.

```bash
glimpsectl get battery.status --field percentage
glimpsectl watch 'network.**' --json
glimpsectl call audio.set_volume volume=0.42
glimpsectl services
glimpsectl doctor
```

## Subcommands

`get`, `watch`, `call`, `topics`, `services`, `config show|validate|path`, `doctor`, `monitor`.

## Rules

Human output is aligned and coloured on a terminal and plain when piped. `--json` emits exactly the
daemon's payload, one object per line for `watch`, so `jq` works without unwrapping. Errors go to
stderr; only requested data goes to stdout.

Exit codes distinguish a failed command from an unreachable daemon from a usage error, so scripts
branch without parsing prose.

This is the first client to build. It exercises the protocol before any GTK exists.

Spec: [`specs/007_glimpsectl.md`](../../specs/007_glimpsectl.md)
