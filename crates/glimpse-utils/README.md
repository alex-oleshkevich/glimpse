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
environment variable that is its default: `SocketArg` (`-s`/`--socket`, `GLIMPSED_SOCKET_PATH`),
`ConfigArg` (`-c`/`--config`, `GLIMPSE_CONFIG_PATH`), `LogArgs` (`--log`/`--log-format`, `RUST_LOG`).
The variable is declared on the flag rather than read separately, so it shows in `--help` and cannot
acquire a second spelling elsewhere.

`init_app_tracing(level, format)` builds the subscriber; an invalid filter warns and falls back to
`info` rather than aborting a binary over a stale entry in somebody's profile.

## Translations

- `init_translations()` calls `setlocale`, `bindtextdomain`, `bind_textdomain_codeset` and
  `textdomain` for the single domain `glimpse`, never failing a binary — every problem is a
  `tracing::warn!` and the text stays English. **One domain serves every UI binary**, since they all
  link `glimpse-widgets`; per-binary domains would translate the same button differently per process.
- **`init_translations()` runs after `init_app_tracing` and `glimpse_config::load`, and before
  `register_resources`.** A subscriber must exist first for its own warnings, the document first for
  the language in `[regional]`, and a GTK template resolves `translatable="yes"` per **instance**, not
  at class-init — two instances built either side of a `LANGUAGE` change come out in different
  languages, so the domain must be bound before the first widget instance exists.
- **`init_locale()` is `init_translations()` without the catalog** — `setlocale(LC_ALL, "")` alone,
  for `glimpse-sunset` and `glimpse-weather`, which have no UI but still need the C library to see
  `LC_MEASUREMENT`, or `[regional] units = "locale"` silently answers metric for everyone. Never give
  a provider `init_translations`; its output is a journal, not a UI.
- **`[regional] language` sets `LANGUAGE`, only when the environment has not already set it**, and
  moves messages only — `LC_TIME` and `LC_MEASUREMENT` keep answering for themselves, so a Russian
  interface in Chicago still shows a twelve-hour clock.
- **The C binding is required, not a preference** — `gtk_widget_init_template` resolves
  `translatable="yes"` through `g_dgettext(NULL, …)` against the process domain, which a pure-Rust
  catalog cannot answer, leaving every blueprint in English.
- **The catalog directory is a build-time default with a runtime override.** `build.rs` turns
  `PREFIX` into `GLIMPSE_LOCALE_DIR_DEFAULT`, `GLIMPSE_LOCALE_DIR` overrides it at runtime, and it
  reads `PREFIX` because `scripts/glimpse-paths.sh` already does — the two must agree, or a build
  baking one prefix while the installer writes catalogs under another shows English.

## Text, size and markup

`clean(text, cap)` flattens whitespace, drops control characters and bidi overrides, and caps by
character count so a multi-byte string cannot be cut mid-codepoint; `glimpse-compositors` keeps its
own copy for window titles rather than take a dependency on this crate to avoid one. `size::bytes(u64)
-> String` formats a byte count SI-style, base 1000, dropping a trailing `.0` — `"1.5 MB"`, never
`"1.50 MB"`.

`sanitize_body(body)` turns a notification body into Pango markup: bounds the input, replaces bidi
overrides and control characters with spaces (keeping `\n`, which `clean` would flatten), then runs
`ammonia` with `tags(["b", "i", "u"])`, `link_rel(None)` and `strip_comments(true)`. The parse gate
itself lives at the widget, in `glimpse-widgets`, since it needs Pango and this crate takes no GTK
dependency.

- **No `<a>` tag** — Pango has no anchor element, and a clickable attacker-controlled link in a
  surface the user did not deliberately open is a phishing affordance; link text survives, the URL
  does not.
- **`<span>` is stripped with its attributes** — `<span foreground="red" size="50pt">` is valid
  Pango, and letting it through lets any sender repaint and resize text inside the shell.
- **`&nbsp;` is rewritten by hand** to `\u{a0}` after `ammonia` runs: html5ever re-encodes U+00A0 as
  `&nbsp;`, which Pango does not know, and a `GtkLabel` given markup that fails to parse renders
  **empty** rather than showing raw tags.

## Rules

- **Logs go to stderr, writer set explicitly** — `tracing_subscriber::fmt()` defaults to stdout,
  which is data; `glimpsectl get --json | jq` breaks the moment a `WARN` lands ahead of the payload.
- **Color is resolved, not detected, and set before `init_app_tracing`.** It asks `anstream` about
  stderr specifically, honoring `NO_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE` and `TERM` with no detection
  of its own; call `write_global()` first, since a `fmt` subscriber fixes its ANSI setting when built
  and a color override applied afterwards reaches nothing.
