# glimpsectl

The suite's command line. It addresses the three standalone providers over their own typed D-Bus
interfaces, and reads the configuration stack straight from disk.

```bash
glimpsectl sunset status
glimpsectl sunset mode off
glimpsectl weather status --json
glimpsectl weather refresh
glimpsectl notifications list
glimpsectl notifications dismiss 7
glimpsectl notifications clear --app org.mozilla.firefox
glimpsectl notifications dnd on
glimpsectl config show
glimpsectl doctor
```

## Contents

- `main.rs` — flag resolution, the session-bus connection, subcommand dispatch, exit codes
- `cli.rs` — the argument surface and `needs_session_bus`
- `commands/` — one module per command group, each printing its own output
- `render.rs` — `Table`, `Section`, the `styled` colours, and the one `print` everything leaves through
- `errors.rs` — the `Exit` table and the one D-Bus-error-name to exit-code mapping

## Rules

**There are no generic commands.** Every command names one operation on one provider, with a typed
request and a typed result, so the help text is the whole surface rather than a starting point for
guessing topic names. **Every proxy is built with `CacheProperties::No`**, since a process that
reads once and exits gains nothing from a property cache and loses correctness to it: `sunset mode
off` followed by `sunset status` would print the mode it had just replaced. **`--json` is global and
every command that prints anything honours it**, wrapping availability itself when it is not part
of the payload, so an empty result from nothing watched cannot be confused with an unreachable one.

**`weather` has no `add` or `remove`.** A place is a lease renewed on a thirty-minute clock, so
anything a CLI process watched ages out on its own once it exits; the places to keep live in the
applets that name them, not in the CLI. **`sunset mode` changes the mode in force and never the
document** — it lasts until `[night-light]` is edited or `glimpse-sunset` restarts, and `status`
says `(override)` so a reader is not sent looking for it in a file that does not say it.

**Provider text is capped and stripped before it reaches the terminal.** Untrusted text reaching a
terminal is worse than untrusted text reaching a label: an escape sequence repaints the screen.
**Everything drawn goes through `render.rs`** — a single `print` handles `BrokenPipe` once rather
than per command, and `Table` takes `[String; N]` rows so a mismatched row is a compile error.
**`notifications dnd --until HH:MM` is the next time the clock reads that way**, sent as epoch
microseconds since that is what the provider reads back (`0` means indefinite), and it is refused on
`dnd off`, where there is nothing to lapse.

**`config` reads the layered stack from disk and opens no bus**, which is what makes it work when a
provider will not start. **`doctor` connects but tolerates failure**, asks `NameHasOwner` first — a
provider is D-Bus activatable, so a direct call would start the process it exists to report on —
probes all three concurrently, and tells a provider that is gone apart from one that owns its name
but reports `serving = false` or fails to reply, both `degraded` with the reason.

**Every call to a provider is bounded, because zbus does not bound one for you** — a peer that owns
its name and never replies leaves `Proxy::call` awaiting forever, so each request goes through
`within` on the same `glimpse_dbus::DEADLINE` the panel uses, and a timeout maps to `Exit::Timeout`.
**`exit` maps D-Bus error names and holds only the codes something returns today**, with an unlisted
name mapping to 1 and 2 reserved for clap; `message` is the display half of the same mapping —
`zbus::Error::FDO` already exposes its text as `source()`, so a naive print doubles it, and
`message` walks the chain dropping what the previous link already said.

Colour resolution is `anstream`'s, so `NO_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE` and `TERM` are
honoured without any detection of our own; errors go to stderr, only requested data to stdout.
Column width in `render.rs` counts characters, not display columns, so a wide glyph in third-party
text — CJK, an emoji, as in a notification summary — is measured one short and hangs its row.

