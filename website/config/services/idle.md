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

A profile can't have two listeners with the same `timeout`, and `on-idle` can't be empty.
`glimpsectl config validate` reports both. For these problems, a running glimpse-idle logs a warning
and applies the file anyway. Set `timeout = 0` to turn a listener off.

`on-idle` and `on-resume` run with `/bin/sh -c` in glimpse-idle's own environment. Commands for one
listener run one at a time without blocking other listeners. A failed command is only logged.

`glimpse-dpms` turns the screens off and on, and supports `niri` and `Hyprland` only. It needs `NIRI_SOCKET` or
`HYPRLAND_INSTANCE_SIGNATURE` in its environment and exits 1 when neither is set. With no `off` or
`on` argument, it prints usage and exits 2. `loginctl lock-session` locks the screen.

For syntax errors and a file that was already broken at startup, see
[When something is wrong](/config/index#when-something-is-wrong).

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

## Hold idle and suspend

Apps that play video or hold a call keep the timers from firing while `respect-inhibitors` is on.
A listener whose timeout passes during a hold waits. When the last hold ends, every waiting
listener fires at once if you are still idle. Listeners with `respect-inhibitors = false` still fire
during a hold.

`glimpsectl idle inhibit` creates a manual hold that blocks both `idle` and `suspend`. It needs
logind; `Idle1.Hold` fails without it. The registry allows 32 holds per source and 128 in total.

Without a command, `inhibit` prints the hold's id first; `--json` prints `{"id": 7}` instead. End
the hold with Ctrl-C, `--for`, or `glimpsectl idle release <id>`. Durations are whole positive
numbers with an optional `s`, `m`, or `h` suffix, such as `90`, `90s`, `15m`, or `2h`. `0`, `1d`,
and `1.5h` are refused with exit 2.

`glimpsectl idle inhibit -- CMD...` holds only while `CMD` runs and exits with the command's status.
`glimpsectl idle release <id>` can release a manual hold or an app's screen-saver inhibitor. A hold
another app acquired through logind is not releasable, and an unknown id exits 4. If you release a
manual hold from another command, its original `inhibit` process keeps running; Ctrl-C on that
process then exits 4 because the hold is already gone.

Apps that hold through `systemd-inhibit` are polled every 5 seconds, so their holds can take up to
5 seconds to count.

## Inspect inhibitors

`glimpsectl idle status` does not need the panel. Its SOURCES table lists `screensaver`, `portal`,
`login1`, and `wayland`, with a state of `ready`, `degraded`, or `unsupported`. Its INHIBITORS table
has `ID`, `WHO`, `WHY`, `BLOCKS`, `RELEASABLE`, and `EXPIRES` columns. `--watch` reprints the tables
on change, and `--json` prints JSON.
