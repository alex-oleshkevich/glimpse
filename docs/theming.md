# Theming

Glimpse is meant to look professional: calm, polished, readable, and free of noisy effects. Theme it like a desktop you can use all day, not like a demo.

## Theme Files

| File | Purpose |
|---|---|
| `~/.config/glimpse/themes/<name>.css` | Panel, applets, popovers, and notification styling. |
| `~/.config/glimpse/themes/lock.css` | Lock screen styling. |

The default theme name is `adwaita`.

## Choose A Shell Theme

In `~/.config/glimpse/config.toml`:

```toml
theme = "adwaita"
theme_mode = "auto"
```

| Option | Default | Values | Meaning |
|---|---|---|---|
| `theme` | `"adwaita"` | theme name | Selects the shell theme. |
| `theme_mode` | `"auto"` | `auto`, `dark`, `light` | Chooses light/dark styling. |

`theme` and `theme_mode` are independent. `theme = "mytheme"` selects the CSS file, while `theme_mode = "dark"` applies dark mode classes to the shell.

If `theme = "mytheme"`, Glimpse looks for:

```txt
~/.config/glimpse/themes/mytheme.css
```

If that file does not exist, Glimpse keeps the embedded base theme and logs that the user theme was not found.

## Per-Panel Mode

Panels can also choose a mode:

```toml
[[panels]]
position = "top"
theme_mode = "dark"
left = ["pager"]
center = ["clock"]
right = ["network", "battery", "session"]
```

| Option | Default | Values | Meaning |
|---|---|---|---|
| `theme_mode` | `"dark"` | `auto`, `dark`, `light` | Applies mode classes to that panel. |

Use this when one panel sits on a bright wallpaper area and another sits on a dark one.

## Create A Theme

Create a file:

```txt
~/.config/glimpse/themes/mytheme.css
```

Use it:

```toml
theme = "mytheme"
theme_mode = "dark"
```

Theme name changes and CSS changes inside the themes directory are watched while the shell is running.

## Useful Shell Selectors

Start small. These selectors cover the parts most users want to adjust first:

```css
.panel {
  background: rgba(20, 20, 20, 0.82);
  color: #f4f4f4;
}

.applet {
  padding: 0 8px;
}

.applet:hover {
  background: rgba(255, 255, 255, 0.08);
}

.popover,
.card-surface {
  background: rgba(28, 28, 28, 0.96);
  border: 1px solid rgba(255, 255, 255, 0.10);
}

.badge,
.status-dot {
  background: #7aa2f7;
}
```

## Lock Screen Theme

Export starter CSS:

```sh
glimpse-lock --export-css
```

This writes:

```txt
~/.config/glimpse/themes/lock.css
```

Lock config defaults to that path:

```toml
[lock]
css_path = "themes/lock.css"
```

| Option | Default | Meaning |
|---|---|---|
| `css_path` | `"themes/lock.css"` | CSS file loaded over the built-in lock screen style. |

Use preview mode while editing:

```sh
glimpse-lock --preview
```

Preview reloads CSS changes and does not lock your real session.

## Useful Lock Selectors

```css
.lock-clock {
  font-size: 80px;
  font-weight: 600;
}

.lock-date {
  font-size: 18px;
}

.lock-auth-panel {
  margin-top: 180px;
}

.lock-entry {
  background: rgba(255, 255, 255, 0.20);
  color: white;
}

.lock-controls {
  color: white;
}
```

## Practical Style Rules

| Rule | Why |
|---|---|
| Keep contrast high | Panel and lock text must stay readable on busy wallpapers. |
| Use transparency carefully | A little glass effect looks polished; too much makes text noisy. |
| Pick one accent color | Status badges and warnings should not fight each other. |
| Keep animations subtle | The desktop should feel smooth, not distracting. |
| Avoid giant panel labels | Long labels crowd the panel and break screenshots. |
| Test on your real wallpaper | A theme that looks good on a flat color can fail on an image. |

## Troubleshooting

| Problem | Check |
|---|---|
| Theme did not change | Confirm `theme = "name"` matches `~/.config/glimpse/themes/name.css`. |
| CSS edits do not reload | Make sure the file is inside `~/.config/glimpse/themes/`. |
| Lock CSS did not load | Run `glimpse-lock --export-css`, then check `css_path`. |
| Text is hard to read | Increase background opacity or lower wallpaper brightness. |

## Theme Variables

