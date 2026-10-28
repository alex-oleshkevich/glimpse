# How configuration works

Glimpse reads two files in `~/.config/glimpse/`. `config.toml` holds every setting except the panel
layout: the lock screen, notifications, the wallpaper and the background services each read the
tables they care about and ignore the rest. `panel.toml` holds the layout of the panels and their
applets, `[[panels]]` and `[applets]`, and only the panel and `glimpsectl` read it. There is no
settings app and no hidden state: what the files say is what runs.

## The files glimpse writes for you

The first time the panel, the notifications, the lock screen or the wallpaper starts, it creates `config.toml` and `panel.toml` if they are
missing. Each holds a `#:schema` line and a link to this documentation, nothing else, so every
setting stays at its default and the default panel shows.

`/usr/share/glimpse/config.commented.toml` and `/usr/share/glimpse/panel.commented.toml` list every
setting at its default, commented out. Copy what you want to change into your file and remove the
`#`. Where a `[table]` header is commented out too, uncomment the header along with the setting,
because a key on its own belongs to whichever table comes before it.

Those two files always match the installed version, and so do `config.default.toml` and
`panel.default.toml` beside them. This reference is generated from the same source.

## Tables and keys

The file is [TOML](https://toml.io). Settings are grouped into tables, one per part of the shell:

```toml
[notifications]
edge = "bottom-right"
hide-delay = 6

[night-light]
temperature = 3800
```

Panels are an array of tables, written `[[panels]]` in `panel.toml`, because you can have more than
one. Each applet's settings live under `[applets.<name>]`, also in `panel.toml`. Every key, and every choice from a fixed list, is lowercase with dashes.

glimpse refuses a key it doesn't know rather than ignoring it, so a typo such as `[notifcations]`
is reported instead of silently doing nothing. It also refuses a table in the wrong file, such as
`[[panels]]` in `config.toml` or `[lock]` in `panel.toml`, and the error names the file the table
belongs in.

## Saving applies the change

Each glimpse process watches the files it reads, except the ones in `/etc/glimpse`. Save, and within a moment the change is live: the
clock reformats, the panel moves to another edge, notifications pop up in a different corner. You
never restart anything for a configuration change, except `[lock] pam-service`, which is read when
each process starts, and a smaller panel `size`,
which applies when the panel next starts. To read the files again right away, run `glimpsectl panel reload`
for the panel or send any glimpse process `SIGHUP`, which also reads the files in `/etc/glimpse`.

## When something is wrong

A file glimpse can't use doesn't take the shell down. On a syntax error or an unknown key, each
process keeps running with the last version that worked, and the panel shows one notification
saying so. `glimpsectl panel list` shows the same reason under `CONFIG` until the file is fixed. A
broken `panel.toml` only affects the panel: the other programs never read it.

`glimpsectl config validate` checks the files. A syntax error comes with its file, line and column.
A wrong value or an unknown key names the key but not the file, and only the first one is reported,
so fix it and run the command again. Every unknown applet name gets its own line:

```sh
glimpsectl config validate
```

A clean run writes nothing to standard output; add `--log error` to also silence the line it logs
for each file it reads. A path that does not exist is an error that names it.

If the file was already broken when glimpse started, it starts with the built-in defaults, except
that idle timers and the night light stay off.

## Completion in your editor

The first line of each generated file points at a JSON schema, `config.schema.json` for
`config.toml` and `panel.schema.json` for `panel.toml`:

```toml
#:schema /usr/share/glimpse/config.schema.json
```

With a TOML language server such as [Taplo](https://taplo.tamasfe.dev), or an editor extension
built on it such as Even Better TOML for VS Code, you get completion for every table and key, the
description of each setting on hover, and an error on any value glimpse would refuse. Keep that line
if you start a file of your own.

## Where glimpse looks

Glimpse reads a stack of directories, highest first:

1. the directory you pass with `--config-dir`, or set in `GLIMPSE_CONFIG_DIR`
2. `~/.config/glimpse`, or `$XDG_CONFIG_HOME/glimpse` when you set that variable
3. `/etc/glimpse`, for settings a system administrator wants for every user

`config.toml` from every directory is merged on top of the built-in defaults, the highest directory
winning. For `panel.toml` the same applies, and each directory's `panel.d/*.toml` files follow its
`panel.toml` in alphabetical order. A drop-in is a small file holding only a few panel tables, such
as a `panel.d/laptop.toml` that only one machine has. Only files ending in `.toml` count, and no
file may be larger than 1 MiB.

Merging works key by key. Tables merge, so a file that sets `[notifications] edge` leaves every
other notification setting alone. Single values replace. Lists replace too, rather than growing, so
a later file can always shorten a list an earlier one set. `[[panels]]` is a list as well, so the
last file with any `[[panels]]` table defines all of your panels.

The screenshot tool saves the choices you make in its preferences window to `[screenshot]` in
`config.toml`: the one in your `--config-dir` when you passed one, otherwise the one in
`~/.config/glimpse`.

`glimpsectl config path` lists the files glimpse looks at, in order, including the `config.toml` and
`panel.toml` that do not exist yet and the `panel.d` files that do, and `glimpsectl config show` prints the merged result.

## Where the rest lives

| Path | Holds |
| --- | --- |
| `~/.config/glimpse/` | `config.toml`, `panel.toml`, `panel.d/`, and your own themes under `themes/` |
| `~/.local/state/glimpse/` | things glimpse remembers between sessions, such as your last known location and applet data |
| `~/.local/share/applications/` | the desktop entries of exec applets you install |
| `/usr/share/glimpse/` | the default and commented references, the schemas and the built-in themes |

## Testing a different configuration

Every glimpse program that reads configuration takes `--config-dir <directory>`, which stacks that
directory above `~/.config/glimpse`. Its `config.toml` and `panel.toml` win, and anything it leaves
out falls through to your own files. Anything glimpse saves for you, such as a screenshot
preference or a wallpaper an app sets through the desktop portal, goes into that directory. `GLIMPSE_CONFIG_DIR`
does the same as `--config-dir`. To keep your own files out of a trial completely, set
`XDG_CONFIG_HOME` to an empty directory too.
