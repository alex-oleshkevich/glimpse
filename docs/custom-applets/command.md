# Command Applet

The command applet is the easiest way to add a launcher or small menu to your panel.

## Button Example

```toml
# ~/.config/glimpse/applets/terminal.toml
id = "terminal"
type = "command"

[command]
icon = "utilities-terminal-symbolic"
label = "Terminal"
tooltip = "Open terminal"
command = ["ghostty"]
```

Add it to a panel:

```toml
right = ["terminal", "network", "battery"]
```

## Menu Example

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
label = "Shutdown"
command = ["systemctl", "poweroff"]
```

The main button runs `command`. The menu items run their own commands.

## Options

| Option | What it does |
|---|---|
| `icon` | Symbolic icon name. |
| `label` | Optional text beside the icon. |
| `tooltip` | Text shown on hover. |
| `command` | Command to run when clicked. |
| `menu` | Optional right-click or popover menu items. |

## Shell Examples

Open a web app:

```toml
command = ["xdg-open", "https://calendar.google.com"]
```

Use shell features:

```toml
command = ["sh", "-c", "grim ~/Pictures/Screenshots/$(date +%F-%H%M%S).png"]
```

When in doubt, write the command exactly as you would run it in a terminal, then wrap it with `["sh", "-c", "..."]` if it needs shell syntax.
