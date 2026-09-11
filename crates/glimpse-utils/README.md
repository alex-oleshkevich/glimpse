# glimpse-utils

The argument structs and logging setup every binary repeats, written once.

## Contents

- `args.rs` — `LogArgs`, `ConfigArg`, `SocketArg`, flattened into each binary's clap `Cli`
- `log.rs` — `LogFormat` and `init_app_tracing`
- `i18n.rs` — `init_translations`, the one place the gettext domain is bound
- `text.rs` — `clean`, the gate text from another application passes before it reaches a label
- `build.rs` — bakes `PREFIX` into the default catalog directory

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

## Translations

`init_translations()` calls `setlocale`, `bindtextdomain`, `bind_textdomain_codeset` and
`textdomain` for the single domain `glimpse`. It never fails a binary: every problem is a
`tracing::warn!` and the text stays English, because a missing catalog is not a reason for a panel
not to start.

One domain serves every translated UI binary. They all link `glimpse-widgets`, so per-binary
domains would split one widget's strings across several catalogs and translate the same button
differently depending on which process drew it.

**`init_translations()` runs after `init_app_tracing` and `glimpse_config::load`, and before
`register_resources`.** Its own failures are `tracing::warn!`, so a subscriber has to exist first or
they go nowhere; it takes the language out of `[regional]`, so the document has to be read before
it; and a GTK template resolves `translatable="yes"` as each widget is **built**, so the domain has
to be bound before the first widget instance exists. Nothing in the type system enforces any of it —
it is four lines in each binary's `run`, and that is the whole guard.

Resolution is per instance, not at class-init. Measured: `class_init` runs once, and two instances
of one class built either side of a `LANGUAGE` change come out in different languages. The
practical deadline is therefore later than "before the first type is registered" — but nothing is
gained by cutting it fine, so the domain is still bound before `register_resources`.

**`init_locale()` is `init_translations()` without the catalog.** It is the `setlocale(LC_ALL, "")`
half alone, for `glimpsed`, which has no UI to translate but still has to let the C library see
`LC_MEASUREMENT` — `[regional] units = "locale"` reads `C` and answers metric for everyone
otherwise. Do not "simplify" it away by giving the daemon `init_translations`: the daemon's output
is a journal, not a UI.

**A language named in `[regional]` is applied by setting `LANGUAGE`, and only when the environment
has not already set it.** `LANGUAGE` is what glibc consults per lookup, it takes the short form
(`ru`) whether or not `ru_RU.UTF-8` has been generated, and an explicit one in the environment
wins — the same precedence `GLIMPSE_THEME` has. It moves messages and nothing else: `LC_TIME` and
`LC_MEASUREMENT` keep answering for themselves, which is why a Russian interface in Chicago still
shows a twelve-hour clock.

**The C binding is required, not a preference.** GTK translates a template's `translatable="yes"`
inside GTK, in C: `gtk_widget_init_template` builds its `GtkBuilder` with a NULL translation domain,
so every marked string resolves through `g_dgettext(NULL, …)` against the process default domain.
A pure-Rust catalog cannot answer that call, so it would translate Rust strings and leave every
blueprint in English.

**The catalog directory is a build-time default with a runtime override.** `build.rs` turns
`PREFIX` into `GLIMPSE_LOCALE_DIR_DEFAULT`, so `PREFIX=/usr/local just install` produces binaries
that look in `/usr/local/share/locale` without anyone setting a variable at run time.
`GLIMPSE_LOCALE_DIR` overrides it, which is how `target/locale` is used before installing.

It reads `PREFIX` and not a name of its own **because `scripts/glimpse-paths.sh` already reads
`PREFIX`**, and the two have to agree: a build that bakes one prefix while the installer writes
catalogs under another produces a binary that finds nothing, reports nothing, and shows English.
That is the same defect the path-ownership rule exists to prevent, one level down.

**`setlocale` is `unsafe` in gettext-rs 0.8** — it mutates global C locale state, and the process
is threaded. The gtk-rs book example predates the change and does not compile.
`bindtextdomain` does not validate its argument: it returns `Ok` for a directory that does not
exist, and panics rather than erring on a path containing a NUL. Its `Result` is therefore
unreachable for the literal and environment inputs this crate gives it, and the warning it guards
exists for the case where that stops being true.

