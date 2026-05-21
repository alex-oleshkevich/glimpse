# Theming

Glimpse themes are **theme packs** — directories that bundle panel CSS, lock screen CSS, and light/dark image assets (wallpapers, backdrops, lock backgrounds) under one name. A theme can be chosen by name in your config, and CSS edits hot-reload while the shell is running.

## Theme Pack Layout

A pack is a directory containing any subset of these files:

```
<pack>/
├── panel.css            Shell panel + popovers CSS
├── lock.css             Lock screen CSS
├── wallpaper-light.<ext>   Light-mode wallpaper image
├── wallpaper-dark.<ext>    Dark-mode wallpaper image
├── backdrop-light.<ext>    Light-mode backdrop (blurred behind popovers)
├── backdrop-dark.<ext>     Dark-mode backdrop
├── lock-light.<ext>        Light-mode lock screen background image
└── lock-dark.<ext>         Dark-mode lock screen background image
```

Image extensions supported: `png`, `jpg`, `jpeg`, `webp`, `avif`.

Every file is optional. If a pack only ships `panel.css`, that's fine — missing assets simply fall through to lower-priority sources.

## Search Path

Theme packs are resolved by name. Glimpse looks for `<pack-name>/<file>` in these roots, in order:

| Order | Root | Notes |
|---|---|---|
| 1 | `$GLIMPSE_THEME` (env, if set) | An absolute pack directory. Skips all other roots when set. |
| 2 | `<repo>/themes/<name>/` | **Dev only**, when built with `--features dev`. |
| 3 | `~/.config/glimpse/themes/<name>/` | User packs. |
| 4 | `/usr/share/glimpse/themes/<name>/` | System-installed packs. |

Resolution is **per-file**: panel.css can come from your home dir while wallpaper-dark.png comes from `/usr/share`. First hit per file wins.

## Choose A Theme

In `~/.config/glimpse/config.toml`:

```toml
theme = "rosepine"
theme_mode = "auto"
```

| Option | Default | Values | Meaning |
|---|---|---|---|
| `theme` | `"adwaita"` | pack name | Selects the active theme pack. |
| `theme_mode` | `"auto"` | `auto`, `dark`, `light` | Forces light/dark, or follows the system color scheme. |

### Environment Overrides

Two env vars can override the configured theme without editing config:

| Variable | What it does |
|---|---|
| `GLIMPSE_THEME` | Absolute path to a pack directory. Used as the only search root — overrides name lookup entirely. Best for one-off testing. |
| `GLIMPSE_THEME_NAME` | Theme **name** override. Same semantics as setting `theme = "..."` in config — still searches all roots. Best for switching between installed packs. |

`GLIMPSE_THEME` (path) wins over `GLIMPSE_THEME_NAME` (name) wins over `config.theme`.

### Per-Panel Mode

Each panel can pick its own mode:

```toml
[[panels]]
position = "top"
theme_mode = "dark"
left = ["pager"]
center = ["clock"]
right = ["network", "battery", "session"]
```

Useful when one panel sits over a bright wallpaper area and another over a dark one. Both panels share the same `theme` pack — only the mode flips.

## User Overrides

In addition to packs, you can keep your own override CSS at the top level of `themes/`:

```
~/.config/glimpse/themes/
├── panel.css        ← user override (top-level)
├── lock.css         ← user override (top-level)
└── rosepine/        ← a theme pack
    ├── panel.css
    └── ...
```

Override files load **last** and beat the active pack — useful for tweaking a couple of CSS variables without forking the pack.

Lock override path is configured via `lock.css_path` (default: `"themes/lock.css"`, resolved relative to your config dir).

## CSS Provider Stack

The shell loads CSS into four GTK style providers stacked by priority:

