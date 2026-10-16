# Change how glimpse looks

Light and dark, blur and animation speed are settings under `[appearance]`; deeper changes are a
theme directory of CSS. The full setting list is in the
[appearance reference](/config/services/appearance).

## Light and dark

`desktop-color-scheme` decides the desktop's mode, which your apps follow; `color-scheme` decides
glimpse's own. The defaults are `sun` and `desktop`: with a known location, your desktop turns dark
at sunset and light at sunrise, and glimpse follows it. Without a location, `sun` leaves your
desktop mode unchanged.

Choose `light` or `dark` for `desktop-color-scheme` to keep your desktop in that mode, even if
another tool changes it. Choose `external` to switch modes yourself in GNOME Settings or let
another tool decide. You need `glimpse-settings-daemon` running for glimpse to set the desktop mode.

With `color-scheme = "desktop"`, your bar, popovers, notifications, lock screen and wallpaper follow
the desktop. Choose `light` or `dark` to keep glimpse in one mode. For a dark bar with apps following
the sun:

```toml
[appearance]
desktop-color-scheme = "sun"
color-scheme = "dark"
```

`glimpsectl appearance set light` or `glimpsectl appearance set dark` gives you a temporary hold
with any desktop policy. `glimpsectl appearance resume` returns to your configured policy. With
`external`, it restores the preference from before the hold if another tool hasn't changed it.

The accent color comes from the system: glimpse reads the same accent your other GTK apps use, so
there is nothing to set.

## Blur

```toml
[appearance]
blur = ["all"]
```

The list takes `panel`, `popover`, `notification` or `all`. Your compositor draws the blur and sets
its strength; where blur is not supported, glimpse stays solid and works the same.

## Animation speed

```toml
[appearance]
animation-speed = 0.5
```

`1.0` is the default pace, `0.5` is half as fast and `2.0` is twice as fast. The accepted range is
`0.1` to `10`, or `0` to turn animations off.

## Custom CSS

A theme is a folder of CSS files under `~/.config/glimpse/themes/` that you select by name:

```toml
[appearance]
theme = "midnight"
```

[Write a theme](/guide/theming) walks through making one, and [Theme tokens](/config/theme-tokens)
lists the main values a theme can change.
