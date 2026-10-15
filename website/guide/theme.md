# Change how glimpse looks

Light and dark, blur and animation speed are settings under `[appearance]`; deeper changes are a
theme directory of CSS. The full setting list is in the
[appearance reference](/config/services/appearance).

## Light and dark

Two settings work together. `auto-dark` decides the desktop's mode: `auto` (the default) switches to
dark at sunset and back to light at sunrise, and GTK apps that follow the system setting switch with
it; `system` leaves the decision to you or another tool. `color-scheme` decides whether glimpse
itself follows that mode: `auto` follows it, while `light` or `dark` keeps the bar, popovers,
notifications and lock screen in one scheme whatever the desktop does.

```toml
[appearance]
auto-dark = "system"
color-scheme = "dark"
```

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

`1.0` is the default pace and `0` turns animations off.

## Custom CSS

A theme is a directory under `~/.config/glimpse/themes/<name>/` holding any of `panel.css`,
`wallpaper.css`, `lock.css` and `dark.css`. `panel.css` styles the bar, its popovers, notification
popups and the screenshot app; `wallpaper.css` and `lock.css` style the wallpaper and the lock
screen. `dark.css` loads on every surface, only while the effective scheme is dark. Point `[appearance]` at the directory's
name:

```toml
[appearance]
theme = "midnight"
```

glimpse looks in `~/.config/glimpse/themes` first, then the installed themes under
`/usr/share/glimpse/themes`, and falls back to the shipped `adwaita` theme when the name does not
resolve. A theme overrides the built-in CSS variables. This
`~/.config/glimpse/themes/midnight/panel.css` darkens the bar and the surfaces drawn over it:

```css
:root {
    --gl-panel: #101018;
    --gl-surface: mix(var(--gl-panel), white, 0.07);
}
```

The shipped theme does the same for dark mode: see `dark.css` under
`/usr/share/glimpse/themes/adwaita`.
