# Appearance, region and location

These tables affect every glimpse surface: the panel, popovers, notifications, the lock screen and
the wallpaper.

## Appearance

The theme, light or dark, blur and animation speed.

<ConfigTable path="appearance" />

### Light and dark

Two settings decide when glimpse is light or dark, and they do different jobs.

`auto-dark` decides the mode of your whole desktop. With `auto`, glimpse switches the desktop to
dark at sunset and back to light at sunrise, and GTK apps that follow the system setting switch
with it. With `system`, glimpse leaves that decision to you or another tool, for example the
dark style switch in GNOME Settings.

`color-scheme` decides whether glimpse itself follows that mode. With `auto` it does. With `light`
or `dark`, the bar, popovers, notifications and lock screen stay in that scheme whatever the rest of
the desktop does, which suits a dark bar over light apps.

| You want | Set |
| --- | --- |
| Everything dark at night, light by day | nothing, the defaults do this |
| To switch light and dark yourself | `auto-dark = "system"` |
| A dark bar all the time, apps following the sun | `color-scheme = "dark"` |

`glimpsectl settings mode dark` holds the desktop in one mode until you run
`glimpsectl settings resume`, without editing the file.

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
`~/.config/glimpse/themes/<name>/` first.

## Region

The language, the clock format and units. Each one defaults to `locale`, which follows your system
locale, so you only set them to differ from it.

<ConfigTable path="regional" describe />

## Location

glimpse needs to know roughly where you are to switch to dark mode and warm the screen at sunset,
and to show the local weather. By default it asks GeoClue, the system location service:

```toml
[geolocation]
provider = "geoclue"
```

To keep location services off, or if GeoClue can't place you, write your coordinates instead.
Both are required with `manual`:

```toml
[geolocation]
provider = "manual"
latitude = 52.37
longitude = 4.90
```

glimpse remembers the last location it was given, so sunset times stay right when you are offline.
