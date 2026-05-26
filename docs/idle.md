# Idle

Idle rules decide what happens when you stop using the computer.

By default, Glimpse runs a three-step ladder on both profiles:

| Step | AC | Battery |
|---|---|---|
| Turn monitors off | 10 minutes | 5 minutes |
| Lock session | 15 minutes | 15 minutes |
| Suspend | 60 minutes | 30 minutes |

Monitors come back on automatically when input resumes. The `/usr/share/glimpse/scripts/monitors` helper is installed alongside the binaries and supports both niri and hyprland out of the box.

## A Good Laptop Setup

The defaults above are already tuned for laptops. To customise, drop a block in `~/.config/glimpse/config.toml`:

```toml
[idle]
enabled = true
respect_inhibitors = true

[idle.profiles.ac]
listeners = [
  { timeout = 600, on_idle = "/usr/share/glimpse/scripts/monitors off", on_resume = "/usr/share/glimpse/scripts/monitors on" },
  { timeout = 900, on_idle = "loginctl lock-session" },
  { timeout = 3600, on_idle = "systemctl suspend" },
]

[idle.profiles.battery]
listeners = [
  { timeout = 300, on_idle = "/usr/share/glimpse/scripts/monitors off", on_resume = "/usr/share/glimpse/scripts/monitors on" },
  { timeout = 900, on_idle = "loginctl lock-session" },
  { timeout = 1800, on_idle = "systemctl suspend" },
]
```

Then enable the service:

```sh
systemctl --user enable --now glimpse-idle.service
```

## Listener Options

| Option | What it means |
|---|---|
| `timeout` | Seconds of no keyboard or mouse activity before the rule runs. |
| `on_idle` | Shell command to run after the timeout. |
| `on_resume` | Shell command to run when activity returns, but only if `on_idle` already ran. |
| `respect_inhibitors` | Optional per-rule override for apps that ask the desktop to stay awake. |

## AC And Battery Profiles

Glimpse can use different rules on charger and battery.

| Profile | Good for |
|---|---|
| `ac` | Longer timeouts while plugged in. |
| `battery` | Shorter timeouts to save power. |

When power state changes, Glimpse switches to the matching profile.

## Useful Commands

| Goal | Command |
|---|---|
| Lock the session | `loginctl lock-session` |
| Turn monitors off (niri or hyprland) | `/usr/share/glimpse/scripts/monitors off` |
| Turn monitors on (niri or hyprland) | `/usr/share/glimpse/scripts/monitors on` |
| Suspend | `systemctl suspend` |

`/usr/share/glimpse/scripts/monitors` detects the running compositor from `$NIRI_SOCKET` or `$HYPRLAND_INSTANCE_SIGNATURE` and dispatches the right command. For other compositors, use their native command directly.

## Inhibitors

Some apps ask the desktop not to idle. Video players, screen sharing, games, and presentation tools often do this.

This keeps those apps respected:

```toml
[idle]
respect_inhibitors = true
```

For one rule that should always run:

```toml
{ timeout = 900, on_idle = "loginctl lock-session", respect_inhibitors = false }
```