These variables come from the embedded shell base theme. `theme_mode` can remap mode-sensitive tokens such as `--color-bg`, `--panel-bg`, `--popover-bg`, `--row-fg`, `--card-bg`, and `--badge-bg` to their `--dark-*` values.

### Layout Tokens

| Variable | Default |
|---|---|
| `--space-1` | `2px` |
| `--space-2` | `4px` |
| `--space-3` | `6px` |
| `--space-4` | `8px` |
| `--space-5` | `12px` |
| `--space-6` | `16px` |
| `--radius-sm` | `4px` |
| `--radius-md` | `6px` |
| `--radius-lg` | `12px` |
| `--radius-pill` | `999px` |
| `--border-width` | `1px` |
| `--shadow-sm` | `0 1px 2px rgb(0 0 0 / 0.12)` |
| `--shadow-md` | `0 10px 24px rgb(0 0 0 / 0.16)` |
| `--opacity-disabled` | `0.4` |
| `--opacity-muted` | `0.64` |

### Type Tokens

| Variable | Default |
|---|---|
| `--font-family-ui` | `"Adwaita Sans", system-ui, sans-serif` |
| `--font-size-xs` | `11px` |
| `--font-size-sm` | `12px` |
| `--font-size-md` | `13px` |
| `--font-size-lg` | `15px` |
| `--font-weight-normal` | `400` |
| `--font-weight-medium` | `500` |
| `--font-weight-semibold` | `600` |

### Color Tokens

| Variable | Default |
|---|---|
| `--sys-accent` | `#3584e4` |
| `--sys-accent-fg` | `#ffffff` |
| `--color-bg` | `#fafafa` |
| `--color-fg` | `#1f1f1f` |
| `--color-surface` | `#ffffff` |
| `--color-surface-raised` | `#f4f4f4` |
| `--color-border` | `rgb(0 0 0 / 0.14)` |
| `--color-border-strong` | `rgb(0 0 0 / 0.22)` |
| `--color-muted` | `#6e6e6e` |
| `--color-muted-fg` | `#707070` |
| `--color-accent` | `var(--sys-accent)` |
| `--color-accent-fg` | `var(--sys-accent-fg)` |
| `--color-success` | `#2b7a4b` |
| `--color-success-fg` | `#ffffff` |
| `--color-warning` | `#b26400` |
| `--color-warning-fg` | `#ffffff` |
| `--color-danger` | `#c01c28` |
| `--color-danger-fg` | `#ffffff` |
| `--overlay-hover` | `rgb(0 0 0 / 0.06)` |
| `--overlay-active` | `rgb(0 0 0 / 0.12)` |
| `--overlay-selected` | `rgb(53 132 228 / 0.18)` |
| `--overlay-focus` | `rgb(53 132 228 / 0.22)` |
| `--overlay-disabled` | `rgb(255 255 255 / 0.24)` |

### Dark Mode Tokens

| Variable | Default |
|---|---|
| `--dark-color-bg` | `#1f1f1f` |
| `--dark-color-fg` | `#ffffff` |
| `--dark-color-surface` | `#262626` |
| `--dark-color-surface-raised` | `#303030` |
| `--dark-color-border` | `rgb(255 255 255 / 0.12)` |
| `--dark-color-border-strong` | `rgb(255 255 255 / 0.24)` |
| `--dark-color-muted` | `#9f9f9f` |
| `--dark-color-muted-fg` | `#a7a7a7` |
| `--dark-overlay-hover` | `rgb(255 255 255 / 0.08)` |
| `--dark-overlay-active` | `rgb(255 255 255 / 0.14)` |
| `--dark-overlay-selected` | `rgb(120 174 255 / 0.24)` |
| `--dark-overlay-focus` | `rgb(120 174 255 / 0.28)` |
| `--dark-overlay-disabled` | `rgb(0 0 0 / 0.18)` |
| `--dark-panel-bg` | `#000000` |
| `--dark-panel-fg` | `#ffffff` |
| `--dark-panel-border` | `rgb(255 255 255 / 0.08)` |
| `--dark-indicator-hover-bg` | `rgb(255 255 255 / 0.08)` |
| `--dark-indicator-active-bg` | `rgb(255 255 255 / 0.16)` |
| `--dark-popover-bg` | `#262626` |
| `--dark-popover-fg` | `#ffffff` |
| `--dark-popover-border` | `var(--dark-color-border)` |
| `--dark-row-fg` | `var(--dark-color-fg)` |
| `--dark-row-muted-fg` | `var(--dark-color-muted-fg)` |
| `--dark-card-bg` | `var(--dark-color-surface-raised)` |
| `--dark-card-fg` | `var(--dark-color-fg)` |
| `--dark-card-border` | `var(--dark-color-border)` |
| `--dark-badge-bg` | `var(--dark-overlay-selected)` |
| `--dark-badge-fg` | `var(--dark-color-fg)` |

