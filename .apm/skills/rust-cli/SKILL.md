---
name: rust-cli
description: Command-line binaries in Rust with clap 4, anyhow, thiserror and tracing — the main/run split, the cli.rs / commands.rs / errors.rs layout, nested subcommands, global flags, exit-code mapping, color resolution and log-format selection. Invoke whenever task involves any interaction with a binary crate's command line: adding a subcommand or a flag, wiring exit codes, initializing tracing, handling errors in main, deciding what goes on stdout against stderr, or reviewing a main.rs, cli.rs, commands.rs or errors.rs.
---

# Rust CLI

Every command-line binary has the same skeleton. Not for tidiness — the shape falls out of two
facts that cannot be argued with:

- **`?` does not work in a function returning `ExitCode`.** `ExitCode` implements no `Try`, so the
  work has to happen somewhere else and `main` translates the outcome. That single constraint
  produces the `main`/`run` split, and everything else hangs off it.
- **A binary's failures all end at one message and one exit code.** Nobody matches on them, so the
  error type is `anyhow::Error`. A library's failures are matched on, so those are `thiserror`
  enums. Mixing the two the other way round is the most expensive mistake in this file.

**Core principle:** `main` decides _nothing_. It parses, initializes output, calls `run`, and maps
the result. Every decision that depends on what the user asked for lives past that boundary.

## References

- **complete four-file skeleton** — `${CLAUDE_SKILL_DIR}/references/skeleton.md`
  Copy-pasteable `main.rs`, `cli.rs`, `commands.rs` and `errors.rs` for a new binary, with the
  nested-subcommand and global-flag forms filled in. Read when starting a binary from nothing.

## The four files

A binary crate splits along what changes for different reasons. Anything else grows into a 400-line
`main.rs` that nobody will refactor.

- **`main.rs`** — `#[tokio::main] async fn main() -> ExitCode`, `run`, and `init_tracing`. Nothing
  else. If it grew a third concern, that concern has a file.
- **`cli.rs`** — the clap types and only those: `Cli`, the `Command` enum, nested subcommand enums,
  value parsers, and the tests that exercise the argument surface.
- **`commands.rs`** — one function per subcommand. Each does the work and prints its own output.
- **`errors.rs`** — the exit-code table and the one function that maps an `anyhow::Error` onto it.

Split `commands.rs` into `commands/` with a module per subcommand once a handler stops fitting on a
screen. Do not split it before that.

## main and run

`main` is allowed four statements. `run` takes `Cli` **by value**, so handlers receive owned
`String`s with no clone — the process is about to exit, nothing else wants them.

```rust
#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    if cli.no_color {
        anstream::ColorChoice::Never.write_global();
    }
    let color = anstream::AutoStream::choice(&std::io::stderr()) != anstream::ColorChoice::Never;
    init_tracing(&cli.log, cli.log_format.resolve(…), color);

    match run(cli).await {
        Ok(()) => Exit::Ok.into(),
        Err(error) => {
            report(&error);
            errors::exit(&error).into()
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Command::Get { key, field } => commands::get(key, field, cli.json).await,
        …
    }
}
```

- **Resolve the color choice before building the subscriber.** A `fmt` subscriber fixes its ANSI
  setting at `.init()`, so a `write_global` afterwards reaches nothing.
- **Do work in `run`, never in `main`.** Loading configuration, opening a socket or reading a file
  in `main` runs it for every subcommand, including the ones that do not need it.
- **Acquire a shared resource once, in `run`, and only when the command needs it.** Put the "does
  this command need it?" question on the command enum as a method, so it is one tested statement
  rather than a condition repeated per arm.

## cli.rs

- **Every flag that may follow a subcommand is `global = true`.** Users add flags by pressing
  up-arrow and appending. One non-global flag among globals is a usage error that looks like a bug.
- **The environment variable is the flag's default: `env = "NAME"`.** Reading the same variable
  again in a library is how two names for one setting appear. clap's form shows in `--help` and is
  testable without mutating the process environment, which edition 2024 makes `unsafe` — correctly,
  since the test harness is threaded.
