# glimpsectl

The suite's command line. It addresses providers through typed D-Bus interfaces, controls panel
applets through GTK actions, and reads the configuration stack straight from disk.

```bash
glimpsectl night-light status
glimpsectl night-light schedule off
glimpsectl weather status --json
glimpsectl weather refresh
glimpsectl notifications list
glimpsectl notifications dismiss 7
glimpsectl notifications clear --app org.mozilla.firefox
glimpsectl notifications dnd toggle
glimpsectl notifications dnd on --until 18:00
glimpsectl clipboard list
glimpsectl clipboard get 1287236412341234 > copied.png
glimpsectl config show
glimpsectl doctor
glimpsectl applet list
glimpsectl applet inspect me.example.Clock --installed
glimpsectl applet restart me.example.Clock
glimpsectl applet check --run
glimpsectl applet run --dev --isolated .
```

## Contents

- `lib.rs` — the modules, public so `tests/` can reach them
- `main.rs` — flag resolution, the session-bus connection, subcommand dispatch, exit codes
- `cli.rs` — the canonical grammar, hidden legacy adapters and `needs_session_bus`
- `commands/` — typed provider calls, panel actions and applet development tools
- `render.rs` — `Table`, `Section`, the `styled` colours, and the one `print` everything leaves through
- `errors.rs` — the `Exit` table and the one D-Bus-error-name to exit-code mapping

## Rules

`applet run` asks the panel's `AppletHost` to run an applet as a systemd unit until Ctrl-C, passing
glimpsectl's stderr as the unit's, with options and env from the nearby `config.toml` or the file
passed to `--applet-config` plus `--env`; an installed id requires the flag. `--dev` restarts on
source edits, updates options live, and attaches again when env or the desktop entry changes.
The CLI never relays an applet. The `t` command reads its validated tree from the panel over the
same sender-owned attachment. Numeric `--bar` selects a config panel; global `--panel` selects
the running panel's app id.

`dev panel` starts a foreground test panel with a fresh app id and an explicit scratch config
under `runtime_dir()`, using defaults unless a config was explicitly supplied. `applet run
--isolated` owns the same test-panel lifecycle. Neither reads the user's config implicitly, and
both clean up only their own child and runtime files.

`applet new` selects `$GLIMPSE_SDK_TEMPLATES_DIR/<language>` or
`DATA_DIR/sdk/templates/<language>`. It renders only `.jinja` files with minijinja and copies
other files unchanged.
`applet check` validates its desktop entry and config without a panel or bus;
`--run` also checks its protocol greeting and first commit with temporary state and a bounded
unconfined process. `applet install` writes a marked desktop entry and prints a config suggestion;
`uninstall` removes only an entry carrying that marker. Neither edits the panel config.

**There are no generic commands.** Every command names one operation on one provider, with a typed
request and a typed result, so the help text is the whole surface rather than a starting point for
guessing topic names. **Every proxy is built with `CacheProperties::No`**, since a process that
reads once and exits gains nothing from a property cache and loses correctness to it: `night-light
schedule off` followed by `night-light status` would print the mode it had just replaced. **`--json` is global and
every data command honours it**. Existing JSON shapes stay unchanged; lists do not gain a common
envelope. Pure mutations print nothing. `clipboard get` writes exact bytes normally and a JSON
object containing the id, MIME type and byte array with `--json`. Legacy command spellings warn
on stderr for one minor release; `applets new/run` remain for two.

**`weather` has no `add` or `remove`.** A place is a lease renewed on a thirty-minute clock, so
anything a CLI process watched ages out on its own once it exits; the places to keep live in the
applets that name them, not in the CLI. **`night-light schedule` changes the mode in force and never the
document** — it lasts until `[night-light]` is edited or `glimpse-sunset` restarts, and `status`
says `(override)` so a reader is not sent looking for it in a file that does not say it.

**Provider text is capped and stripped before it reaches the terminal.** Untrusted text reaching a
terminal is worse than untrusted text reaching a label: an escape sequence repaints the screen.
**Everything drawn goes through `render.rs`** — a single `print` handles `BrokenPipe` once rather
than per command, and `Table` takes `[String; N]` rows so a mismatched row is a compile error.
**`notifications dnd on --until HH:MM` uses the next occurrence of that local time**, sent as
epoch microseconds (`0` means indefinite). `--until` is accepted only with `on`.
**`dnd toggle` reverses the current state and clears any expiry.**

**`config` reads the layered stack from disk and opens no bus**, so it works when a provider will
not start; validation problems exit 1. **`doctor` asks `NameHasOwner` before probing**, so inspection
does not activate the provider it reports on. It prints its report before returning 1 for invalid
configuration or a failed required provider. An absent optional UI process is informational.

**`applet list --installed` and `inspect --installed` read desktop entries and placements locally**, query systemd for
running `app-glimpse-*.service` units when a bus is there, and report `runtime: unavailable` when it is not. `list --installed --json`
returns `{runtime, applets}`; each refused row keeps its reason in `refused`, separate from `name`.
`inspect --installed --json` marks a vanished runtime unit unavailable without losing its peers. `inspect --installed` shows `MainPID`, `MemoryCurrent`, `NRestarts` and `ActiveEnterTimestamp` per unit. `restart`
requires a running unit and asks systemd to restart it, including development units.

**Every call to a provider is bounded, because zbus does not bound one for you** — a peer that owns
its name and never replies leaves `Proxy::call` awaiting forever, so each request goes through
`within` on the same `glimpse_dbus::DEADLINE` the panel uses, and a timeout maps to `Exit::Timeout`.
**A `--watch` subscription starts before its first snapshot and exits 3 when its provider loses
its owner.** JSON watch output is NDJSON. `weather` has no watch mode.
**`exit` maps D-Bus error names and holds only the codes something returns today**, with an unlisted
name mapping to 1 and 2 reserved for clap; `message` is the display half of the same mapping —
`zbus::Error::FDO` already exposes its text as `source()`, so a naive print doubles it, and
`message` walks the chain dropping what the previous link already said.

Color resolution is `anstream`'s, so `NO_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE` and `TERM` are
honored without any detection of our own; errors go to stderr, only requested data to stdout.
Column width in `render.rs` counts characters, not display columns, so a wide glyph in third-party
text — CJK, an emoji, as in a notification summary — is measured one short and hangs its row.

`session declare` is the hidden `glimpse-desktop.service` entry point. It changes an inherited
`XDG_CURRENT_DESKTOP=niri` to `glimpse:niri` in the systemd and D-Bus activation environments,
then restarts the desktop portal through its D-Bus API. Other desktop values are left alone.