### Panel And Indicator Tokens

| Variable | Default |
|---|---|
| `--panel-bg` | `#ffffff` |
| `--panel-fg` | `#030712` |
| `--panel-border` | `rgb(255 255 255 / 0.08)` |
| `--panel-opacity` | `1` |
| `--indicator-fg` | `var(--panel-fg)` |
| `--indicator-hover-bg` | `rgb(255 255 255 / 0.08)` |
| `--indicator-active-bg` | `rgb(255 255 255 / 0.16)` |
| `--indicator-checked-bg` | `color-mix(in srgb, var(--color-accent) 88%, black 12%)` |
| `--indicator-checked-fg` | `var(--color-accent-fg)` |
| `--indicator-danger-bg` | `color-mix(in srgb, var(--color-danger) 86%, black 14%)` |
| `--indicator-warning-bg` | `color-mix(in srgb, var(--color-warning) 86%, black 14%)` |
| `--indicator-padding-x` | `8px` |
| `--indicator-padding-y` | `0px` |
| `--indicator-gap` | `6px` |
| `--indicator-icon-size` | `16px` |
| `--indicator-radius` | `var(--radius-pill)` |

### Popover And Row Tokens

| Variable | Default |
|---|---|
| `--popover-bg` | `var(--color-surface)` |
| `--popover-fg` | `var(--color-fg)` |
| `--popover-border` | `var(--color-border)` |
| `--popover-shadow` | `var(--shadow-md)` |
| `--popover-radius` | `var(--radius-lg)` |
| `--popover-padding` | `4px` |
| `--popover-section-gap` | `12px` |
| `--popover-row-gap` | `6px` |
| `--popover-small-width` | `280px` |
| `--popover-medium-width` | `320px` |
| `--popover-large-width` | `540px` |
| `--popover-large-height` | `600px` |
| `--popover-xlarge-width` | `620px` |
| `--popover-min-width` | `var(--popover-small-width)` |
| `--popover-min-height` | `0px` |
| `--row-fg` | `var(--color-fg)` |
| `--row-muted-fg` | `var(--color-muted-fg)` |
| `--row-hover-bg` | `var(--overlay-hover)` |
| `--row-active-bg` | `var(--overlay-active)` |
| `--row-selected-bg` | `var(--overlay-selected)` |
| `--row-radius` | `var(--radius-md)` |
| `--row-padding-x` | `8px` |
| `--row-padding-y` | `4px` |
| `--row-gap` | `8px` |
| `--row-icon-size` | `16px` |
| `--row-trailing-gap` | `8px` |

### Component Tokens

| Variable | Default |
|---|---|
| `--hero-title-fg` | `var(--color-fg)` |
| `--hero-subtitle-fg` | `var(--color-muted-fg)` |
| `--hero-gap` | `12px` |
| `--hero-padding-x` | `0px` |
| `--hero-padding-y` | `0px` |
| `--hero-icon-size` | `32px` |
| `--hero-icon-radius` | `var(--radius-md)` |
| `--card-bg` | `var(--color-surface-raised)` |
| `--card-fg` | `var(--color-fg)` |
| `--card-border` | `var(--color-border)` |
| `--card-hover-bg` | `color-mix(in srgb, var(--card-bg) 92%, var(--overlay-hover))` |
| `--card-radius` | `var(--radius-lg)` |
| `--card-shell-padding` | `0px` |
| `--card-padding` | `12px` |
| `--card-gap` | `8px` |
| `--card-extra-shadow` | `0 0 transparent` |
| `--card-hover-extra-shadow` | `var(--shadow-sm)` |
| `--footer-bg` | `transparent` |
| `--footer-fg` | `var(--color-fg)` |
| `--footer-border` | `var(--color-border)` |
| `--footer-hover-bg` | `var(--row-hover-bg)` |
| `--footer-padding-x` | `8px` |
| `--footer-padding-y` | `4px` |
| `--badge-bg` | `var(--overlay-selected)` |
| `--badge-fg` | `var(--color-fg)` |
| `--badge-radius` | `var(--radius-pill)` |
| `--badge-padding-x` | `8px` |
| `--badge-padding-y` | `2px` |
| `--status-success` | `var(--color-success)` |
| `--status-warning` | `var(--color-warning)` |
| `--status-danger` | `var(--color-danger)` |
| `--status-accent` | `var(--color-accent)` |