- **`help = "…"` on every argument.** These are user-facing strings, not comments.
- **Nest subcommands with a second `Subcommand` enum**, `Config(ConfigCommand)`, not with a string
  argument. Nesting deeper than two levels means the second level wanted to be a flag.
- **Custom `value_parser` functions return `Result<T, String>`** and describe what was expected:
  `expected KEY=VALUE, got "port"`. clap prints it and exits 2 for you.
- **Put facts about a command on the command.** `fn needs_backend(&self) -> bool` next to the enum is
  testable and has one home; the same `matches!` scattered through `run` is neither.

## commands.rs

- **One `pub` function per subcommand, named for it.** `get`, `watch`, `config_path` — not
  `get_the_value` or `show_all_resolved_config_file_paths`.
- **Owned arguments, plus the presentation flags the handler needs.** `key: String`, `json: bool`.
- **Every handler returns `anyhow::Result<()>` and prints its own output.** Rendering is the
  handler's job; `main` has no idea what a record looks like.
- **Stub with `bail!("get is not implemented yet")`, never `todo!()`.** `todo!()` panics: a
  backtrace on stderr and exit 101, which no exit-code table contains. `bail!` exits 1 with a
  sentence, and a script reading the code is not misled.
- **Underscore-prefix parameters a stub does not use yet.** Clippy with `-D warnings` fails on an
  unused parameter, and an `#[allow]` added here would outlive the stub.

## errors.rs and exit codes

**The exit table is an enum, not loose integers.** It is a closed set the caller branches on, which
is the same test that puts a `thiserror` enum in a library. `std::process::ExitCode` is opaque — it
implements neither `PartialEq` nor any way to read the number back — so an enum is also the only
form the mapping can be tested in. Convert at the boundary with one `From` impl.

```rust
use std::process::ExitCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    Ok = 0,
    Failed = 1,
    Unreachable = 3,
    Timeout = 5,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        Self::from(exit as u8)
    }
}

pub fn exit(error: &anyhow::Error) -> Exit {
    match error.downcast_ref::<ConnectError>() {
        Some(ConnectError::NotListening { .. }) => Exit::Unreachable,
        Some(ConnectError::Timeout { .. }) => Exit::Timeout,
        _ => Exit::Failed,
    }
}
```

- **The variants are the binary's specified table, named after it.** A number written at a call site
  is a code no script can rely on; a variant cannot be invented without editing the table.
- **`#[repr(u8)]` with explicit discriminants** so the numbers are visible in the type and `as u8`
  is exact. The `From` impl is the only place a number is produced.
- **Leave 2 out.** clap exits with it itself, so a variant for it would never be constructed — and
  in a binary crate that is a dead-code warning.
- **Carry only the codes something returns today.** The rest arrive with the type that returns them.
- **`downcast_ref` sees through `.context(…)` layers.** That is the property worth a test: adding
  context upstream must not change which code a script sees.
- **A binary-local marker type is the one place `thiserror` belongs inside a binary** — when `main`
  genuinely branches on it. One unit struct with an `#[error("…")]` beats an enum of `String`s.

## Output channels

- **stdout is data, stderr is diagnostics.** Only what was asked for goes to stdout. A user piping
  to `jq` must not receive a progress line.
- **`--json` emits exactly the payload**, one object per line for streaming subcommands, so `jq`
  works without unwrapping.
- **Pass `json: bool` to handlers.** Do not convert it to a `"json"` / `"text"` string first — that
  is a bool with extra steps and a `_ =>` arm that cannot happen.
- **An interactive tool reports failures with `anstream::eprintln!`, not `tracing::error!`.** A
  failure is the answer; routing it through the log filter means `--log warn` returns a non-zero
  exit with no explanation. A daemon is the opposite: it runs under systemd, the journal is what
  anyone reads, so `tracing::error!` is right there.
- **Print with `{error:#}`.** The alternate `Display` flattens the whole `anyhow` context chain onto
  one line; plain `{error}` prints the outermost context and silently drops the cause.
- **A streaming subcommand handles `BrokenPipe`.** Rust sets `SIGPIPE` to ignore at startup, so
  `println!` into a closed pipe returns `Err` and the macro panics — `watch … | head -1` exits 101
  with a backtrace. Write with `writeln!(stdout(), …)` and treat `ErrorKind::BrokenPipe` as a clean
  exit.

