# Systemd units and D-Bus names

Every systemd user unit, D-Bus activation file and portal definition glimpse installs, as shipped in
`data/systemd/`, `data/dbus-1/services/` and `data/portals/`. All units are user units. Paths in the
installed files use the `/usr` prefix.

## Systemd units

| Unit | Runs | Bus name (`BusName`) | Relationships and restart |
| --- | --- | --- | --- |
| `glimpse-session.target` | The session target itself | None | `WantedBy=graphical-session.target`; `PartOf=` and `After=` it. Wants the panel, wallpaper, sunset, notifications, idle, lock, desktop and settings-daemon units. Propagates reloads to all of those except lock and desktop. |
| `glimpse-panel.service` | `glimpse-panel` (`Type=exec`) | None | Part of `glimpse-session.target`, after it. `Restart=on-failure` (which excludes `SIGTERM`, so losing the session bus does not restart it). |
| `glimpse-wallpaper.service` | `glimpse-wallpaper` (`Type=exec`) | None | Part of `glimpse-session.target`, after it. Restart on failure. |
| `glimpse-lock.service` | `glimpse-lock` (`Type=exec`) | None | Part of `graphical-session.target` only (not of `glimpse-session.target`), after it. `Restart=always`, `StartLimitIntervalSec=0`. No hardening options; see [How the lock screen works](/developers/lock-screen). |
| `glimpse-notifications.service` | `glimpse-notifications` (`Type=dbus`) | `me.aresa.Glimpse.Notifications` | Part of `glimpse-session.target` and `Requisite=` on it, after it: its start fails unless the target is already active. Restart on failure. |
| `glimpse-idle.service` | `glimpse-idle` (`Type=dbus`) | `me.aresa.Glimpse.Idle` | Part of `glimpse-session.target` and `Requisite=` on it, after it: its start fails unless the target is already active. Restart on failure. |
| `glimpse-settings-daemon.service` | `glimpse-settings-daemon` (`Type=dbus`) | `me.aresa.Glimpse.Settings` | Part of `glimpse-session.target` and `Requisite=` on it, after it: its start fails unless the target is already active. Restart on failure. |
| `glimpse-sunset.service` | `glimpse-sunset` (`Type=dbus`) | `me.aresa.Glimpse.NightLight` | Part of `glimpse-session.target`, after it. Restart on failure, except exit status 4 (the compositor has no gamma control), which `RestartPreventExitStatus=4` does not retry. No D-Bus activation file exists for this name. |
| `glimpse-weather.service` | `glimpse-weather` (`Type=dbus`) | `me.aresa.Glimpse.Weather` | Part of `glimpse-session.target` and `Requisite=` on it, after it: its start fails unless the target is already active. Restart on failure. Not wanted by the target; started by D-Bus activation. |
| `glimpse-shot-portal.service` | `glimpse-shot portal` (`Type=dbus`) | `me.aresa.Glimpse.Shot.Portal` | Part of `glimpse-session.target` and `Requisite=` on it, after it: its start fails unless the target is already active. No restart directive; started by D-Bus activation. |
| `glimpse-desktop.service` | `glimpsectl session declare` (`Type=oneshot`, `RemainAfterExit`) | None | Part of `glimpse-session.target`. After `niri.service`; before `graphical-session.target` and `xdg-desktop-autostart.target`. |

Common directives across the long-running services: `StartLimitBurst=5` within
`StartLimitIntervalSec=60`, `RestartSec=2`, and `ExecReload=/bin/kill -HUP $MAINPID` (a reload is a
`SIGHUP`; every binary also reloads on its own when its configuration files change). The shot-portal unit
sets none of these, and the lock unit replaces the start limit with `StartLimitIntervalSec=0`.

Hardening directives are set per unit. `NoNewPrivileges`, `LockPersonality` and `RestrictRealtime`
appear on every long-running unit except the lock and shot-portal units. The idle, settings-daemon,
sunset and weather units additionally set `ProtectKernelTunables`, `ProtectKernelModules`,
`ProtectControlGroups` and `RestrictSUIDSGID`; the wallpaper unit sets the same minus `RestrictSUIDSGID`;
the notifications unit sets `ProtectKernelTunables`, `ProtectKernelModules` and `ProtectControlGroups`.
The lock unit sets none of them, on purpose.

Transient units for exec applets are created at runtime by the panel and are not installed files; their
name pattern and properties are in [How exec applets work](/developers/exec-applets).

## D-Bus service files

Installed under `data/dbus-1/services/`. A call to the name starts the unit named by `SystemdService`
when no process owns the name.

| Name | `Exec` | `SystemdService` |
| --- | --- | --- |
| `me.aresa.Glimpse.Idle` | `/usr/bin/glimpse-idle` | `glimpse-idle.service` |
| `me.aresa.Glimpse.Idle.Portal` | `/usr/bin/glimpse-idle` | `glimpse-idle.service` |
| `me.aresa.Glimpse.Notifications` | `/usr/bin/glimpse-notifications` | `glimpse-notifications.service` |
| `me.aresa.Glimpse.Settings` | `/usr/bin/glimpse-settings-daemon` | `glimpse-settings-daemon.service` |
| `me.aresa.Glimpse.Shot.Portal` | `/usr/bin/glimpse-shot portal` | `glimpse-shot-portal.service` |
| `me.aresa.Glimpse.Shot` | `/usr/bin/glimpse-shot --gapplication-service` | None (plain D-Bus activation of the GTK application) |
| `me.aresa.Glimpse.Weather` | `/usr/bin/glimpse-weather` | `glimpse-weather.service` |
| `org.freedesktop.Notifications` | `/usr/bin/glimpse-notifications` | `glimpse-notifications.service` |
| `org.freedesktop.ScreenSaver` | `/usr/bin/glimpse-idle` | `glimpse-idle.service` |

There is no service file for `me.aresa.Glimpse.NightLight`: the name is owned only while
`glimpse-sunset.service` runs, and calls to it do not activate anything. The panel, wallpaper and lock
units likewise have no service files. Interface names differ from bus names for the glimpse providers
(the weather object, for example, is `me.aresa.Glimpse.Weather1` on the `me.aresa.Glimpse.Weather` name);
[Architecture](/developers/) lists them.

## Portals

Installed under `data/portals/`. glimpse provides two xdg-desktop-portal backends:

| Portal file | D-Bus name | Interface |
| --- | --- | --- |
| `glimpse.portal` | `me.aresa.Glimpse.Idle.Portal` | `org.freedesktop.impl.portal.Inhibit` |
| `glimpse-shot.portal` | `me.aresa.Glimpse.Shot.Portal` | `org.freedesktop.impl.portal.Screenshot` (version 3: `Screenshot`, `PickColor`, `AvailableTargets`) |

Both portal files carry `UseIn=glimpse`, so the portal frontend selects them when the session's desktop
identity includes glimpse. Two preference files state the same choices explicitly for setups that use
`portals.conf`: `glimpse-portals.conf` and `niri-portals.conf` each map
`org.freedesktop.impl.portal.Inhibit` to `glimpse` and `org.freedesktop.impl.portal.Screenshot` to
`glimpse-shot`. The screenshot backend spawns `glimpse-shot` children for captures and
`glimpse-lens --tool picker` for `PickColor`; it never constructs the screenshot application itself.