### Notification Tokens

| Variable | Default |
|---|---|
| `--notification-popup-min-width` | `500px` |
| `--notification-inline-image-size` | `48px` |
| `--notification-control-size` | `24px` |
| `--notification-card-padding` | `12px` |
| `--notification-card-margin-y` | `2px` |
| `--notification-card-radius` | `12px` |
| `--notification-group-radius` | `14px` |
| `--notification-action-radius` | `8px` |
| `--notification-list-padding-y` | `4px` |
| `--notification-group-gap` | `4px` |
| `--notification-group-header-padding` | `4px 12px 0px 12px` |
| `--notification-stack-offset` | `2px` |
| `--notification-stack-second-height` | `4px` |
| `--notification-stack-third-height` | `3px` |
| `--notification-popup-card-margin-bottom` | `8px` |
| `--notification-popup-padding` | `var(--space-5)` |
| `--notification-popup-card-padding` | `16px` |
| `--notification-popup-card-radius` | `16px` |
| `--notification-popup-enter-duration` | `240ms` |
| `--notification-popup-leave-duration` | `180ms` |
| `--notification-popup-enter-offset` | `6px` |
| `--notification-popup-enter-offset-negative` | `-6px` |
| `--notification-popup-shadow` | `0 6px 18px rgb(0 0 0 / 0.5), 0 1px 3px rgb(0 0 0 / 0.3)` |
| `--notification-inline-image-radius` | `10px` |
| `--notification-summary-size` | `14px` |
| `--notification-title-size` | `15px` |
| `--notification-subtitle-size` | `12px` |
| `--notification-app-name-size` | `13px` |
| `--notification-body-size` | `13px` |
| `--notification-popup-body-size` | `14px` |
| `--notification-time-size` | `11px` |
| `--notification-surface-bg` | `color-mix(in srgb, var(--color-fg) 6%, transparent)` |
| `--notification-surface-hover-bg` | `color-mix(in srgb, var(--color-fg) 10%, transparent)` |
| `--notification-inline-image-bg` | `color-mix(in srgb, var(--color-fg) 4%, transparent)` |
| `--notification-dismiss-hover-bg` | `color-mix(in srgb, currentColor 10%, transparent)` |
| `--notification-popup-bg` | `var(--color-bg)` |
| `--notification-popup-hover-bg` | `color-mix(in srgb, var(--notification-popup-bg) 95%, white)` |

### Motion Tokens

| Variable | Default |
|---|---|
| `--motion-fast` | `120ms` |
| `--motion-normal` | `180ms` |
| `--motion-slow` | `240ms` |
| `--ease-standard` | `cubic-bezier(0.2, 0, 0, 1)` |
| `--ease-emphasized` | `cubic-bezier(0.2, 0, 0, 1.15)` |
| `--press-scale` | `0.985` |
| `--popover-animation-duration` | `160ms` |
| `--popover-enter-offset` | `6px` |
| `--popover-enter-scale` | `0.985` |

### Session Confirmation Tokens

| Variable | Default |
|---|---|
| `--session-confirmation-margin` | `calc(var(--space-6) + var(--space-4))` |
| `--session-confirmation-close-duration` | `70ms` |
| `--session-confirmation-open-duration` | `90ms` |
| `--session-confirmation-ease` | `cubic-bezier(0.22, 1, 0.36, 1)` |

### Legacy Compatibility Tokens

| Variable | Default |
|---|---|
| `--view-bg` | `var(--color-bg)` |
| `--on-view` | `var(--color-fg)` |
| `--accent-bg` | `var(--color-accent)` |
| `--on-accent` | `var(--color-accent-fg)` |
| `--radius` | `var(--radius-lg)` |
| `--card-padding-legacy` | `var(--card-padding)` |
| `--border-opacity` | `14%` |
| `--border` | `var(--color-border)` |
| `--popover-section-spacing` | `var(--popover-section-gap)` |
| `--dim-opacity` | `var(--opacity-muted)` |