**Only `LC_MESSAGES` is ours.** `LC_TIME` is a separate category, so a session exporting
`LC_TIME=pl_PL.utf8` renders Russian text beside Polish month names. That is correct POSIX
behaviour and not a translation bug; it is also the first thing to check when a screenshot looks
half-translated.

## Text

`clean(text, cap)` flattens whitespace, drops control characters and bidi overrides, and caps by
character count rather than by byte, so a multi-byte string cannot be cut mid-codepoint. Calendar
summaries and weather alert headlines both pass through it; the account of why the bidi ranges are
named beside `is_control`, and why a control character becomes a separator rather than vanishing, is
in `glimpse-services/README.md`.

It knows nothing about a service, a topic or a payload — it takes a `&str` and a cap — so it lives
here rather than in the crate that happened to need it first. `glimpse-compositors` carries the
same predicate for window titles and has not been folded in: a second copy of a five-line function
is cheaper than a third crate taking a dependency on this one to avoid it.

## Markup

`sanitize_body(body)` turns a notification body written by another application into Pango markup.
It bounds the input, replaces bidi overrides and control characters with spaces — keeping `\n`,
which a body legitimately uses and which `clean` would flatten — then runs `ammonia` with
`tags(["b", "i", "u"])`, `link_rel(None)` and `strip_comments(true)`.

Three measurements decide the shape, all of them made against real `ammonia 4`, `pango 0.22` and a
`GtkLabel` on GTK 4.22 rather than assumed:

**Pango markup has no `<a>`**; its element set is `b, big, i, s, sub, sup, small, tt, u, span`.
`GtkLabel` accepts `<a href>` anyway, because it parses links itself before Pango sees the rest — so
stripping links is a policy choice, not a technical one. It is still the right choice: a clickable
attacker-controlled link in a surface the user did not deliberately open is a phishing affordance.
The link text survives; the URL does not.

**`<span>` is stripped with its attributes.** `<span foreground="red" size="50pt">` is valid Pango,
and letting it through lets any sender repaint and resize text inside the shell.

**One named entity has to be rewritten by hand.** `ammonia` decodes every named entity to a literal
character — `&mdash;`, `&hellip;`, `&rsquo;`, `&copy;`, `&euro;` — with a single exception:
html5ever's serializer *re-encodes* U+00A0 as `&nbsp;`, which is the one entity name Pango does not
know. A body carrying a non-breaking space therefore fails to parse, and a `GtkLabel` handed markup
that fails to parse renders **empty** rather than showing raw tags. `.replace("&nbsp;", "\u{a0}")`
is the whole fix; no entity table is needed, because `&`, `<`, `>`, `"` and U+00A0 are all that
serializer emits as references.

The previous implementation shipped without that replacement, and without a parse gate. The gate now
lives at the widget, in `glimpse-widgets`, because it needs Pango and this crate takes no GTK
dependency.

## Rules

**Logs go to stderr, and the writer is set explicitly.** `tracing_subscriber::fmt()` defaults to
stdout, which is not a preference to inherit: stdout is data. A log line landing there corrupts
whatever the binary was asked to print — `glimpsectl get --json | jq` broke on exactly that, because
one `WARN` from the client arrived ahead of the payload. `with_writer(std::io::stderr)` is what
stops it, and it has to survive `.json()`, so the two formats are built from one builder rather than
two.

**Color is resolved, not detected.** `init_app_tracing` asks `anstream` what stderr should do, so
`NO_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE` and `TERM` are honored without any detection here. It asks
about stderr specifically, because that is where logs go — asking about stdout would strip color
from logs the moment output is redirected to a file.

**Call `write_global()` before `init_app_tracing`.** A `fmt` subscriber fixes its ANSI setting when
it is built, so a color override applied afterwards reaches nothing.

**Nothing here is domain logic.** No config schema, no topics, no socket. What earns a place is
either a decision every binary has to make identically — what `--log` means, which gettext domain is
bound — or a pure function over primitives that more than one crate needs, like `clean`. Neither is
a licence to park code that has no other home.