| Priority | Source | Role |
|---|---|---|
| `APPLICATION` (600) | `themes/base.css` | Token defaults + `--dark-*` defaults |
| `USER` (800) | pack `panel.css` | Per-theme variable values |
| `USER + 1` (801) | `themes/base-remap.css` | `.theme-dark` and `@media (prefers-color-scheme: dark)` rules that remap `--*` to `--dark-*` |
| `USER + 2` (802) | `~/.config/glimpse/themes/panel.css` | User final override |

**Why four?** GTK provider priority overrides selector specificity *across* providers. If the remap rules lived in the same provider as base defaults, a theme pack's `:root { --color-bg: <light> }` at `USER` would clobber `.theme-dark { --color-bg: var(--dark-color-bg) }` from `APPLICATION` no matter how specific the selector. Putting the remap at `USER + 1` ensures it wins over the pack's `:root` while still reading the pack's `--dark-*` values through `var()`.

The lock screen uses an analogous three-provider stack (`APPLICATION` bundled `lock.css` → `USER` pack `lock.css` → `USER + 1` user override).

## Writing A Theme

A theme typically only redeclares **variable values** — not full rule blocks. The base CSS owns the rules; themes own the palette.

### Minimal panel.css

```css
:root,
popover {
    /* Light side */
    --sys-accent: #2e7de9;
    --color-bg: #e1e2e7;
    --color-fg: #3760bf;
    --color-surface: #d0d5e3;
    --color-surface-raised: #c4c8da;
    --panel-bg: #d0d5e3;
    --panel-fg: #3760bf;

    /* Dark side — base-remap.css remaps these into --color-* / --sys-accent
     * when .theme-dark is active or the system prefers dark. */
    --dark-sys-accent: #7aa2f7;
    --dark-color-bg: #1a1b26;
    --dark-color-fg: #c0caf5;
    --dark-color-surface: #16161e;
    --dark-color-surface-raised: #292e42;
    --dark-panel-bg: #16161e;
    --dark-panel-fg: #c0caf5;
}
```

> **Always include `popover` in the selector list.** GTK4 popovers are detached widget subtrees, so CSS custom properties on `:root` don't cascade into them. `themes/base.css` follows the same pattern.

### Minimal lock.css

The bundled lock CSS exposes a `--lock-*` token contract. Themes only override the variables; rule structure stays in the bundled file.

```css
:root {
    --lock-bg: #1a1b26;
    --lock-scrim: rgba(22, 22, 30, 0.50);
    --lock-fg: #c0caf5;
    --lock-fg-secondary: rgba(192, 202, 245, 0.78);
    --lock-input-bg: rgba(41, 46, 66, 0.72);
    --lock-input-border-focus: #7aa2f7;
    --lock-avatar-bg: rgba(122, 162, 247, 0.20);
    /* …see "Lock Tokens" below for the full set */
}
```

## Active Token Contract

Below are the variables that the bundled CSS actually consumes. Setting any other `--xxx` in your theme has no effect.

### Foundation

| Variable | Default |
|---|---|
| `--space-1` … `--space-6` | `2px`, `4px`, `6px`, `8px`, `12px`, `16px` |
| `--radius-md` | `6px` |
| `--radius-lg` | `12px` |
| `--radius-pill` | `999px` |
| `--border-width` | `1px` |
| `--shadow-sm` | `0 1px 2px rgb(0 0 0 / 0.12)` |
| `--shadow-md` | `0 10px 24px rgb(0 0 0 / 0.16)` |
| `--opacity-disabled` | `0.4` |
| `--opacity-muted` | `0.64` |

### Typography

| Variable | Default |
|---|---|
| `--font-family-ui` | `"Adwaita Sans", system-ui, sans-serif` |
| `--font-size-base` | `15px` |
| `--font-size-ui` | `var(--font-size-base)` |
| `--font-size-panel` | `calc(var(--font-size-base) * 1)` |
| `--font-size-xs` / `-sm` / `-md` / `-lg` | `11px`, `12px`, `13px`, `15px` |
| `--font-weight-normal` / `-medium` / `-semibold` | `400`, `500`, `600` |

