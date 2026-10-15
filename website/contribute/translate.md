# Translate glimpse

All binaries share one gettext domain, `glimpse`, with the template and catalogs in `po/`. You need
gettext, ripgrep and python3; the `just` recipes below call `xgettext`, `msgmerge` and `msgfmt`
through scripts. Testing the result also needs the packages from
[Build glimpse from source](/contribute/build).

## Add a language

A new language is three edits, shown here for German (`de`):

1. Add the language code to `po/LINGUAS`, one per line.
2. Create `po/de.po`. Copy `po/glimpse.pot` as the starting point, fill in the header and translate
   each `msgstr`. Keep every `{placeholder}` exactly as written: the code fills them in by name, and
   a renamed placeholder leaves the sentence with a hole. Where a message has a plural form,
   translate every `msgstr[N]`.
3. Add one asset line for the compiled catalog to each of the three lists in
   `crates/glimpse-package/Cargo.toml` (deb, rpm and the openSUSE rpm variant), next to the existing
   per-language lines. The first form goes in the deb list, the second in both rpm lists:

```toml
["../../target/locale/de/LC_MESSAGES/glimpse.mo", "usr/share/locale/de/LC_MESSAGES/", "644"],
```

```toml
{ source = "target/locale/de/LC_MESSAGES/glimpse.mo", dest = "/usr/share/locale/de/LC_MESSAGES/glimpse.mo", mode = "644" },
```

## Update the strings

`just update-po` rewrites `po/glimpse.pot` from every translation marker in the tree and merges it
into each catalog named in `po/LINGUAS`, so run it first and translate what is new or marked fuzzy:

```sh
just update-po
```

## Build and test

```sh
just build-translations
```

This compiles the catalogs into `target/locale/<lang>/LC_MESSAGES/glimpse.mo`. glimpse loads
catalogs from the installed locale directory, so install your build to see the language on your own
desktop (see [Build glimpse from source](/contribute/build#install)). Then set `[regional] language`
in `~/.config/glimpse/config.toml`. If your session already sets the `LANGUAGE` environment variable,
that wins over the setting. A language change does not apply live: restart the running components to
pick it up.

## Check your work

```sh
just check-strings
```

It fails on a stale template, a catalog `msgfmt` rejects, or a quoted string the extractor cannot
read, and it is part of `just verify`, so a green run is what CI will see.
