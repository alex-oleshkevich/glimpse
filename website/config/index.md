# How configuration works

Every part of glimpse reads one file, `~/.config/glimpse/config.toml`. The panel, the lock screen,
notifications, the wallpaper and the background services each read the tables they care about and
ignore the rest. There is no settings app and no hidden state: what the file says is what runs.

## The file glimpse writes for you

The first time glimpse starts, it writes `config.toml` with every setting listed at its default and
commented out. The file changes nothing until you remove a `#`. Where a `[table]` header is
commented out too, uncomment the header along with the setting, because a key on its own belongs to
whichever table comes before it.

That first copy is never rewritten, so after an upgrade it can miss settings added since.
`/usr/share/glimpse/config.default.toml` always matches the installed version, and this reference
is generated from the same source.

## Tables and keys

The file is [TOML](https://toml.io). Settings are grouped into tables, one per part of the shell:

```toml
[notifications]
edge = "bottom-right"
hide-delay = 6

[night-light]
temperature = 3800
```

Panels are an array of tables, written `[[panels]]`, because you can have more than one. Each
applet's settings live under `[applets.<name>]`. Every key, and every choice from a fixed list, is lowercase with dashes.

glimpse refuses a key it doesn't know rather than ignoring it, so a typo such as `[notifcations]`
is reported instead of silently doing nothing.

## Saving applies the change

Each glimpse process watches the files it reads. Save, and within a moment the change is live: the
clock reformats, the panel moves to another edge, notifications pop up in a different corner. You
never restart anything for a configuration change, except `[regional] language` and
`[lock] pam-service`, which are read when each process starts, and a smaller panel `size`,
`[color-picker]` and `[ruler]`, which apply when the panel next starts. To read the files again right away, run `glimpsectl panel reload`
for the panel or send any glimpse process `SIGHUP`.

## When something is wrong

A file glimpse can't use doesn't take the shell down. On a syntax error or an unknown key, each
process keeps running with the last version that worked, and the panel shows one notification
saying so. `glimpsectl panel list` shows the same reason under `CONFIG` until the file is fixed.

`glimpsectl config validate` checks the files. A syntax error comes with its file, line and column.
A wrong value or an unknown key names the key but not the file, and only the first one is reported,
so fix it and run the command again. Every unknown applet name gets its own line:

```sh
glimpsectl config validate
```

A clean run writes nothing to standard output; add `--log error` to also silence the line it logs
for each file it reads. A path that does not exist is not an error, so check its spelling.

If the file was already broken when glimpse started, it starts with the built-in defaults, except
that idle timers and the night light stay off.

## Completion in your editor

The first line of the generated file points at a JSON schema:

```toml
#:schema /usr/share/glimpse/config.schema.json
```

With a TOML language server such as [Taplo](https://taplo.tamasfe.dev), or an editor extension
built on it such as Even Better TOML for VS Code, you get completion for every table and key, the
description of each setting on hover, and an error on any value glimpse would refuse. Keep that line
if you start a file of your own.

## Layers and drop-ins

`config.toml` is the file you edit, but glimpse reads a short stack of files and merges them, later
ones winning:

1. built-in defaults
2. `/etc/glimpse/config.toml`, for settings a system administrator wants for every user
3. `/etc/glimpse/config.d/*.toml`, in alphabetical order
4. `~/.config/glimpse/config.toml`
5. `~/.config/glimpse/config.d/*.toml`, in alphabetical order

Only files ending in `.toml` count, and no file may be larger than 1 MiB. `~/.config/glimpse` is
`$XDG_CONFIG_HOME/glimpse` when you set that variable.

A drop-in is a small file holding only a few settings. It lets you keep one concern apart, such as a
`config.d/laptop.toml` that only one machine has, or settings a script manages without touching
your main file.

Merging works key by key. Tables merge, so a drop-in that sets `[notifications] edge` leaves every
other notification setting alone. Single values replace. Lists replace too, rather than growing, so
a later file can always shorten a list an earlier one set. `[[panels]]` is a list as well, so the
last file with any `[[panels]]` table defines all of your panels.

The screenshot tool also keeps its own `shot.toml` beside `config.toml`, holding the `[screenshot]`
choices you make in its preferences window. It is read last and supplies only `[screenshot]`.

`glimpsectl config path` lists the files glimpse looks at, in order, including ones that do not
exist yet, and `glimpsectl config show` prints
the merged result.

## Where the rest lives

| Path | Holds |
| --- | --- |
| `~/.config/glimpse/` | `config.toml`, drop-ins, and your own themes under `themes/` |
| `~/.local/state/glimpse/` | things glimpse remembers between sessions, such as your last known location and applet data |
| `~/.local/share/applications/` | the desktop entries of exec applets you install |
| `/usr/share/glimpse/` | the default config, the schema and the built-in themes |

## Testing a different file

Every glimpse program takes `--config <file>`, which replaces the system files, your `config.toml`
and your drop-ins with that one file. A `shot.toml` beside it still supplies `[screenshot]`. It is
useful for trying a configuration without touching your own. `GLIMPSE_CONFIG_PATH` does the same as
`--config`.