### Color — Light Values

These appear at `:root`. Themes override them for the **light** palette.

| Variable | Default | Notes |
|---|---|---|
| `--sys-accent` | `#3584e4` | Primary brand accent. |
| `--sys-accent-fg` | `#ffffff` | Text on accent. |
| `--color-accent` | `var(--sys-accent)` | Alias. |
| `--color-accent-fg` | `var(--sys-accent-fg)` | Alias. |
| `--color-bg` | `#fafafa` | Surface background. |
| `--color-fg` | `#1f1f1f` | Primary text. |
| `--color-surface` | `#ffffff` | Card / popover surface. |
| `--color-surface-raised` | `#f4f4f4` | Elevated surface (cards). |
| `--color-border` | `rgb(0 0 0 / 0.14)` | Default border. |
| `--color-muted-fg` | `#707070` | Subtitles, secondary text. |
| `--color-success` | `#2b7a4b` | Success / good state. |
| `--color-success-fg` | `#ffffff` | |
| `--color-warning` | `#b26400` | Warning state. |
| `--color-warning-fg` | `#ffffff` | |
| `--color-danger` | `#c01c28` | Error / destructive state. |
| `--color-danger-fg` | `#ffffff` | |

### Overlays

State overlays painted over surfaces.

| Variable | Default |
|---|---|
| `--overlay-hover` | `rgb(0 0 0 / 0.06)` |
| `--overlay-active` | `rgb(0 0 0 / 0.12)` |
| `--overlay-selected` | `rgb(53 132 228 / 0.18)` |
| `--overlay-focus` | `rgb(53 132 228 / 0.22)` |

### Panel

| Variable | Default |
|---|---|
| `--panel-bg` | `#ffffff` |
| `--panel-fg` | `#030712` |
| `--panel-opacity` | `1` |
| `--panel-padding-y` / `-x` | `0`, `var(--space-4)` |
| `--panel-margin` | `0` |
| `--panel-radius` | `0` |
| `--island-gap` | `4px` |
| `--island-bg` / `-border` / `-radius` | `transparent`, `0`, `0` |

### Indicators

| Variable | Default |
|---|---|
| `--indicator-fg` | `var(--panel-fg)` |
| `--indicator-bg` | `transparent` |
| `--indicator-hover-bg` | `rgb(255 255 255 / 0.08)` |
| `--indicator-active-bg` | `rgb(255 255 255 / 0.16)` |
| `--indicator-padding-x` / `-y` | `6px`, `4px` |
| `--indicator-size` / `--indicator-icon-size` | `22px` |
| `--indicator-font-weight` | `700` |
| `--indicator-radius` | `var(--radius-pill)` |

### Popovers

| Variable | Default |
|---|---|
| `--popover-bg` | `var(--color-surface)` |
| `--popover-fg` | `var(--color-fg)` |
| `--popover-border` | `var(--color-border)` |
| `--popover-shadow` | `var(--shadow-md)` |
| `--popover-radius` | `var(--radius-lg)` |
| `--popover-padding` | `4px` |
| `--popover-section-gap` | `12px` |
| `--popover-small-width` / `-medium-width` / `-large-width` / `-xlarge-width` | `280px`, `320px`, `540px`, `620px` |
| `--popover-large-height` | `600px` |
| `--popover-min-width` / `-min-height` | `var(--popover-small-width)`, `0px` |

### Rows / Cards / Badges / Hero

