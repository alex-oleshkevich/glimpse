# Command Applet — Full Reference

The `command` applet adds a clickable panel button (with optional context menu)
that runs a shell command when activated. It is the simplest custom applet:
purely declarative TOML, no script, no protocol.

This page is self-contained. You do not need to read any other page to write a
working command applet.

## Defining A Command Applet

Custom applets live under `[applets.<name>]` in `~/.config/glimpse/config.toml`.
A command applet is declared by setting `extends = "command"`:

```toml
[applets.terminal]
extends = "command"
icon = "utilities-terminal-symbolic"
label = "Terminal"
tooltip = "Open a terminal"
command = ["ghostty"]
```

The applet `<name>` (here `terminal`) is the identifier you use elsewhere — for
example to place the applet in a panel:

```toml
[[panels]]
right = ["terminal", "network", "battery"]
```

## All Options

| Option | Type | Default | Meaning |
|---|---|---|---|
| `extends` | string | required | Must be `"command"`. |
| `icon` | string | unset | Symbolic icon name (preferred) or absolute path to an image. Symbolic icons should end in `-symbolic`. |
| `label` | string | unset | Optional text shown beside the icon in the panel. |
| `tooltip` | string | unset | Hover text. |
| `command` | array of strings | required | Argv to run when the button is clicked. The first element is the program; remaining elements are arguments. Arguments are passed literally — no shell expansion. |
| `menu` | array of menu items | unset | Optional right-click / popover menu. See below. |

### Menu Items

Each menu entry is a `[[applets.<name>.menu]]` table:

| Field | Type | Default | Meaning |
|---|---|---|---|
| `label` | string | required | Text shown in the menu. |
| `command` | array of strings | required | Argv to run when the item is selected. |
| `icon` | string | unset | Optional symbolic icon for the menu item. |

The main button always runs the top-level `command`. Menu items each run their
own `command`. Right-clicking the button opens the menu; on touch input it
opens via long-press.

## Shell Syntax And `argv`

`command` is **always** an explicit argv array. There is no implicit shell.
Shell metacharacters (`|`, `>`, `&&`, `~`, `$VAR`, globs, etc.) are not
interpreted. To use shell features, run a shell yourself:

```toml
command = ["sh", "-c", "grim ~/Pictures/Screenshots/$(date +%F-%H%M%S).png"]
```

Rules of thumb:
- A single binary with literal arguments: argv array directly.
- Anything with pipes, redirects, variables, globs, command substitution, `~`,
  or `&&`/`||`: wrap with `["sh", "-c", "..."]`.
- For one-off complex scripts, save them under `~/.config/glimpse/scripts/foo`
  and call `command = ["sh", "-c", "~/.config/glimpse/scripts/foo"]`.

## Working Examples

### Plain Launcher

```toml
[applets.terminal]
extends = "command"
icon = "utilities-terminal-symbolic"
tooltip = "Open terminal"
command = ["ghostty"]
```

### Launcher With Visible Label

```toml
[applets.editor]
extends = "command"
icon = "accessories-text-editor-symbolic"
label = "Edit"
tooltip = "Open editor"
command = ["code", "-n"]
```

### Power Menu (Button + Right-Click Menu)

```toml
[applets.power-menu]
extends = "command"
icon = "system-shutdown-symbolic"
tooltip = "Power"
command = ["loginctl", "lock-session"]

[[applets.power-menu.menu]]
label = "Suspend"
command = ["systemctl", "suspend"]

[[applets.power-menu.menu]]
label = "Restart"
command = ["systemctl", "reboot"]

[[applets.power-menu.menu]]
label = "Shutdown"
command = ["systemctl", "poweroff"]
```

Left-click locks the session; right-click opens Suspend / Restart / Shutdown.

### Web Shortcut

```toml
[applets.calendar]
extends = "command"
icon = "x-office-calendar-symbolic"
label = "Cal"
command = ["xdg-open", "https://calendar.google.com"]
```

### Screenshot With Shell Substitution

```toml
[applets.screenshot]
extends = "command"
icon = "camera-photo-symbolic"
tooltip = "Screenshot region"
command = ["sh", "-c", "slurp | grim -g - ~/Pictures/$(date +%F-%H%M%S).png"]
```

### Append A Timestamped Note

```toml
[applets.note-time]
extends = "command"
icon = "document-edit-symbolic"
command = ["sh", "-c", "date >> ~/.cache/glimpse-notes.log"]
```

### Custom Script Path With Menu

```toml
[applets.dotfiles]
extends = "command"
icon = "preferences-system-symbolic"
tooltip = "Dotfiles"
command = ["sh", "-c", "~/.local/bin/dotfiles-status | xclip -selection clipboard"]

[[applets.dotfiles.menu]]
icon = "view-refresh-symbolic"
label = "Sync"
command = ["sh", "-c", "~/.local/bin/dotfiles-sync"]

[[applets.dotfiles.menu]]
icon = "document-open-symbolic"
label = "Open"
command = ["xdg-open", "/home/me/.dotfiles"]
```

### Quick-Switch With Multiple Menu Targets

```toml
[applets.workspaces]
extends = "command"
icon = "preferences-desktop-workspaces-symbolic"
command = ["niri", "msg", "action", "focus-workspace", "1"]

[[applets.workspaces.menu]]
label = "Workspace 1"
command = ["niri", "msg", "action", "focus-workspace", "1"]

[[applets.workspaces.menu]]
label = "Workspace 2"
command = ["niri", "msg", "action", "focus-workspace", "2"]

[[applets.workspaces.menu]]
label = "Workspace 3"
command = ["niri", "msg", "action", "focus-workspace", "3"]
```

## What Command Applets Cannot Do

- Show changing state (counts, status, live values). Use [`exec`](./exec.md).
- React to events from the command's output. Use `exec`.
- Display popover content beyond a flat right-click menu. Use `exec`.
- Run with arbitrary environment overrides or env clearing. Use `exec`.

If any of those apply, switch to `exec`.

## Behavior Notes

- Commands run with the user environment Glimpse inherited at startup.
- Glimpse does not capture stdout/stderr. Redirect inside `sh -c` if you need
  output captured.
- Exit codes are ignored. Non-zero exits are not surfaced in the UI.
- Multiple instances of the same applet may run concurrently if the user
  clicks repeatedly; commands are not serialized.
- A failed launch (binary not found) is logged but the panel does not visibly
  indicate failure.
- Symbolic icons should resolve through the system icon theme. Falling back to
  an absolute `path` form is supported when you pass a path string starting
  with `/`.
