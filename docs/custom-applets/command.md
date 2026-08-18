# Command applet

Command applets are experimental; the package API may change.

Use a command applet when the panel item only needs to run commands. It is the right fit for app launchers, URL launchers, screenshot buttons, lock buttons, power menus, and small personal shortcuts.

If the applet needs to show changing state, render custom popovers, or react to events while a process keeps running, use an [exec applet](./exec.md) instead.

## Flow

1. Create a package file in `~/.config/glimpse/applets`.
2. Add the package `id` to a panel section in `config.toml`.
3. Click the panel item to run the command.

## Button example

```toml
# ~/.config/glimpse/applets/terminal.toml
id = "terminal"
type = "command"

[command]
icon = "utilities-terminal-symbolic"
tooltip = "Open terminal"
command = ["ghostty"]
```

Add the package id to a panel:

```toml
# ~/.config/glimpse/config.toml
[[panels]]
left = ["pager", "terminal", "mpris"]
```

## Menu example

```toml
# ~/.config/glimpse/applets/power-menu.toml
id = "power-menu"
type = "command"

[command]
icon = "system-shutdown-symbolic"
tooltip = "Power"
command = ["loginctl", "lock-session"]

[[command.menu]]
label = "Suspend"
command = ["systemctl", "suspend"]

[[command.menu]]
label = "Restart"
command = ["systemctl", "reboot"]

[[command.menu]]
label = "Power Off"
command = ["systemctl", "poweroff"]
```

The main button runs `command`. Menu items run their own commands.

## Options

| Field | Description |
| --- | --- |
| `icon` | Icon name shown in the panel. |
| `label` | Optional text label shown in the panel. |
| `tooltip` | Tooltip shown on hover. |
| `command` | Command run by the primary click. |
| `on_click` | Same behavior as `command`; useful when you prefer explicit event names. |
| `on_middle_click` | Command run by middle click. |
| `on_scroll_up` | Command run when scrolling up over the applet. |
| `on_scroll_down` | Command run when scrolling down over the applet. |
| `on_scroll_left` | Command run when horizontal scrolling left over the applet. |
| `on_scroll_right` | Command run when horizontal scrolling right over the applet. |
| `menu` | Optional menu entries. Each entry has `label` and `command`. |

## Shell syntax

Commands are argument arrays, not shell strings. This avoids quoting surprises:

```toml
command = ["xdg-open", "https://calendar.google.com"]
```

Use a shell only when you need shell features like pipes, redirects, variables, or command substitution:

```toml
command = ["sh", "-c", "grim ~/Pictures/Screenshots/$(date +%F-%H%M%S).png"]
```

## When to use exec instead

Use an [exec applet](./exec.md) when the panel item needs to:

- update its label, icon, or state over time;
- show a custom popover with live content;
- react to panel events inside a running program;
- keep state between clicks.

## See also

| Page | Use it for |
| --- | --- |
| [Custom Applets](./index.md) | Choosing between command and exec applets. |
| [Exec Applet](./exec.md) | Live applets and custom popovers. |
| [Applet Reference](../applets/index.md) | Built-in applets shipped with the shell. |
