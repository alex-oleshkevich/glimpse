# Package and share an applet

Install your applet so it runs without a terminal, and build a file you can send to someone else.
The [package format](/applets/exec/reference/package) lists what goes in and what the installer
checks.

## Install it for yourself

From the project directory:

```sh
just install
```

This builds the applet, packages it, and installs the package: the files go to
`~/.local/share/glimpse/applets/<id>/` and the desktop entry to `~/.local/share/applications/`.
Then add the id to a panel zone, as [Configure an applet in the panel](/applets/exec/configure)
shows.

While you work on the applet, use `just dev`, which rebuilds and restarts it on every edit. Install
again only to update the installed copy, then restart it so the panel runs the new version:

```sh
glimpsectl applet restart me.aresa.GlimpseTodo
```

Check that the panel sees it:

```sh
glimpsectl applet list --installed
```

## Share it

```sh
just package
```

This writes `<id>.glimpse.zip` in the project directory. Send that file. The recipient installs it
with:

```sh
glimpsectl applet install me.aresa.GlimpseTodo.glimpse.zip
```

and places the id in a panel zone. A TypeScript applet needs `node` on the recipient's machine, and
a Python applet needs `uv`; Go and Rust applets ship a binary.

::: warning
Exec applets run with full access to your session, so install only applets whose code you trust.
:::

## Remove it

```sh
glimpsectl applet uninstall me.aresa.GlimpseTodo
```

This removes the files the install wrote and the desktop entry. Take the id out of your panel zones
as well, or the panel keeps looking for it.
