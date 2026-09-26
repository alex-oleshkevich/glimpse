# Translations

One gettext domain, `glimpse`, for all binaries. `glimpse-utils` owns it: `init_translations()`
binds it, and the panel, notification popup, lock screen, wallpaper and `glimpse-ruler` call that
once in `run`. The daemon, `glimpsectl` and `glimpse-sunset` do not — their output is a journal and
a terminal. `glimpse-picker` does not either — its only on-screen text is a raw color value, never a
phrase.

```bash
just extract-strings     # rewrite po/glimpse.pot from the tree
just update-po           # merge the .pot into every catalog named by po/LINGUAS
just build-translations  # compile po/*.po into target/locale/<lang>/LC_MESSAGES/glimpse.mo
just check-strings       # part of `just verify`
GLIMPSE_LOCALE_DIR=$PWD/target/locale LANGUAGE=ru just preview <blueprint.blp>
```

- **`glimpse-sunset` and `glimpse-weather` call `init_locale()` instead, and must keep doing so.**
  That is the `setlocale(LC_ALL, "")` half without the catalog. Without it the process locale is
  `C`, `nl_langinfo(LC_MEASUREMENT)` answers metric for everyone, and `[regional] units = "locale"`
  is silently wrong rather than absent. It is not `init_translations` and must not become it.
- **`[regional] language` sets `LANGUAGE` and moves messages only.** `LC_TIME` and `LC_MEASUREMENT`
  keep answering for themselves, so a Russian interface in Chicago still gets a twelve-hour clock
  and Fahrenheit. Russian labels beside English weekday names look like a bug and are not. An
  explicit `LANGUAGE` in the environment wins over the document, matching `GLIMPSE_THEME`.
- **The config load runs before `init_translations`** in all three UI binaries, because the language
  comes out of the document: `init_app_tracing` → `glimpse_config::load` → `init_translations` →
  `register_resources`. Reordering breaks the warnings, the language, or both.
- **A language change cannot be applied to a running process.** A GTK template resolves
  `translatable="yes"` per **instance**, so two widgets built either side of a `LANGUAGE` change
  come out in different languages. `ConfigChanged` logs at `info` and waits for a restart.
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
  without staging it. `grep -c '^msgid ' po/glimpse.pot` is still the number that tells the truth
  when the count looks wrong.
- **`var/` is not extracted.** Its examples carry `_()` markers so they read like the real thing,
  but they never ship.
- **A new language is three edits:** `po/LINGUAS`, a new `po/<lang>.po`, and one asset line in
  *each* of the two lists in `crates/glimpse-package/Cargo.toml`.
