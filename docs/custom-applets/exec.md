# Exec Applet

The exec applet runs a child process and lets that process control panel status and popover content.

Use it when you want a custom local widget without writing a built-in Rust applet. Raw exec applets use the [line protocol](./exec-protocol.md). If you prefer typed helpers instead of raw protocol lines, use the [Exec SDK](../applets/exec-sdk.md). For project creation and live reload, use [Applet Tooling](./tooling.md).

## Basic Config

```toml
# ~/.config/glimpse/applets/sysinfo.toml
id = "sysinfo"
type = "exec"

[exec]
command = ["sh", "-c", "~/.config/glimpse/scripts/sysinfo"]
restart_delay_ms = 1000
env_forward = false

[exec.env]
PATH = "/usr/bin:/bin"

[exec.options]
interval = 5
unit = "celsius"
```

Add the custom applet name to a panel section:

```toml
right = ["sysinfo", "network", "battery"]
```

| Option | Default | Meaning |
|---|---|---|
| `type` | required | Use `"exec"` in an applet package file. |
| `command` | `[]` | Program to run. Required. Use `["sh", "-c", "..."]` when you need shell syntax. |
| `restart_delay_ms` | `1000` | Delay before the program restarts after exit. Minimum 50. |
| `options` | `{}` | Custom data sent to the child process in the `init` message. |
| `env_forward` | `false` | Set `true` to inherit the parent process environment. |
| `env` | `{}` | Extra environment variables for the child process. |

## Applet Directories

Exec applets are declared as applet package files, either linked into
`~/.config/glimpse/applets` or kept in project directories with an `applet.toml`
file:

```toml
id = "sysinfo"
type = "exec"

[exec]
command = ["uv", "run", "main.py"]
restart_delay_ms = 1000
```

Project directories are the preferred shape for SDK applets. They keep source code, package metadata, and `applet.toml` together. `glimpse-shell applets link` installs the project by symlinking `applet.toml` to `~/.config/glimpse/applets/<id>.toml`.

Development mode creates a temporary discovered applet instead:

```sh
glimpse-shell applets dev /path/to/sysinfo
```

That command writes `~/.config/glimpse/applets/sysinfo.dev.toml` while it runs. The default panel already includes `__dev__`; keep or add it in custom panel layouts to show active dev applets.

If an applet id exists in both the normal applet directory and the dev applet
set, the normal linked applet wins.

## Options

`[exec.options]` is arbitrary data for your applet instance. Glimpse does not interpret it.

When the child process starts, Glimpse sends the options in the first `init` line:

```txt
init {"instance":"sysinfo","options":{"interval":5,"unit":"celsius"}}
```

Use this for script-specific settings such as polling intervals, units, paths, thresholds, or feature flags.

## Raw Protocol Applets

A raw exec applet reads lines from stdin and writes lines to stdout. Glimpse sends `init` and `event` messages to the child process. The child process sends `status` and `popover` messages back.

Read [Line Protocol](./exec-protocol.md) for the full message reference.

## Popover Components

Popover content is a component tree. Each node has a `type` and `data`, and the available component types are shared by raw protocol applets and SDK applets.

Read [Components](./exec-components.md) for the component reference.

## SDK Applets

SDK applets still run through `extends = "exec"`, but the SDK handles stdin, stdout, JSON encoding, events, and rendering.

Read [Exec SDK](../applets/exec-sdk.md) for installation and minimal examples in Python, TypeScript, Rust, and Go.

## See Also

| Page | Covers |
|---|---|
| [Line Protocol](./exec-protocol.md) | Raw protocol commands, message shapes, events, and shell examples. |
| [Components](./exec-components.md) | Popover component fields and component types. |
| [Exec SDK](../applets/exec-sdk.md) | SDK installation and language examples. |
| [Applet Tooling](./tooling.md) | Project scaffolding, live reload, linking, and diagnostics. |
