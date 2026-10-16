# Installed files

Every file `scripts/install.sh` puts on the system, where it goes, and what reads it. The
destinations come from `scripts/glimpse-paths.sh`, which `scripts/uninstall.sh` shares, so an
uninstall removes exactly this list. Paths use the default `/usr` prefix; `PREFIX` changes it and
`DESTDIR` stages the whole tree for a package. The `.deb` and `.rpm` packages install the same system
files and never write into a home directory.

| Installed at | From | Purpose |
| --- | --- | --- |
| `/usr/bin/` | `target/release/`, `data/bin/glimpse-dpms` | The glimpse binaries and the screen power helper |
| `/usr/lib/systemd/user/` | `data/systemd/` | [Systemd units](#systemd-units) |
| `/usr/share/dbus-1/services/` | `data/dbus-1/services/` | [D-Bus activation files](#d-bus-service-files) |
| `/usr/share/xdg-desktop-portal/portals/` | `data/portals/*.portal` | [Portal backends](#portals) |
| `/etc/xdg-desktop-portal/` | `data/portals/*-portals.conf` | [Portal preferences](#portals) |
| `/etc/pam.d/glimpse-lock` | `data/pam.d/` | [The lock screen's PAM stack](#pam) |
| `/etc/geoclue/conf.d/glimpse.conf` | `data/geoclue/conf.d/` | [Location access](#geoclue) |
| `/usr/share/applications/`, `/usr/share/metainfo/`, `/usr/share/icons/hicolor/` | `data/` | [The screenshot app's launcher](#launcher-and-icons) |
| `/usr/share/glimpse/` | `data/`, `sdk/template/` | [Shared data](#shared-data) |
| `/usr/share/locale/<lang>/LC_MESSAGES/glimpse.mo` | `po/`, built by `just build-translations` | Translations, one catalog per language |
| `~/.config/glimpse/config.toml` | `data/config.commented.toml` | [A starting configuration](#your-configuration) |

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
sets none of these, and the lock unit replaces the start limit with `StartLimitIntervalSec=0`. The night light unit also sets `RestartPreventExitStatus=4`, so a session whose compositor has
no gamma control does not use up its restart budget.

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
identity includes glimpse. Two preference files in `/etc/xdg-desktop-portal/` state the same choices
explicitly for setups that use `portals.conf`: `glimpse-portals.conf` and `niri-portals.conf` each map
`org.freedesktop.impl.portal.Inhibit` to `glimpse` and `org.freedesktop.impl.portal.Screenshot` to
`glimpse-shot`. The screenshot backend spawns `glimpse-shot` children for captures and
`glimpse-lens --tool picker` for `PickColor`; it never constructs the screenshot application itself.

The preference files sit in `/etc` because an administrator may edit them. A pristine copy of each is
kept in `/usr/share/glimpse/portals/`; an install replaces a preference file only while it still
matches that copy, and otherwise keeps it and prints a warning naming it.

## PAM

`/etc/pam.d/glimpse-lock` is the PAM service the lock screen authenticates against (its
[`pam-service`](/config/services/lock) setting). It allows authentication and account checks
through the system's usual stack and denies password changes and sessions, because the lock screen
only ever checks a password. The install script picks the variant for the distribution: on a system
with `/etc/pam.d/common-auth` and no `/etc/pam.d/system-auth` it installs the `common-auth` variant
from `data/pam.d/debian/`, and otherwise the `system-auth` one. The `.rpm` package ships the variant
in `data/pam.d/fedora/`. Packages mark the file as a configuration file, so an edited copy survives an
upgrade.

## GeoClue

`/etc/geoclue/conf.d/glimpse.conf` grants glimpse location access without asking.
glimpse-settings-daemon is the only glimpse process that asks GeoClue, and the weather and night
light read the location from it. It runs in the background, with no window and nobody to answer a
prompt, so without this policy GeoClue would refuse it or wait on a prompt nobody sees.
The file sets `allowed=true` and `system=true` for the `glimpse` application id and leaves `users=`
empty, which means every user. The section name matches the desktop id glimpse sends to GeoClue.
Location is used only when [`[geolocation] provider`](/config/services/appearance) is `geoclue`, the
default.

## Launcher and icons

glimpse-shot is the only glimpse program with an app launcher:
`/usr/share/applications/me.aresa.Glimpse.Shot.desktop`, with Screen, Window and Region actions, its
AppStream entry in `/usr/share/metainfo/`. `/usr/share/icons/hicolor/` gets the glimpse-shot icon and
the glimpse icon, each in a full color and a symbolic version.

## Shared data

`/usr/share/glimpse/` holds what every glimpse binary reads and nobody edits:

| File | Read by |
| --- | --- |
| `config.default.toml` | You: every default, uncommented, always matching the installed version. glimpse itself does not read it; the defaults are built in. |
| `config.commented.toml` | The install script, and the first start of the panel, notifications, lock screen or wallpaper, which seed your configuration from it when you have none. The same defaults, every line commented out. |
| `config.schema.json` | Editors that validate TOML; reference it with `#:schema /usr/share/glimpse/config.schema.json`. |
| `language-codes.json` | The keyboard applet, which shortens layout names to codes such as `US`. A copy in `~/.config/glimpse/` takes precedence. |
| `themes/adwaita/` | The built-in theme: `panel.css`, `dark.css`, `wallpaper.css`, `lock.css` and the default wallpaper. |
| `sdk/template/` | `glimpsectl applet new`, which copies it to start an applet. |
| `portals/` | The install script, as the pristine copies of the portal preferences. |
| `LICENSE` | The license text. |

## Your configuration

When run without `DESTDIR`, the install script writes `~/.config/glimpse/config.toml` from
`config.commented.toml` if that file does not exist yet. Under `sudo` it uses the home directory of
the user who ran `sudo` and gives them the file. It never replaces an existing file, and an upgrade
never touches it.

