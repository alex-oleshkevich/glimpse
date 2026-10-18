# systemd and D-Bus

The units glimpse installs, the units it creates for applets, the bus names it owns, and the applet
host interface.

## Session units

`glimpse-session.target` is `WantedBy=graphical-session.target` and wants every unit below except
weather and the screenshot portal, which start on their first D-Bus call. `systemctl --user reload
glimpse-session.target` sends `SIGHUP` to each except the lock and desktop units, and each rereads
its config; they also reread it on their own when the file changes.

| Unit | Service bus name | Restart |
| --- | --- | --- |
| `glimpse-panel.service` | `me.aresa.GlimpsePanel.AppletHost` | On failure |
| `glimpse-wallpaper.service` | None | On failure |
| `glimpse-lock.service` | None | Always, no start limit |
| `glimpse-notifications.service` | `me.aresa.Glimpse.Notifications`, `org.freedesktop.Notifications` | On failure |
| `glimpse-idle.service` | `me.aresa.Glimpse.Idle`, `me.aresa.Glimpse.Idle.Portal`, `org.freedesktop.ScreenSaver` | On failure |
| `glimpse-settings-daemon.service` | `me.aresa.Glimpse.Settings` | On failure |
| `glimpse-sunset.service` | `me.aresa.Glimpse.NightLight` | On failure, except exit 4 (no gamma control) |
| `glimpse-weather.service` | `me.aresa.Glimpse.Weather` | On failure |
| `glimpse-shot-portal.service` | `me.aresa.Glimpse.Shot.Portal` | No |
| `glimpse-desktop.service` | None | Oneshot at login |

Units that restart on failure wait 2 seconds and give up after 5 failures in 60 seconds.

::: warning
Never add sandboxing options to `glimpse-lock.service`, and never stop it while the screen is
locked. PAM password checks fail inside a sandbox, and a stopped lock screen leaves the session
locked with nothing to unlock it.
:::

## Applet units

The panel starts each exec applet as a transient unit:

```text
app-glimpse-<panel>-<applet>-<slot>-<pid>-<n>.service
```

| Property | Value |
| --- | --- |
| `Type` | `exec` |
| `Slice` | `app.slice` |
| `MemoryMax` | 256 MiB |
| `Restart` | `always`; `no` for a copy started by `glimpsectl applet run` |
| Restart delay | 1 second, rising in 6 steps to 60 seconds, with no start limit |
| `WorkingDirectory` | The entry's `Path`; for a copy from `glimpsectl applet run`, the directory of its desktop file |
| `Environment` | The panel's environment without systemd's own variables (`INVOCATION_ID`, `NOTIFY_SOCKET`, `LISTEN_*` and the like), plus `env` from the applet's config table and `GLIMPSE_APPLET_SOCKET` |
| `CollectMode` | `inactive-or-failed` |

```sh
systemctl --user list-units 'app-glimpse-*'
journalctl --user -u 'app-glimpse-*' -f
glimpsectl applet restart me.aresa.GlimpseTodo
```

## Applet host interface

`glimpse-panel` exports `me.aresa.Glimpse.AppletHost1` at `/me/aresa/Glimpse/AppletHost` on the bus
name `me.aresa.GlimpsePanel.AppletHost`. `glimpsectl applet run` uses it; any program can.

| Method | Arguments | Returns |
| --- | --- | --- |
| `Attach` | `desktop_path` s, `panel` u, `output` s, `zone` s, `options_json` s, `env` a{ss}, `stderr` h | A handle, s |
| `Restart` | `handle` s | Nothing |
| `SetOptions` | `handle` s, `options_json` s | Nothing |
| `Tree` | `handle` s | The widget tree as JSON, s |
| `Detach` | `handle` s | Nothing |

- `desktop_path` is an absolute path to a `.desktop` file. `panel` is the index of a `[[panels]]`
  entry. An empty `output` or `zone` lets the panel choose.
- `options_json` is a JSON object of at most 64 KiB.
- The applet's stderr goes to the `stderr` file descriptor.
- Only the caller that attached can use the handle. When it leaves the bus, its applets detach.
- The host accepts 8 attached applets per caller and 16 in total.
- `Attach` returns as soon as the panel accepts the request. The unit starts afterwards; a start
  failure shows on the bar, not as an error from `Attach`.
