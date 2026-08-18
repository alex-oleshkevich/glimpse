# Theming

Glimpse uses GTK CSS. The built-in base style is always loaded, then an optional theme pack and your own override CSS can change colors, spacing, panel shape, popovers, and lock screen styling.

The first-class customization path is simple: put CSS variables in `~/.config/glimpse/themes/panel.css`. You usually do not need to target GTK classes or widget nodes.

## Quick start

Create a final override file:

```txt
~/.config/glimpse/themes/panel.css
```

Use it as a variable override file. Start small:

```css
:root,
popover {
    --sys-accent: #7c3aed;
    --sys-accent-fg: #ffffff;

    --panel-bg: rgb(20 20 24 / 0.82);
    --panel-fg: #f8fafc;
    --panel-padding-y: 4px;
    --panel-padding-x: 8px;

    --island-bg: rgb(255 255 255 / 0.08);
    --island-border: 1px solid rgb(255 255 255 / 0.10);
    --island-radius: 8px;

    --popover-bg: #24242a;
    --popover-fg: #ffffff;
    --popover-border: rgb(255 255 255 / 0.12);
}
```

Treat custom selectors and classes as advanced overrides. Variables are the supported surface for changing the look without depending on internal widget structure.

CSS reloads while the panel is running.

Theme pack selection and light/dark mode live in the main config. See [Theme and mode](./configuration.md#theme-and-mode) for `theme` and `theme_mode`.

## Theme packs

A theme pack is a directory with any subset of these files:

```txt
<pack>/
├── panel.css
├── lock.css
├── wallpaper-light.<ext>
├── wallpaper-dark.<ext>
├── backdrop-light.<ext>
├── backdrop-dark.<ext>
├── lock-light.<ext>
└── lock-dark.<ext>
```

Supported image extensions are `png`, `jpg`, `jpeg`, `webp`, and `avif`.

Install user packs under:

```txt
~/.config/glimpse/themes/<pack-name>/
```

Glimpse resolves each file independently. For example, `panel.css` can come from your user pack while `wallpaper-dark.png` comes from a system pack with the same name.

## Search order

Theme packs are resolved by name in this order:

| Order | Root | Notes |
|---|---|---|
| **1** | `$GLIMPSE_THEME` | Absolute path to one pack directory. When set, it is the only root. |
| **2** | `<repo>/themes/<name>/` | Development builds only, with the `dev` feature. |
| **3** | `~/.config/glimpse/themes/<name>/` | User-installed packs. |
| **4** | `/usr/share/glimpse/themes/<name>/` | Packaged system themes. |

`GLIMPSE_THEME_NAME` overrides the configured `theme` name but still uses the normal search order. Set `GLIMPSE_THEME_NAME=rosepine` before starting the panel to try a pack without editing config.

`GLIMPSE_THEME` wins over `GLIMPSE_THEME_NAME`, and `GLIMPSE_THEME_NAME` wins over `theme = "..."`.

## Override files

Top-level override files live beside theme pack directories:

```txt
~/.config/glimpse/themes/
├── panel.css
├── lock.css
└── rosepine/
    ├── panel.css
    └── lock.css
```

| File | What it overrides |
|---|---|
| `~/.config/glimpse/themes/panel.css` | Your panel variable overrides. Loaded last in the shell, above the active pack. |
| `~/.config/glimpse/themes/lock.css` | Default lock override path, loaded above the active pack lock CSS. |

The lock override path is configurable:

```toml
[lock]
css_path = "themes/lock.css"
```

Relative lock CSS paths are resolved from the Glimpse config directory.

## Light and dark values

For panel themes, set light values and matching dark values:

```css
:root,
popover {
    --sys-accent: #3584e4;
    --color-bg: #fafafa;
    --color-fg: #1f1f1f;
    --panel-bg: #ffffff;
    --panel-fg: #030712;

    --dark-sys-accent: #78aeff;
    --dark-color-bg: #1f1f1f;
    --dark-color-fg: #ffffff;
    --dark-panel-bg: #000000;
    --dark-panel-fg: #ffffff;
}
```

`themes/base-remap.css` maps dark mode to the `--dark-*` values. If a theme only sets light variables, dark mode falls back to built-in dark defaults.

Always include `popover` in the selector list. GTK popovers are separate widget subtrees, so variables on `:root` alone do not reliably reach them.

## Panel token groups

These are the main variables consumed by `themes/base.css`. Override these before writing class-specific CSS.

| Group | Variables |
|---|---|
| Spacing | `--space-1` through `--space-10` |
| Radius | `--radius-sm`, `--radius-md`, `--radius-lg`, `--radius-xl`, `--radius-2xl`, `--radius-pill` |
| Typography | `--font-family-ui`, `--font-size-base`, `--font-size-ui`, `--font-size-panel`, `--font-size-xs`, `--font-size-sm`, `--font-size-md`, `--font-size-lg`, `--font-size-xl` |
| Accent | `--sys-accent`, `--sys-accent-fg`, `--color-accent`, `--color-accent-fg` |
| Base colors | `--color-bg`, `--color-fg`, `--color-surface`, `--color-surface-raised`, `--color-border`, `--color-muted-fg` |
| Status colors | `--color-success`, `--color-warning`, `--color-danger`, plus `--status-success`, `--status-warning`, `--status-danger`, `--status-accent` |
| Overlays | `--overlay-hover`, `--overlay-active`, `--overlay-selected`, `--overlay-focus` |
| Panel | `--panel-bg`, `--panel-fg`, `--panel-opacity`, `--panel-padding-y`, `--panel-padding-x`, `--panel-margin`, `--panel-radius` |
| Islands | `--island-gap`, `--island-bg`, `--island-border`, `--island-radius` |
| Indicators | `--indicator-fg`, `--indicator-bg`, `--indicator-hover-bg`, `--indicator-active-bg`, `--indicator-padding-x`, `--indicator-padding-y`, `--indicator-size`, `--indicator-icon-size`, `--indicator-font-weight`, `--indicator-radius` |
| Popovers | `--popover-bg`, `--popover-fg`, `--popover-border`, `--popover-shadow`, `--popover-radius`, `--popover-padding`, `--popover-section-gap`, `--popover-small-width`, `--popover-medium-width`, `--popover-large-width`, `--popover-xlarge-width`, `--popover-xxlarge-width` |
| Rows and cards | `--row-fg`, `--row-muted-fg`, `--row-hover-bg`, `--row-active-bg`, `--row-selected-bg`, `--row-radius`, `--row-padding-x`, `--row-padding-y`, `--card-bg`, `--card-fg`, `--card-border`, `--card-hover-bg`, `--card-radius`, `--card-padding` |
| Notifications | `--message-bg`, `--message-bg-second`, `--message-bg-lower`, `--message-header-fg`, `--message-radius`, `--message-padding`, `--message-shadow`, `--message-width`, `--message-height` |
| Motion | `--motion-fast`, `--motion-medium`, `--motion-slow`, `--ease-standard`, `--press-scale`, `--popover-animation-duration` |

Compatibility aliases still exist for older theme snippets: `--accent-bg`, `--on-accent`, `--border`, `--border-opacity`, `--popover-section-spacing`, and `--dim-opacity`.

## Libadwaita accent

Glimpse maps its accent into libadwaita variables:

```css
:root,
popover {
    --accent-color: var(--sys-accent);
    --accent-bg-color: var(--sys-accent);
    --accent-fg-color: var(--sys-accent-fg);
}
```

That means widgets such as switches, check buttons, progress bars, links, and focus rings follow `--sys-accent`.

## Lock CSS

You can theme the lock screen separately from the panel.

Put your lock overrides here:

```txt
~/.config/glimpse/themes/lock.css
```

A theme pack can also include `lock.css`. Your personal `lock.css` wins over the theme pack, so it is the best place for small tweaks.

A minimal lock override:

```css
:root {
    --lock-bg: #101010;
    --lock-scrim: rgba(0, 0, 0, 0.35);
    --lock-fg: white;
    --lock-fg-secondary: rgba(255, 255, 255, 0.78);
    --lock-input-bg: rgba(255, 255, 255, 0.22);
    --lock-input-border-focus: rgba(255, 255, 255, 0.64);
    --lock-avatar-bg: rgba(255, 255, 255, 0.20);
    --lock-button-hover-bg: rgba(255, 255, 255, 0.14);
    --lock-menu-bg: rgba(18, 18, 18, 0.58);
    --lock-modal-bg: rgb(18, 18, 18);
}
```

Main lock tokens:

| Group | Variables |
|---|---|
| Surfaces | `--lock-bg`, `--lock-scrim` |
| Text | `--lock-fg`, `--lock-fg-emphasis`, `--lock-fg-secondary`, `--lock-fg-status`, `--lock-fg-muted`, `--lock-fg-caps` |
| Password input | `--lock-input-bg`, `--lock-input-bg-focus`, `--lock-input-border`, `--lock-input-border-focus`, `--lock-input-highlight`, `--lock-input-shadow` |
| Auth layout | `--lock-auth-margin-top`, `--lock-avatar-name-gap`, `--lock-name-password-gap`, `--lock-auth-message-gap` |
| Avatar | `--lock-avatar-bg` |
| Buttons | `--lock-button-hover-bg`, `--lock-button-active-bg` |
| Menu and modal | `--lock-menu-bg`, `--lock-modal-bg`, `--lock-modal-border`, `--lock-modal-shadow`, `--lock-modal-button-bg`, `--lock-modal-button-hover-bg`, `--lock-modal-button-danger-bg` |

## Image fallbacks

Config paths always win over theme assets.

| Surface | Fallback order |
|---|---|
| Wallpaper | `[wallpaper].path`, then `<pack>/wallpaper-{light,dark}.<ext>` |
| Backdrop | `[backdrop].path`, then `<pack>/backdrop-{light,dark}.<ext>`, then the resolved wallpaper image |
| Lock background | `[lock.background].path`, then `[wallpaper].path`, then `<pack>/lock-{light,dark}.<ext>`, then `<pack>/wallpaper-{light,dark}.<ext>` |

When only one light/dark image exists, Glimpse uses that image for both modes.

## Hot reload

| Area | What reloads |
|---|---|
| Shell CSS | `.css` changes under `~/.config/glimpse/themes/`, `/usr/share/glimpse/themes/`, and dev `themes/` when available. |
| Lock CSS | Bundled dev `lock.css`, active pack `lock.css`, and configured lock override. |
| Wallpaper and lock images | The currently resolved image files. Changing config or theme mode re-resolves the selected files. |

If you add a brand-new image file to a pack and it is not the currently resolved file, switch theme mode or reload config to force a fresh resolution.

## Shipped theme packs

| Pack | Panel CSS | Lock CSS | Images |
|---|---|---|---|
| `rosepine` | Yes | Yes | Light/dark wallpaper, backdrop, and lock images |

Try it without editing config by setting `GLIMPSE_THEME_NAME=rosepine` before starting the panel.

Or set it permanently:

```toml
theme = "rosepine"
theme_mode = "auto"
```

## Troubleshooting

| Problem | Check |
|---|---|
| Theme not applied | Confirm the pack directory exists at one of the search roots and contains `panel.css`. |
| Only base styling appears | The configured pack name may not resolve. The default `theme = "adwaita"` still works because base CSS is always loaded. |
| Dark mode looks wrong | Add matching `--dark-*` variables for the light variables you override. |
| Popovers ignore colors | Put variables under `:root, popover`, not only `:root`. |
| User override wins unexpectedly | `~/.config/glimpse/themes/panel.css` is intentionally the final variable override layer. |
| Class selector stopped working | Prefer variables when possible. GTK classes and widget structure can change as applets evolve. |
| Lock CSS not applied | Check `[lock].css_path`; relative paths start from `~/.config/glimpse/`. |
| New image did not appear | Re-resolve by changing config, changing theme mode, or restarting the relevant daemon. |

### See also

| Document | Purpose |
|---|---|
| [Configuration](./configuration.md) | Main config file and `theme` / `theme_mode` fields. |
| [Wallpaper](./wallpaper.md) | Wallpaper and backdrop config. |
| [Lock](./lock.md) | Lock screen config and CSS preview. |
