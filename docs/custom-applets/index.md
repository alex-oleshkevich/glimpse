# Custom Applets

Custom applets let you add your own buttons, menus, and live status items to the panel.

Use them when the built-in applets are not enough, or when you want your desktop to show exactly the things you care about.

**New here?** Walk through [Getting Started](./getting-started.md) to build your first applet in Rust, Python, TypeScript, or Go. Use [Applet Tooling](./tooling.md) when you want the `glimpse-shell applets` command reference.

## Two Types

| Type | Use it for |
|---|---|
| [`command`](./command.md) | A button or menu that runs commands. |
| [`exec`](./exec.md) | A script that continuously controls what the applet shows. |

## Applet Projects

The recommended workflow is to keep each custom applet in its own project directory:

```sh
glimpse-shell applets new counter --lang python
cd counter
glimpse-shell applets dev
```

Each project has an `applet.toml` file. `glimpse-shell applets dev` creates a temporary `.dev.toml` entry while you work, and `glimpse-shell applets link` installs the applet for normal use by symlinking `applet.toml` into `~/.config/glimpse/applets`.

Add `__dev__` to a panel section to show applets started with `glimpse-shell applets dev`:

```toml
[[panels]]
right = ["network", "__dev__", "battery"]
```

## Quick Launcher

```toml
# ~/.config/glimpse/applets/terminal.toml
id = "terminal"
type = "command"

[command]
icon = "utilities-terminal-symbolic"
tooltip = "Open terminal"
command = ["ghostty"]
```

```toml
# ~/.config/glimpse/config.toml
[[panels]]
right = ["terminal", "network", "battery"]
```

## Shell Syntax

Commands are always explicit. If you want shell features such as pipes, redirects, `~`, variables, or `&&`, run a shell yourself:

```toml
[applets.note-time]
extends = "command"
icon = "document-edit-symbolic"
command = ["sh", "-c", "date >> ~/.cache/glimpse-notes.log"]
```

This keeps simple commands simple and makes complex commands clear.

## Which One Should I Use?

| You want | Choose |
|---|---|
| Open an app | `command` |
| Run one command on click | `command` |
| Show a small menu | `command` |
| Show changing text or icons | `exec` |
| React to clicks inside a custom status widget | `exec` |
| Build a mini applet with your own script | `exec` |

## Reference

| Page | Covers |
|---|---|
| [Getting Started](./getting-started.md) | Build and run a generated SDK applet. |
| [Applet Tooling](./tooling.md) | `glimpse-shell applets new`, `dev`, `link`, `ls`, `doctor`, and related subcommands. |
| [Exec Applet](./exec.md) | Applet config and options. |
| [Exec SDK](../applets/exec-sdk.md) | SDK installation and language examples. |
| [Line Protocol](./exec-protocol.md) | Raw protocol commands, message shapes, and events. |
| [Components](./exec-components.md) | Popover component fields and component types. |
