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
- `commands.rs` — one function per subcommand, each printing its own output
- `errors.rs` — the `Exit` table and the one `anyhow::Error` to exit-code mapping

`get`, `watch`, `call`, `topics`, `services` and all three `config` subcommands are wired. `doctor`
and `monitor` still report that they are not implemented and exit 1, so a script cannot read
"did nothing" as "worked".

`topics` and `services` are `get` on `system.topics` and `system.services` rather than frames of
their own — the daemon already has to publish that state, and a second way to ask for it would be a
second thing to keep true. Both refuse today because no service declares those topics yet.

## Subcommands

`get`, `watch`, `call`, `topics`, `services`, `config show|validate|path`, `doctor`, `monitor`.

## Rules

`--socket` names one outright; otherwise `glimpse_ipc::socket_path` derives it from
`XDG_RUNTIME_DIR`. The three `config` subcommands resolve no socket at all — they read the layered
stack straight from disk via `glimpse_config::load`/`resolved_files`, which is what makes them work
when the daemon is what will not start. `doctor` resolves none either, for the same reason: a
command that exists to diagnose a missing daemon cannot require one.

`config show` prints the merged document, TOML by default and JSON under `--json`; `config validate`
loads the stack and reports the first problem `glimpse-config` finds; `config path` lists the
resolved files in order. Reporting every problem rather than the first, and marking each path
found or missing, wait on `glimpse-config` returning more than one error.

`--timeout` is handed to `Client::connect` rather than wrapped around each request, because a
caller that abandons its own future does not release the in-flight slot the request still holds.

`exit` holds only the codes something returns today; the rest arrive with the code that returns
them. 2 will never be there, because clap owns it.

Color resolution is `anstream`'s, so `NO_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE` and `TERM` are honored
without any detection of our own; `--color` sets the global override before anything writes. Human
output is pretty-printed JSON today and is not yet aligned or colored — that arrives with the
payload types there is something to align.

`--json` emits exactly the daemon's payload, one object per line for `watch`, so `jq` works without
unwrapping. Errors go to stderr; only requested data goes to stdout, and every subcommand that
streams treats a closed pipe as a clean exit rather than panicking out of `println!`.

Exit codes distinguish a failed command from an unreachable daemon from a usage error, so scripts
branch without parsing prose.

This is the first client to build. It exercises the protocol before any GTK exists.

Spec: [`specs/007_glimpsectl.md`](../../specs/007_glimpsectl.md)