| Variable | Default |
|---|---|
| `--row-fg` | `var(--color-fg)` |
| `--row-muted-fg` | `var(--color-muted-fg)` |
| `--row-hover-bg` / `--row-active-bg` / `--row-selected-bg` | overlay values |
| `--row-radius` | `var(--radius-md)` |
| `--row-padding-x` / `-y` | `8px`, `4px` |
| `--card-bg` / `-fg` / `-border` | surface values |
| `--card-hover-bg` | mixed surface |
| `--card-radius` | `var(--radius-lg)` |
| `--card-shell-padding` / `--card-padding` | `0px`, `12px` |
| `--card-extra-shadow` / `--card-hover-extra-shadow` | `0 0 transparent`, `var(--shadow-sm)` |
| `--footer-fg` / `--footer-border` / `--footer-hover-bg` | row values |
| `--badge-bg` / `-fg` / `-radius` | overlay/color values, pill |
| `--badge-padding-x` / `-y` | `8px`, `2px` |
| `--hero-title-fg` / `--hero-subtitle-fg` | color tokens |
| `--hero-icon-size` | `32px` |
| `--status-success` / `--status-warning` / `--status-danger` / `--status-accent` | semantic aliases |

### Color — Dark Values

These appear at `:root` as defaults. Themes override them to define the **dark** palette. `base-remap.css` remaps the corresponding `--*` token to read from these when the panel is in dark mode (either via `.theme-dark` class or `@media (prefers-color-scheme: dark)`).

| Variable | Default |
|---|---|
| `--dark-sys-accent` | `#3584e4` |
| `--dark-sys-accent-fg` | `#ffffff` |
| `--dark-color-bg` | `#1f1f1f` |
| `--dark-color-fg` | `#ffffff` |
| `--dark-color-surface` | `#262626` |
| `--dark-color-surface-raised` | `#303030` |
| `--dark-color-border` | `rgb(255 255 255 / 0.12)` |
| `--dark-color-muted-fg` | `#a7a7a7` |
| `--dark-color-success` / `-fg` | `#2b7a4b` / `#ffffff` |
| `--dark-color-warning` / `-fg` | `#b26400` / `#ffffff` |
| `--dark-color-danger` / `-fg` | `#c01c28` / `#ffffff` |
| `--dark-overlay-hover` | `rgb(255 255 255 / 0.08)` |
| `--dark-overlay-active` | `rgb(255 255 255 / 0.14)` |
| `--dark-overlay-selected` | `rgb(120 174 255 / 0.24)` |
| `--dark-overlay-focus` | `rgb(120 174 255 / 0.28)` |
| `--dark-panel-bg` | `#000000` |
| `--dark-panel-fg` | `#ffffff` |
| `--dark-popover-bg` | `#262626` |
| `--dark-popover-fg` | `#ffffff` |
| `--dark-popover-border` | `var(--dark-color-border)` |
| `--dark-row-fg` / `-muted-fg` | dark color values |
| `--dark-card-bg` / `-fg` / `-border` | dark surface values |
| `--dark-badge-bg` / `-fg` | dark overlay/color values |
| `--dark-indicator-hover-bg` / `-active-bg` | dark overlay values |

### Motion

| Variable | Default |
|---|---|
| `--motion-fast` | `120ms` |
| `--ease-standard` | `cubic-bezier(0.2, 0, 0, 1)` |
| `--press-scale` | `0.985` |
| `--popover-animation-duration` | `160ms` |

### Notifications

A self-contained set covering popup and inline notification surfaces (see `themes/base.css` for the full list — `--notification-popup-*`, `--notification-card-*`, `--notification-group-*`, etc.).

### Session Confirmation

| Variable | Default |
|---|---|
| `--session-confirmation-margin` | `calc(var(--space-6) + var(--space-4))` |
| `--session-confirmation-close-duration` | `70ms` |
| `--session-confirmation-open-duration` | `90ms` |
| `--session-confirmation-ease` | `cubic-bezier(0.22, 1, 0.36, 1)` |

### Compatibility Aliases

| Variable | Maps to |
|---|---|
| `--accent-bg` | `var(--color-accent)` |
| `--on-accent` | `var(--color-accent-fg)` |
| `--border` | `var(--color-border)` |
| `--border-opacity` | `14%` |
| `--popover-section-spacing` | `var(--popover-section-gap)` |
| `--dim-opacity` | `var(--opacity-muted)` |

