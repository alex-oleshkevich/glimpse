# Installed files

Every file an install puts on the system, and the files glimpse and your applets write at runtime.
Paths use the default `/usr` prefix. The `.deb` and `.rpm` packages install the same system files
and never write into a home directory.

| Installed at | Purpose |
| --- | --- |
| `/usr/bin/` | The glimpse binaries and the screen power helper |
| `/usr/lib/systemd/user/` | [Systemd units](/developers/systemd) |
| `/usr/share/dbus-1/services/` | [D-Bus activation files](/developers/systemd) |
| `/usr/share/xdg-desktop-portal/portals/` | [Portal backends](#portals) |
| `/etc/xdg-desktop-portal/` | [Portal preferences](#portals) |
| `/etc/pam.d/glimpse-lock` | [The lock screen's PAM stack](#pam) |
| `/etc/geoclue/conf.d/glimpse.conf` | [Location access](#geoclue) |
| `/usr/share/applications/`, `/usr/share/metainfo/`, `/usr/share/icons/hicolor/` | [App launchers and icons](#launcher-and-icons) |
| `/usr/share/glimpse/` | [Shared data](#shared-data) |
| `/usr/share/locale/<lang>/LC_MESSAGES/glimpse.mo` | Translations, one catalog per language |
| `~/.config/glimpse/config.toml`, `panel.toml` | [Starting configuration files](#your-configuration), written by the first start of a glimpse program |

## Applet files

| Path | What |
| --- | --- |
| `~/.local/share/glimpse/applets/<id>/` | An applet installed with `glimpsectl applet install`, with its `metadata.toml` |
| `~/.local/share/applications/<id>.desktop` | Its desktop entry: `Path` points at the directory above, and an `Exec` of `./<file>` is made absolute |
| `~/.local/state/glimpse/applets/<id>/` | The applet's state directory, mode 0700 |
| `$XDG_RUNTIME_DIR/glimpse/applets/` | The panel's applet sockets, `<panel-pid>-<slot>.sock` |

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

`/etc/pam.d/glimpse-lock` is the PAM service the lock screen authenticates against. It allows
authentication and account checks
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

Two glimpse programs have app launchers. glimpse-shot has
`/usr/share/applications/me.aresa.Glimpse.Shot.desktop`, with Screen, Window and Region actions, and
its AppStream entry in `/usr/share/metainfo/`. glimpse-lens has one entry per tool:
`me.aresa.Glimpse.Lens.desktop` (Magnifier), `me.aresa.Glimpse.Lens.Ruler.desktop` (Ruler) and
`me.aresa.Glimpse.Lens.Picker.desktop` (Color Picker). The Ruler and Color Picker entries pass
`--copy`, so the result lands on the clipboard. `/usr/share/icons/hicolor/` gets the glimpse,
glimpse-shot and glimpse-lens icons, each in a full color and a symbolic version.

## Shared data

`/usr/share/glimpse/` holds what every glimpse binary reads and nobody edits:

| File | Read by |
| --- | --- |
| `config.default.toml`, `panel.default.toml` | You: every default, uncommented, always matching the installed version. glimpse itself does not read them; the defaults are built in. |
| `config.commented.toml`, `panel.commented.toml` | You: the same defaults, every line commented out, to copy from. glimpse itself does not read them. |
| `config.schema.json`, `panel.schema.json` | Editors that validate TOML; reference one with `#:schema /usr/share/glimpse/config.schema.json`. |
| `language-codes.json` | The keyboard applet, which shortens layout names to codes such as `US`. A copy in `~/.config/glimpse/` takes precedence. |
| `themes/adwaita/` | The built-in theme: `panel.css`, `dark.css`, `wallpaper.css`, `lock.css` and the default wallpaper. |
| `sdk/templates/` | `glimpsectl applet new`, which copies it to start an applet. |
| `sdk/skills/` | Coding agents: `glimpse-command-applet` and `glimpse-exec-applet`; `glimpsectl applet new` copies the second into each project. |
| `portals/` | The install script, as the pristine copies of the portal preferences. |
| `LICENSE` | The license text. |

## Your configuration

The first start of the panel, notifications, lock screen or wallpaper creates the `~/.config/glimpse`
directory if it is missing and writes a `config.toml` and a `panel.toml` into it, each only when
absent. Each holds a `#:schema` line and a link to the documentation, nothing else. Nothing is
written when you start a program with `--config-dir`. The install script writes nothing into a home
directory, and an upgrade never touches these files.
