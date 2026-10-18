# Package format

An applet package is a zip file named `<id>.glimpse.zip`. `glimpsectl applet package` builds it and
`glimpsectl applet install` installs it; [Package and share an applet](/applets/exec/share) shows
both.

## Building

`glimpsectl applet package [DIR]` runs `just build` in the project directory. The build must put
every file the applet needs to run into `dist/`, including `<id>.desktop`. glimpsectl then:

- refuses a `dist/` holding a different id, or a desktop entry without
  `Implements=me.aresa.Glimpse.Applet1` and `Exec`;
- refuses an `Exec` of `./<file>` when `dist/<file>` does not exist;
- refuses a `dist/metadata.toml`, which it writes itself;
- writes `<id>.glimpse.zip` in the current directory, or at `--out PATH`.

A bare program name in `Exec` (`node`, `uv`) is looked up on `PATH` of the machine that runs it.

## Contents

```text
me.aresa.GlimpseTodo.glimpse.zip
├── metadata.toml
├── me.aresa.GlimpseTodo.desktop
└── applet.mjs
```

`metadata.toml` names the id and every other file with its SHA-256:

```toml
id = "me.aresa.GlimpseTodo"

[[files]]
path = "me.aresa.GlimpseTodo.desktop"
sha256 = "<64 hex digits>"

[[files]]
path = "applet.mjs"
sha256 = "<64 hex digits>"
```

| Limit | Value |
| --- | --- |
| Unpacked size of all files | 512 MiB |
| `metadata.toml` | 1 MiB |
| Entries, including `metadata.toml` (checked on install) | 10,000 |

## Installing

`glimpsectl applet install` takes a `.glimpse.zip` or a project directory, which it builds first.
It refuses a package whose files do not match `metadata.toml` exactly, by name and hash, or whose
paths leave the package. Then it:

1. copies the files to `~/.local/share/glimpse/applets/<id>/`;
2. writes `~/.local/share/applications/<id>.desktop`, with a relative `Exec` (`./todo`) made
   absolute, `Path` set to that directory, and `X-Glimpse-Installed-By=glimpsectl`;
3. prints a config line to place the applet.

Installing again replaces an earlier glimpsectl install of the same id. It refuses when the
directory holds files the earlier install does not list, or when the desktop entry was not written
by glimpsectl.

`glimpsectl applet uninstall <id>` removes the files `metadata.toml` lists and the desktop entry,
and leaves anything else in place.
