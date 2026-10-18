# Desktop entry keys

Exec applets are desktop applications whose `[Desktop Entry]` group declares `Implements=me.aresa.Glimpse.Applet1`. The host loads them through the desktop application catalog and expands `Exec` into argv without starting a shell.

## Keys used by the host

| Key | Requirement and behavior |
| --- | --- |
| `Type` | Must be `Application` for the entry to load. |
| `Implements` | Its string list must include `me.aresa.Glimpse.Applet1`. |
| `Name` | Read through the desktop catalog as the applet name; also used for `%c` expansion. |
| `Icon` | Optional. An empty value is absent. Used for `%i` expansion and as the icon of the applet's notifications when `notify` gives none (the SDK receives it in the host's `hello`); the SDK tree supplies the icons on the bar. |
| `Exec` | Required. Parsed into a program and arguments, then expanded using the rules below. An empty resulting argv is refused. |
| `Path` | Optional working directory. If absent or empty, the host uses the canonical desktop file's parent directory. |
| `Hidden` | An entry with `Hidden=true` is refused. |
| `DBusActivatable` | An entry with `DBusActivatable=true` is refused. |
| `Terminal` | An entry with `Terminal=true` is refused. |
| `TryExec` | Checked by the desktop loader. A missing executable can prevent loading. The loader also checks the `Exec` program. |
| `NoDisplay` | `true` keeps the entry out of application menus; it does not prevent applet discovery. |

The generated template also writes `Version=1.5` and `Comment`. The applet catalog does not read either field directly.

## Exec expansion

Quoted arguments are parsed before field expansion. There is no implicit shell and no shell variable expansion.

| Field code | Expansion |
| --- | --- |
| `%%` | A literal percent sign. |
| `%c` | The desktop catalog's applet name, including within an argument. |
| `%k` | The desktop file path, including within an argument. |
| `%i` | As a standalone argument, becomes `--icon` and the icon value. Removed when no icon is set. |
| `%f`, `%F`, `%u`, `%U`, `%d`, `%D`, `%n`, `%N`, `%v`, `%m` | Removed when used as standalone arguments. |
| `%f`, `%u`, `%d`, `%n`, `%v`, `%m` within an argument | Removed from that argument. |
| Other codes, a trailing `%`, or embedded `%F`, `%U`, `%D`, `%N` or `%i` | Refused. |

The program is resolved against the applet's merged environment `PATH`. A desktop entry can select its working directory with `Path`; otherwise a symlink resolves back to the source entry's directory.

## Identity and discovery

The desktop filename supplies the applet id. `me.aresa.GlimpseTodo.desktop` is placed in a panel zone as `me.aresa.GlimpseTodo`. Installed discovery asks the desktop catalog for implementations of `me.aresa.Glimpse.Applet1`.

User entries can be installed under `${XDG_DATA_HOME:-$HOME/.local/share}/applications`. A development directory must contain exactly one `.desktop` file. `just dev` there loads the entry and refuses one that is invalid; `just package` builds it into a package. See [Package and share an applet](/applets/exec/share) for installation and panel configuration.
