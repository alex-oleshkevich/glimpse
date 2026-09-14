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
- `render.rs` — `Table`, `Section`, the `styled` colours, and the one `print` everything leaves
  through
- `errors.rs` — the `Exit` table and the one D-Bus-error-name to exit-code mapping

## Rules

**There are no generic commands.** `get`, `watch`, `call`, `topics`, `methods`, `services` and
`monitor` are gone with the socket they addressed. Every command names one operation on one
provider, with a typed request and a typed result, so the help text is the whole surface rather
than a starting point for guessing topic names.

**Every proxy is built with `CacheProperties::No`.** zbus caches a property and answers later reads
from the first until a `PropertiesChanged` arrives. A process that reads once and exits gains
nothing from the cache and loses correctness to it: `sunset mode off` followed by `sunset status`
would print the mode it had just replaced. Measured — it is what made the provider's own test fail
before the flag went in.

**`--json` is global and every command that prints anything honours it.** Where availability is not
part of the payload a provider publishes, the JSON wraps it: an empty weather list because nothing
is watched reads the same as an empty list because the provider is down, unless `available` and
`reason` sit beside it. `config show` and `config path` serialize the document and the resolved
file list, and `doctor` serializes its whole diagnosis — a global flag that some commands silently
ignored would be worse than no flag, because a script cannot tell the difference.

**`weather` has no `add` or `remove`.** A place is a lease held by whoever wants to see it, renewed
on a thirty-minute clock — `glimpse-config`'s `[weather]` table has no `places` key and refuses one.
A CLI process exits, so anything it watched ages out within half an hour, and anything it unwatched
comes back on the panel's next renewal. Watching is for a process that stays; the places are the
ones the weather applets name in `[applets.*]`.

**`sunset mode` changes the mode in force and never the document.** It lasts until `[night-light]`
is edited or `glimpse-sunset` restarts, and `status` says `(override)` so a reader is not sent
looking for it in a file that does not say it.

**`doctor` asks `NameHasOwner` before it reads anything.** `me.aresa.Glimpse.Weather` and
`…Notifications` ship D-Bus activation files naming a systemd unit, so calling a method on an
unowned one *starts it*. A `doctor` that read the property directly would launch the providers it
exists to report on and then call them healthy.

**Provider text is capped and stripped before it reaches the terminal.** The night light's snapshot
is deserialized straight off the wire with no decode step to cap it, and the owner of a well-known
name is whoever claimed it — so `schedule` and every `reason` go through `safe()` on the way to a
column. Untrusted text reaching a terminal is worse than untrusted text reaching a label: an escape
sequence repaints the screen.

**`notifications dnd` takes no expiry.** The store holds `until` and nothing acts on it — no timer,
no read-time check — so a lapse time would be accepted and never honoured. `dnd on` stands until
`dnd off`. `glimpse-kyt0.9.10` is the missing half.

**`config` and `doctor` open no bus.** `config` reads the layered stack from disk, which is what
makes it work when a provider is what will not start. `doctor` connects but tolerates failure, for
the same reason: a command that exists to diagnose a missing provider cannot require one, so an
absent one is a finding it prints and exits 0 on.

**`doctor` reports what a provider says about itself, not merely that it answered.** A provider
owning its name while reporting `serving = false` is the case a bare ownership check calls healthy
— the night light that lost gamma control to another client is exactly that.

**Everything drawn goes through `render.rs`.** `render::print` is the single place a line is
written, so `BrokenPipe` is handled once rather than per command. `Table` takes `[String; N]` rows,
so a row that does not match its headers is a compile error rather than a ragged table, and width
is measured on the visible text so a styled cell never shifts a column.

**`exit` maps D-Bus error names, and holds only the codes something returns today.** The numbers
kept their meanings when the socket went — 3 is still "the thing that answers is not there", 4 is
still "it answered and said no to this" — so a script written against the old table still reads
correctly. An unlisted name is 1 rather than a guess. 2 will never be there, because clap owns it.

**`message` is the display half of that same one mapping site.** `zbus::Error::FDO` wraps a
`zbus::fdo::Error` whose text it already contains *and* exposes as `source()`, so `{error:#}` prints
a property read's failure twice while a method call's prints once. Every property in this crate is
read through that wrapper, so the repair belongs beside `exit` rather than at each call site:
`message` walks the chain and drops a link the previous one has already said. Measured against a
live bus with no provider — `sunset status` printed `ServiceUnknown` twice, `sunset mode` once.

Colour resolution is `anstream`'s, so `NO_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE` and `TERM` are
honoured without any detection of our own. Errors go to stderr; only requested data goes to stdout.

## Known gaps

Column width counts characters, not display columns, so a wide glyph — CJK, an emoji — is measured
one short and hangs its row. The exposure is the summary column of `notifications list`, which is
third-party text. `unicode-width` is the fix and has not been proposed yet.

