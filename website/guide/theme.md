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
