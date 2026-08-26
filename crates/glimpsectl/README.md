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
- `commands/` — one module per subcommand, each printing its own output
- `render.rs` — `Table`, `Section`, payload rendering, the `styled` colours, and the one `print` everything leaves through
- `errors.rs` — the `Exit` table and the one `anyhow::Error` to exit-code mapping

`monitor` is the only subcommand still unimplemented; it reports so and exits 1, so a script cannot
read "did nothing" as "worked". `doctor` covers what a client can determine — the socket, the
configuration stack, and every service with its state — and exits 0 whatever it finds, because
diagnosing a broken session is the command succeeding, not failing. The compositor, the Wayland
protocols and backend availability are the daemon's knowledge and reach `doctor` only through the
states on `system.services`.

`topics` filters on two axes that compose: a positional pattern narrows by topic name, `--owner`
narrows by owning service, and an empty result names whichever one emptied it.

`topics` and `services` are `get` on `system.topics` and `system.services` rather than frames of
their own — the daemon already has to publish that state, and a second way to ask for it would be a
second thing to keep true.

## Rules

**Rendering is for people; `--json` is a passthrough.** `get` and `watch` take `--json`, which
prints what the daemon sent and nothing else — the payload for `get`, one event frame per line for
`watch`. It is per-command, not global, so it cannot appear after a subcommand that would ignore it.
No other subcommand has one: `topics` and `services` render the daemon's own introspection, and a
consumer wanting those as data reads `system.topics` and `system.services` with `get --json`.

**Everything drawn goes through `render.rs`**, including reaching stdout: `Table` and `Section`
carry `.print()`, and `render::print` is the single place a line is written, so `BrokenPipe` is
handled once rather than per command. `Table` aligns columns and takes `[String; N]` rows,
so a row that does not match its headers is a compile error rather than a ragged table; `with_empty`
gives it something to say when there are no rows. `Section` is a heading over indented content and
an optional note, and takes content already rendered, so it composes with a table, with `lines`, or
with a sentence. Width is measured on the visible text, so a styled cell never shifts a column.

**`--socket` names one outright**; otherwise `glimpse_ipc::socket_path` derives it from
`XDG_RUNTIME_DIR`. The three `config` subcommands resolve no socket at all — they read the layered
stack straight from disk via `glimpse_config::load`/`resolved_files`, which is what makes them work
when the daemon is what will not start. `doctor` connects but tolerates failure, for the same
reason: a command that exists to diagnose a missing daemon cannot require one, so an unreachable
socket is a finding it prints, not an error it exits on.

**`--timeout` is handed to `Client::connect`** rather than wrapped around each request, because a
caller that abandons its own future does not release the in-flight slot the request still holds.

**`exit` holds only the codes something returns today**; the rest arrive with the code that returns
them. 2 will never be there, because clap owns it.

Colour resolution is `anstream`'s, so `NO_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE` and `TERM` are
honoured without any detection of our own; `--color` sets the global override before anything
writes, and every line leaves through one `write_line` on `anstream::stdout()`, so a pipe gets plain
text without any command asking whether it is being piped.

Errors go to stderr; only requested data goes to stdout, and every subcommand that streams treats a
closed pipe as a clean exit rather than panicking out of `println!`.

This is the first client to build. It exercises the protocol before any GTK exists.

## Known gap

Column width counts characters, not display columns, so a wide glyph — CJK, an emoji — is measured
one short and hangs its row. Topic and service names are ASCII by convention; the exposure is the
key column of `get` on a payload whose keys come from another application. `unicode-width` is the
fix and has not been proposed yet.

Spec: [`specs/007_glimpsectl.md`](../../specs/007_glimpsectl.md)
