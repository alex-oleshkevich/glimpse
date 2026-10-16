# Appearance, region and location

These tables affect every glimpse surface: the panel, popovers, notifications, the lock screen and
the wallpaper.

## Appearance

The theme, light or dark, blur and animation speed.

<ConfigTable path="appearance" />

`animation-speed` accepts `0`, or a value from `0.1` to `10`. `1.0` is the default pace, `0.5` is
half as fast, `2.0` is twice as fast and `0` turns animations off.

### Light and dark

Two settings decide when glimpse is light or dark, and they do different jobs.
`glimpse-settings-daemon` must be running for `auto-dark` and the `glimpsectl appearance` commands
to change the desktop mode.

`auto-dark` decides the mode of your whole desktop. With `auto`, glimpse switches the desktop to
dark at sunset and back to light at sunrise. With `system`, glimpse leaves that decision to you or
another tool, for example the dark style switch in GNOME Settings.

Without a known location, `auto-dark = "auto"` cannot tell day from night and leaves the desktop
mode unchanged. See [Location](#location).

`color-scheme` decides whether glimpse itself follows that mode. With `auto` it does. With `light`
or `dark`, the bar, popovers, notifications and lock screen stay in that scheme whatever the rest of
the desktop does, which suits a dark bar over light apps.

| You want | Set |
| --- | --- |
| Everything dark at night, light by day | nothing, the defaults do this |
| To switch light and dark yourself | `auto-dark = "system"` |
| A dark bar all the time, apps following the sun | `color-scheme = "dark"` |

`glimpsectl appearance set light` or `glimpsectl appearance set dark` holds the desktop in one mode
without editing the file. This works with `auto-dark` set to either `auto` or `system`.
`glimpsectl appearance resume` clears the hold and returns to the configured behavior. Automatic
switching and the `set` commands write `org.gnome.desktop.interface color-scheme` to `prefer-light`
or `prefer-dark`, the preference GTK apps follow.

### Blur

With `blur` set, the bar, popovers and notifications blur the windows and wallpaper behind them:

```toml
[appearance]
blur = ["all"]
```

Your compositor draws the blur, so it needs to support it, and how strong it looks is set in the
compositor's own configuration. Where it isn't supported, glimpse stays solid and works the same.

Some compositors blur glimpse's popups against the wallpaper only, ignoring the windows behind
them. If that happens, tell the compositor to blur what is actually underneath with a layer rule
like this one:

```kdl
// ~/.config/niri/config.kdl
layer-rule {
    match namespace="^glimpse-(popover|notifications)$"
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
clock, the display applet's paused-until time, the night-light schedule, `glimpsectl sunset` output
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
desktop's light/dark mode unchanged. Run `glimpsectl geolocation status` to check the location and
solar phase. Changes to `[geolocation]` apply without a restart.

glimpse saves GeoClue fixes in `~/.local/state/glimpse/location.json` and uses the remembered fix
when GeoClue is unavailable, so sunset times remain available offline. A `manual` location comes
from the configuration and is never saved there.