## Libadwaita Accent Colors

`--sys-accent` flows into **Glimpse's own** CSS rules (`.panel`, `.indicator`, `.badge`, …). Stock libadwaita widgets — `Switch`, `CheckButton`, `ProgressBar`, links, focus rings — read their accent from libadwaita's CSS variables: `--accent-color`, `--accent-bg-color`, `--accent-fg-color` (introduced in libadwaita 1.6).

**Themes don't need to do anything.** `themes/base.css` declares:

```css
:root, popover {
    --accent-color: var(--sys-accent);
    --accent-bg-color: var(--sys-accent);
    --accent-fg-color: var(--sys-accent-fg);
}
```

So any theme that sets `--sys-accent` (which is the standard contract — see "Color — Light Values" above) automatically tints stock libadwaita widgets too. Dark-mode follows automatically: `base-remap.css` remaps `--sys-accent` to `--dark-sys-accent`, and `--accent-bg-color: var(--sys-accent)` re-evaluates at use time.

| Variable | What it tints |
|---|---|
| `--accent-bg-color` | Filled accent surfaces (switch thumb, progress bar fill, primary button bg). |
| `--accent-color` | Standalone accent — borders, focus rings, link text. |
| `--accent-fg-color` | Text/icons drawn over `--accent-bg-color`. |

Per-panel `theme_mode = "dark"` flips libadwaita accents in that panel for free: `.theme-dark` remaps `--sys-accent → var(--dark-sys-accent)`, and `--accent-bg-color: var(--sys-accent)` re-evaluates to the dark accent. No `@define-color` or programmatic API needed.

## Lock Tokens

`glimpse-lock/resources/lock.css` declares a `--lock-*` token set that themes override in their `lock.css`. The default values render an opaque dark lock screen; themes typically swap surfaces and accent tones.

| Variable | Default | Role |
|---|---|---|
| `--lock-bg` | `#101010` | Window/background base. |
| `--lock-scrim` | `rgba(0, 0, 0, 0.35)` | Overlay tint above the wallpaper. |
| `--lock-fg` | `white` | Clock, name, primary text. |
| `--lock-fg-emphasis` | `rgba(255, 255, 255, 0.9)` | Modal titles. |
| `--lock-fg-secondary` | `rgba(255, 255, 255, 0.78)` | Date. |
| `--lock-fg-status` | `rgba(255, 255, 255, 0.72)` | Status line. |
| `--lock-fg-muted` | `rgba(255, 255, 255, 0.68)` | Body text. |
| `--lock-fg-caps` | `rgba(255, 255, 255, 0.88)` | Caps-lock indicator. |
| `--lock-input-bg` / `-bg-focus` | white tints | Password field background. |
| `--lock-input-border` / `-border-focus` | white tints | Password field border. |
| `--lock-input-highlight` / `-shadow` | subtle highlights/shadow on the field. |
| `--lock-avatar-bg` | `rgba(255, 255, 255, 0.2)` | Avatar placeholder. |
| `--lock-button-hover-bg` / `-active-bg` | overlay values | Control button states. |
| `--lock-menu-bg` | dark transparent | Power menu surface. |
| `--lock-modal-bg` / `-border` / `-shadow` | confirmation modal surface. |
| `--lock-modal-button-bg` / `-hover-bg` / `-danger-bg` | modal action buttons. |

## Wallpaper, Backdrop, Lock Image Fallback

The wallpaper, backdrop, and lock background each follow a fallback chain. **Config always wins over theme assets.**

**Wallpaper** (`[wallpaper]` section):
1. `wallpaper.path` — explicit config
2. `<pack>/wallpaper-{light,dark}.<ext>` — themed

