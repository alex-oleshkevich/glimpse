# Write a theme

A theme is a folder of CSS files that recolors and reshapes the bar, its popovers, notifications
and the lock screen. Most themes only redefine a handful of named values called tokens; the main
tokens are listed in [Theme tokens](/config/theme-tokens).

## Start a theme

Make a folder for it under `~/.config/glimpse/themes/` and put a `panel.css` in it:

```css
/* ~/.config/glimpse/themes/midnight/panel.css */
:root {
    --gl-panel: #101018;
    --gl-panel-fg: #e8e8f0;
    --gl-surface: mix(var(--gl-panel), white, 0.07);
    --gl-radius: 0.6rem;
}
```

Then select it by the folder's name:

```toml
[appearance]
theme = "midnight"
```

The bar, popovers, notifications, lock screen and wallpaper reload when you save. They also pick up
a theme folder you create after selecting its name. The screenshot app reads your CSS only when it
starts, so close and reopen it after an edit.

glimpse looks for the folder in `~/.config/glimpse/themes` first and in the installed themes under
`/usr/share/glimpse/themes` second. When neither has it, or `theme` is empty, glimpse uses `adwaita`
without logging anything. `glimpsectl config validate` passes too, so a typo in `theme` looks like
the default look. The fallback searches `~/.config/glimpse/themes/adwaita` before
`/usr/share/glimpse/themes/adwaita`, so your own `adwaita` folder is used before the shipped one.

Setting `GLIMPSE_THEME` in the environment overrides the configured `theme`. Setting
`GLIMPSE_THEMES_DIR` replaces both lookup roots for loading and watching themes.

## Which file styles what

| File | Styles |
| --- | --- |
| `panel.css` | the bar, its popovers, notification popups and the screenshot app |
| `lock.css` | the lock screen |
| `wallpaper.css` | the wallpaper |
| `dark.css` | all of the above, read only while glimpse is dark |

Every file is optional. A missing file leaves that surface with the built-in look; it does not fall
back to the same file in `adwaita`. glimpse-lens — the magnifier, and the color picker and ruler it
runs as tools — reads only the built-in stylesheet. It ignores your theme, your `styles.css` and
`dark.css` files, and `color-scheme`.

`wallpaper.css` has the plain `window` and `picture` nodes to target, with no glimpse classes.
glimpse tokens have no effect there.

Each file is a separate stylesheet, so a token set in `panel.css` does not reach the lock screen.
Set it in `lock.css` too, or put it in `~/.config/glimpse/styles.css`, which every surface except
glimpse-lens reads.

## Change the dark look

`dark.css` loads on top of the theme's other files whenever glimpse is dark, and unloads when it
turns light. Put only what differs there:

```css
/* ~/.config/glimpse/themes/midnight/dark.css */
:root {
    --gl-panel: black;
    --gl-surface: mix(var(--gl-panel), white, 0.09);
}
```

The shipped `adwaita` theme's only nonempty stylesheet is `dark.css`, which makes the bar black in
dark mode. Your theme does not inherit it. Without a `dark.css`, your bar uses GTK's header bar
color.

You choose glimpse's mode with `color-scheme` and your desktop's mode with `desktop-color-scheme`
under `[appearance]`; see
[light and dark](/guide/theme#light-and-dark).

## Make small changes without a theme

Two files in `~/.config/glimpse/` apply on top of whatever theme is selected, on the same surfaces:
`styles.css` all the time, and `dark.css` only in dark. They suit a tweak you want to keep when you
switch themes:

```css
/* ~/.config/glimpse/styles.css */
:root {
    --gl-font-family: "Inter", sans-serif;
}
```

## What wins when two files disagree

Later entries beat earlier ones:

1. the built-in stylesheet
2. the theme's file for that surface (`panel.css`, `lock.css` or `wallpaper.css`)
3. the theme's `dark.css`, in dark only
4. your `~/.config/glimpse/styles.css`
5. your `~/.config/glimpse/dark.css`, in dark only

So your own files always have the last word, in both light and dark.

## Style one part of the bar

Tokens cover colors, text sizes, corner radius and motion. To change something they don't reach,
write a rule for the part's CSS class:

```css
.panel {
    font-weight: 600;
}
```

GTK's inspector shows the class of anything you point at. Stop the bar, start one from a terminal
with the inspector, and start the service again when you are done:

```sh
systemctl --user stop glimpse-panel.service
GTK_DEBUG=interactive glimpse-panel
systemctl --user start glimpse-panel.service
```

Tokens are the part made for theming. Class names follow the code, so check rules written against
them after you upgrade.

## Ship several looks in one theme

`theme-variant` adds a CSS class to every glimpse window, so one theme can carry more than one
look:

```toml
[appearance]
theme = "midnight"
theme-variant = "compact"
```

```css
/* ~/.config/glimpse/themes/midnight/panel.css */
.compact .panel {
    font-size: 0.9rem;
}
```

The name may use letters, digits, `-` and `_`, and must not start with a digit; glimpse ignores any
other name and logs a warning.

## What CSS does not control

| To change | Use |
| --- | --- |
| how thick the bar is | `size` under [`[[panels]]`](/config/panels); a CSS minimum height would override a smaller size |
| animation speed | `animation-speed` under [`[appearance]`](/config/services/appearance); it sets `--gl-duration` above every stylesheet, so CSS cannot |
| blur strength | your compositor's settings |
| the accent color | your system's GTK accent, the one your apps use; a theme can still override `--gl-accent` |

For blur, `--gl-blur-opacity` sets how see-through a blurred surface is, and `--gl-blur-radius`
only switches blur on or off: GTK asks for no blur below 10px.

## Find mistakes

A rule that does not parse is skipped, and the process logs where it stopped:

```sh
journalctl --user -u glimpse-panel.service -g stylesheet
```

A `var()` that names a token that doesn't exist is not a parse error, and nothing is logged. A
background drops to transparent, while a text color falls back to the inherited color. If part of
the bar looks wrong after an edit, check the token names first.

## Share a theme

A theme is nothing more than its folder. Others copy it into their own `~/.config/glimpse/themes/`, or into
`/usr/share/glimpse/themes/` to make it available to every user on the machine.
