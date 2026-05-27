# Exec Applet

Use an exec applet when a custom panel item needs to stay alive. It can update its status, show a custom popover, handle clicks and scroll events, and keep its own local state.

For a simple launcher or menu, use a [command applet](./command.md) instead.

## How It Runs

| Step | What happens |
| --- | --- |
| Start | The shell reads the package file and starts the configured command. |
| Init | The applet receives an `init` message with its instance name and `[exec.options]`. |
| Status | The applet sends status updates for the panel item. |
| Popover | The applet can send popover content when it has details to show. |
| Events | Click, scroll, and open events are sent back to the applet. |
| Restart | If the process exits, it restarts after `restart_delay_ms`. |

Most users should start with [Getting Started](./getting-started.md), then come back to this page when they need the config reference.

## Basic Config

```toml
# ~/.config/glimpse/applets/sysinfo.toml
id = "sysinfo"
type = "exec"

[exec]
command = ["sh", "-c", "~/.config/glimpse/scripts/sysinfo"]
restart_delay_ms = 1000
env_forward = false

[exec.options]
interval_seconds = 5
```

Add the package id to a panel section:

```toml
[[panels]]
right = ["sysinfo", "network", "battery"]
```

## Applet Project Directories

For a reusable applet project, keep the package file beside the applet code:

```text
my-applet/
├── applet.toml
└── main.py
```

Example `applet.toml`:

```toml
id = "my-applet"
type = "exec"

[exec]
command = ["uv", "run", "main.py"]
restart_delay_ms = 1000
```

Run it during development:

```bash
glimpse-shell applets dev
```

The development command writes a temporary package under `~/.config/glimpse/applets` while it runs. The default panel already includes `__dev__`; keep or add that slot in custom panel layouts to show active development applets.

When the applet is ready for local use, link it:

```bash
glimpse-shell applets link
```

For sharing an applet with other users, distribute `applet.toml` together with the executable or script. [Applet Tooling](./tooling.md) covers the package handoff.

## Options

| Field | Default | Description |
| --- | --- | --- |
| `command` | required | Program and arguments to start. |
| `restart_delay_ms` | `1000` | Delay before the program restarts after exit. Minimum `50`. |
| `work_dir` | unset | Working directory for the command. |
| `options` | `{}` | Custom JSON-like data sent to the applet in the `init` message. |
| `env` | `{}` | Extra environment variables passed to the command. |
| `env_forward` | `false` | Set `true` to inherit the parent process environment. |

## Environment And Working Directory

Use `work_dir` when the command needs to run from a specific directory:

```toml
[exec]
command = ["./target/debug/my-applet"]
work_dir = "/home/me/Projects/my-applet"
```

Use `env` for values that should be explicit:

```toml
[exec.env]
RUST_LOG = "info"
```

Set `env_forward = true` only when the applet needs the full parent environment. Keeping it `false` makes development and startup behavior easier to reason about.

The shell always adds its applet IPC socket environment variables for SDK helpers, even when `env_forward = false`.

## Options In Init

`[exec.options]` is applet-owned data. The shell does not interpret it; it sends the table to the child process in the first `init` line:

```txt
init {"instance":"sysinfo","options":{"interval_seconds":5}}
```

Use this for applet-specific settings such as polling intervals, labels, thresholds, paths, or feature flags.

## Choose An Implementation

| Option | Use it when |
| --- | --- |
| [Exec SDK](../applets/exec-sdk.md) | You want normal language APIs for status, popovers, and events. |
| [Line Protocol](./exec-protocol.md) | You want a tiny script or need to understand the raw messages. |
| [Components](./exec-components.md) | You need the exact status and popover component shapes. |

SDK applets are the easiest path for most applets. Raw protocol applets are useful for short scripts or debugging.

## Development Flow

Use [Applet Tooling](./tooling.md) for the full workflow:

1. Create a project.
2. Run it in development mode.
3. Show it through the `__dev__` panel slot.
4. Link it for local use.
5. Distribute the applet by sharing `applet.toml` with the executable or script.
6. Diagnose package and runtime issues.

## See Also

| Page | Use it for |
| --- | --- |
| [Getting Started](./getting-started.md) | Build your first exec applet. |
| [Applet Tooling](./tooling.md) | Project commands and development workflow. |
| [Command Applet](./command.md) | Simple launchers and menus. |
| [Custom Applets](./index.md) | Overview and path selection. |