**Backdrop** (`[backdrop]` section):
1. `backdrop.path` — explicit config
2. `<pack>/backdrop-{light,dark}.<ext>` — themed
3. Resolved wallpaper image — final fallback

**Lock background** (`[lock.background]` section):
1. `lock.background.path` — explicit config (lock-specific)
2. `wallpaper.path` — config (shared with wallpaper)
3. `<pack>/lock-{light,dark}.<ext>` — themed lock background
4. `<pack>/wallpaper-{light,dark}.<ext>` — themed wallpaper, final fallback

The image picked for each slot depends on the **effective theme mode** at the moment of resolution. The wallpaper and lock apps subscribe to GNOME's `org.gnome.desktop.interface color-scheme` gsetting (which the shell keeps in sync with its effective mode) and re-resolve on changes.

## Hot Reload

The shell watches `~/.config/glimpse/themes/`, `/usr/share/glimpse/themes/`, and (in dev builds) `<repo>/themes/` recursively for `.css` changes. Edits to the active pack's `panel.css`, the user override `panel.css`, or `base.css` itself trigger a reload without restarting the shell.

The lock screen watches the active `lock.css` and override.

## Source Logging

Run with `GLIMPSE_LOG_LEVEL=info` to see resolved sources in tracing logs:

- `shell: loaded ...` — base.css, base-remap.css, pack panel.css, override panel.css paths
- `wallpaper: resolved sources` — `theme=…`, `mode=…`, `wallpaper_source=config|theme|color-only`, `wallpaper_path=…`, `backdrop_source=config|theme|wallpaper-fallback`, `backdrop_path=…`
- `lock: resolved sources` — `theme=…`, `mode=…`, `pack_css=…`, `override_css=…`, `background_source=config.lock.background|config.wallpaper|theme.lock-image|theme.wallpaper|none`, `background_path=…`

## Built-In Theme Packs

| Pack | Light | Dark | Wallpapers |
|---|---|---|---|
| `rosepine` | Rosé Pine Dawn | Rosé Pine Moon | [rose-pine/wallpapers](https://github.com/rose-pine/wallpapers) (leafy, block-wave, obliquas) |
| `tokyonight` | Tokyo Night Day | Tokyo Night | [tokyo-night/wallpapers](https://github.com/tokyo-night/wallpapers) (minimal gnome / stripes / abstract lockscreen) |

Try them:

```sh
GLIMPSE_THEME_NAME=rosepine cargo run -p glimpse-shell
GLIMPSE_THEME_NAME=tokyonight cargo run -p glimpse-shell
```

Or set in config:

```toml
theme = "tokyonight"
theme_mode = "auto"
```

## Practical Rules

| Rule | Why |
|---|---|
| Keep contrast high | Panel and lock text must stay readable on busy wallpapers. |
| Pick one accent | `--sys-accent` flows through indicators, badges, links. Don't fight it. |
| Always set both `--*` and `--dark-*` | Otherwise dark mode falls back to base defaults. |
| Include `popover` in selectors | Popovers are detached widget trees — `:root` alone won't reach them. |
| Test light + dark + per-panel mode | The cascade has three triggers; verify each. |

## Troubleshooting

| Problem | Check |
|---|---|
| Theme not applied | Confirm `theme = "name"` matches a directory at one of the search roots. Check logs for `theme pack has no panel.css`. |
| Dark mode looks wrong | Make sure the pack declares `--dark-*` siblings, not only `--*`. |
| Popovers untouched | Variable rules must use `:root, popover { ... }`. |
| Per-panel mode ignored | Confirm `base-remap.css` is loaded (check shell logs for `loaded bundled base-remap.css`). |
| Lock background is theme's, not config's | Config wins by design — only checked if config left `lock.background.path` and `wallpaper.path` unset. |
| Image change didn't reload | The wallpaper watcher only watches the *currently active* image path. Switching themes triggers a re-resolve. |