## Color

Resolution is `anstream`'s, never hand-written. It already honors `NO_COLOR`, `CLICOLOR`,
`CLICOLOR_FORCE`, `TERM` and whether the stream is a terminal.

```rust
if cli.no_color {
    anstream::ColorChoice::Never.write_global();
}
let color = anstream::AutoStream::choice(&std::io::stderr()) != anstream::ColorChoice::Never;
```

- **Query the stream you are about to write to.** Logs go to stderr, so ask stderr. Asking stdout
  strips color from logs the moment output is redirected to a file.
- **`write_global` covers `anstream::println!` and nothing else.** `tracing_subscriber` does not
  consult it, so the resolved boolean has to be handed to `init_tracing` explicitly.

## Log format and tracing

Three formats, resolved from one flag plus the environment:

```rust
pub enum LogFormat { Auto, Plain, Json }   // the --log-format value
pub enum LogSink { Terminal, Journal, Json }  // what the subscriber does

impl LogFormat {
    // An empty variable means unset; units and shells both export empty values.
    pub fn resolve(self, journal_stream: Option<&OsStr>) -> LogSink {
        let under_journal = journal_stream.is_some_and(|value| !value.is_empty());
        match self {
            Self::Json => LogSink::Json,
            Self::Plain => LogSink::Terminal,
            Self::Auto if under_journal => LogSink::Journal,
            Self::Auto => LogSink::Terminal,
        }
    }
}
```

- **Two enums, not one.** The flag is what the user typed; the sink is what was decided. Collapsing
  them makes `auto` a value the subscriber has to interpret at every use.
- **`resolve` takes the environment as a parameter.** That is what makes `auto` testable.
- **`auto` under a journal drops timestamps and color** — `JOURNAL_STREAM` is set, and the journal
  stamps and colors its own lines, so ours arrive doubled.
- **An invalid `--log` filter warns and falls back to `info`; it never aborts.** The value is
  inherited from `RUST_LOG`, and a stale entry in somebody's profile must not stop the program.

```rust
fn init_tracing(filter: &str, sink: LogSink, color: bool) {
    let env_filter = tracing_subscriber::EnvFilter::try_new(filter).unwrap_or_else(|error| {
        eprintln!("<binary>: ignoring invalid log filter {filter:?}: {error}");
        tracing_subscriber::EnvFilter::new("info")
    });

    let builder = tracing_subscriber::fmt().with_env_filter(env_filter);
    match sink {
        LogSink::Terminal => builder.with_ansi(color).init(),
        LogSink::Journal => builder.without_time().with_ansi(false).init(),
        LogSink::Json => builder.json().init(),
    }
}
```

The binary name in that message is the one thing this pattern gets wrong when copied between
crates. Check it.

## Tests worth having

Argument surfaces rot silently, so `cli.rs` carries its own tests. These four earn their place:

- `Cli::command().debug_assert()` — catches a duplicated short flag or a malformed `value_parser`
  at test time rather than at first run.
- A missing required subcommand exits 2, asserted through `err.exit_code()`.
- Each custom `value_parser`, on its awkward input: `where={"x":1}` splits on the _first_ `=` only.
- `exit(&error)` for each mapped variant, and once through a `.context(…)` layer. The `Exit` enum
  derives `PartialEq` and `Debug`, so assert on it directly — that is what it is for.

## Application

**Writing:** start from `references/skeleton.md`, delete what the binary does not need, and add
subcommands to `cli.rs` and `commands.rs` in the same edit. A subcommand that parses but has no
handler is a `bail!`, not a missing match arm.

**Reviewing:** check the stdout/stderr split first — it is the most common defect and the one that
breaks other people's scripts. Then check that the exit codes match the specified table, that no
flag usable after a subcommand is missing `global = true`, and that no value parsed from the
environment is read in two places.

## Bottom line

- `main` parses, initializes, calls `run`, maps the result. Nothing else.
- `anyhow` in the binary, `thiserror` in the library it calls.
- One exit-code mapping site, returning an `Exit` enum that is the specified table, converted to
  `ExitCode` by a single `From` impl.
- stdout carries data, stderr carries everything else, and `--json` carries the payload verbatim.
