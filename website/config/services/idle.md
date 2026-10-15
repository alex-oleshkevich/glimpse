# Idle

`[idle]` runs commands after a stretch without keyboard or pointer input. Each command is a
listener with its own timeout, and you keep one set of listeners for AC power and another for
battery.

<ConfigTable path="idle" />

<ConfigTable path="idle.profiles" />

<ConfigTable path="idle.profiles.ac" />

<ConfigTable path="idle.profiles.ac.listeners" />

`[idle.profiles.battery]` takes the same settings as `[idle.profiles.ac]`.

## Default listeners

| Profile | After | Runs | On input |
| --- | --- | --- | --- |
| AC | 15 minutes | `glimpse-dpms off` | `glimpse-dpms on` |
| AC | 30 minutes | `loginctl lock-session` | |
| AC | 60 minutes | `systemctl suspend` | |
| Battery | 15 minutes | `glimpse-dpms off` | `glimpse-dpms on` |
| Battery | 30 minutes | `systemctl suspend` | |

`glimpse-dpms` turns the screens off and on. `loginctl lock-session` locks the screen.

Writing `listeners` for a profile replaces that profile's defaults, so list every listener you
want:

```toml
[[idle.profiles.battery.listeners]]
timeout = 300
on-idle = "glimpse-dpms off"
on-resume = "glimpse-dpms on"

[[idle.profiles.battery.listeners]]
timeout = 600
on-idle = "systemctl suspend"
```

Apps that play video or hold a call keep the timers from firing while `respect-inhibitors` is on.
`glimpsectl idle status` shows which apps are holding them, and `glimpsectl idle inhibit` holds them
yourself.
