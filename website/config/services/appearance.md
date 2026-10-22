# Appearance, region and location

These tables affect every glimpse surface: the panel, popovers, notifications, the lock screen and
the wallpaper.

## Appearance

The theme, light or dark, blur and animation speed.

<ConfigTable path="appearance" />

`animation-speed` accepts `0`, or a value from `0.1` to `10`. `1.0` is the default pace, `0.5` is
half as fast, `2.0` is twice as fast and `0` turns animations off.

### Light and dark

`desktop-color-scheme` decides the desktop's mode, which your apps follow; `color-scheme` decides
glimpse's own. You need `glimpse-settings-daemon` running for glimpse to set the desktop mode or
respond to `glimpsectl appearance` commands.

For `desktop-color-scheme`, choose `sun` (the default) to turn your desktop dark at sunset and
light at sunrise. Without a known location, it leaves your desktop mode unchanged. See
[Location](#location). Choose `light` or `dark` to set the desktop preference at startup and restore
it whenever another tool changes it. Choose `external` to leave the preference to you or another tool.

For `color-scheme`, choose `dark` (the default) or `light` to keep your bar, popovers, notifications, lock screen and wallpaper in one mode. Choose `desktop` to follow that preference.

| You want | `desktop-color-scheme` | `color-scheme` |
| --- | --- | --- |
| Follow the sun everywhere | `"sun"` | `"desktop"` |
| A dark bar with apps following the sun | `"sun"` | `"dark"` |
| Switch modes yourself in GNOME Settings | `"external"` | `"desktop"` |
| Always dark everywhere | `"dark"` | `"desktop"` |

`glimpsectl appearance set light` or `glimpsectl appearance set dark` holds your desktop in one mode
with any configured value. `glimpsectl appearance resume` clears the hold and returns to your
configured policy. With `external`, it restores the preference from before the hold only if another
tool hasn't changed it. With `sun` and no known location, `resume` reports `no solar phase yet` and
leaves the desktop mode unchanged. A hold ends on a desktop policy change or a clean daemon shutdown.
The desktop preference is `org.gnome.desktop.interface color-scheme`: `prefer-light` or `prefer-dark`.

### Blur

With `blur` set, the bar, popovers, notifications and password dialogs blur the windows and wallpaper behind them:

```toml
[appearance]
blur = true
```

Your compositor draws the blur, so it needs to support it, and how strong it looks is set in the
compositor's own configuration. glimpse asks for blur through the `ext-background-effect` protocol,
which needs GTK 4.23.3 or newer. Where it isn't supported, glimpse stays solid and works the same.

With `niri` 26.04 or newer you don't enable anything on the compositor side: it blurs every surface
that asks. Its `blur {}` section sets the strength for all of them.

By default `niri` blurs these surfaces against the wallpaper only, ignoring the windows behind them.
To blur what is actually underneath the bar, popovers, notifications and dialogs, add this layer rule:

```kdl
// ~/.config/niri/config.kdl
layer-rule {
    match namespace="^glimpse-(popover|notifications|panel|agents)$"
    background-effect {
        xray false
    }
}
```

### Themes

A theme is a directory of stylesheets. glimpse ships one, `adwaita`, and looks for your own under
`~/.config/glimpse/themes/<name>/` first. The shipped theme's only nonempty stylesheet is
`dark.css`, which makes the bar black in dark mode. Your own theme does not inherit it.

## Region

The language, how glimpse writes times and units. `hour-format` changes the world clock, next-event
rows, power charge and empty estimates, idle times and weather times. It does not change the bar
clock, the display applet's paused-until time, the night-light schedule, `glimpsectl night-light status` output
or the lock clock. The bar clock has its own `label-format`; set it to `%-I:%M %p` for a 12-hour
clock. The lock clock uses `time-format` under `[lock]`.

`hour-format = "locale"` follows `LC_TIME`, and `units = "locale"` follows `LC_MEASUREMENT`. With
`en_US`, those defaults produce 12-hour times and Fahrenheit.

An empty `language` uses your system language, and a new value applies the next time glimpse starts.
If the `LANGUAGE` environment variable is set in your session, it wins and `language` has no effect.
Date formatting, weekday names and month names follow your system settings, not `language`.

<ConfigTable path="regional" describe />

## Location

glimpse needs to know roughly where you are to switch to dark mode and warm the screen at sunset,
and to show the local weather. Leave the `[geolocation]` table out to ask GeoClue, the system
location service. If you write the table, `provider` is required. `latitude` and `longitude` are
read only with `manual`.

To keep location services off, or if GeoClue can't place you, write your coordinates instead.
Both are required with `manual`:

```toml
[geolocation]
provider = "manual"
latitude = 52.37
longitude = 4.90
```

`latitude` runs from -90 to 90, and `longitude` from -180 to 180. Config validation accepts values
outside those ranges, but glimpse then has no location, shows no sunset times and leaves the
desktop's light/dark mode unchanged when you choose `sun`. Run `glimpsectl geolocation status` to check the location and
solar phase. Changes to `[geolocation]` apply without a restart.

glimpse saves GeoClue fixes in `~/.local/state/glimpse/location.json` and uses the remembered fix
when GeoClue is unavailable, so sunset times remain available offline. A `manual` location comes
from the configuration and is never saved there.
