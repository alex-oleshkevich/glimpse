# Translations

One gettext domain, `glimpse`, for all binaries. `glimpse-utils` owns it: `init_translations()`
binds it for programs with a UI or desktop notifications, including the panel, notification popup,
lock screen, wallpaper, `glimpse-lens` and `glimpse-calendar`. The daemon, `glimpsectl` and
`glimpse-sunset` do not load the catalog — their output is a journal and a terminal.

```bash
just extract-strings     # rewrite po/glimpse.pot from the tree
just update-po           # merge the .pot into every catalog named by po/LINGUAS
just build-translations  # compile po/*.po into target/locale/<lang>/LC_MESSAGES/glimpse.mo
just check-strings       # part of `just verify`
GLIMPSE_LOCALE_DIR=$PWD/target/locale LANGUAGE=ru just preview <blueprint.blp>
```

- **`glimpse-sunset` and `glimpse-weather` call `init_locale()` instead, and must keep doing so.**
  That is the `setlocale(LC_ALL, "")` half without the catalog. Without it the process locale is
  `C`, `nl_langinfo(LC_MEASUREMENT)` answers metric for everyone and weather units are silently
  wrong rather than absent. It is not `init_translations` and must not become it.
- **Language, time and units come from the session environment only.** Messages follow
  `LANGUAGE`/`LC_MESSAGES`/`LANG`; day and month names and the 12/24-hour choice follow `LC_TIME`;
  weather units follow `LC_MEASUREMENT`. They are independent, so a Russian interface in Chicago
  still gets a twelve-hour clock and Fahrenheit. Russian labels beside English weekday names look
  like a bug and are not.
- **`init_translations()` takes no argument** and runs after `init_app_tracing` and before
  `register_resources` in the UI binaries.
- **A language change cannot be applied to a running process.** A GTK template resolves
  `translatable="yes"` per **instance**, so two widgets built either side of a `LANGUAGE` change
  come out in different languages. Restart glimpse after changing the session environment.
- **Mark a string where it is written.** In Blueprint, `_("Text")`; in Rust, `gettext("Text")`, or
  `ngettext(singular, plural, count)` when a number decides the wording. Interpolate with named
  `{placeholders}` and `.replace(…)`, never `format!` into the msgid — a positional `%s` cannot be
  reordered by a translator.
- **Double quotes, always.** Blueprint compiles `_('Text')` happily; xgettext's C scanner skips it.
  `scripts/i18n-coverage.py` fails the build on it.
- **No translatable text in a raw or multi-line Rust string.** `r#"…"#` extracts by accident and the
  C scanner loses its place inside both, costing the rest of that file.
- **A new file's strings are extracted as soon as the file exists.** `scripts/i18n-extract.sh`
  builds its list with `rg --files`, not `git ls-files`, so an untracked `.rs` or `.blp` is picked up
  without staging it.
- **`var/` is not extracted.** Its examples carry `_()` markers so they read like the real thing,
  but they never ship.
- **A new language is three edits:** `po/LINGUAS`, a new `po/<lang>.po`, and one asset line in
  *each* of the three lists in `crates/glimpse-package/Cargo.toml` (deb, rpm and the openSUSE variant).
