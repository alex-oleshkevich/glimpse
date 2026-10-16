# Theme tokens

Tokens are the CSS custom properties a theme redefines, usually in `:root`. These are the main
tokens a theme can usefully change. Where a default names a GTK color, glimpse follows your GTK
theme until you set the token. How to write a theme is in [Write a theme](/guide/theming).

The color picker, ruler and magnifier load only the built-in stylesheet. Your theme and
`~/.config/glimpse/styles.css` cannot change them.

## Surfaces and text

| Token | Default | Paints |
| --- | --- | --- |
| `--gl-panel` | GTK's header bar color | the bar's background |
| `--gl-panel-fg` | GTK's window text color | text and icons on the bar |
| `--gl-surface` | GTK's popover color | popovers and other floating cards |
| `--gl-surface-fg` | GTK's popover text color | text on popovers |
| `--gl-notification` | `--gl-surface` mixed 8% toward `--gl-surface-fg` | notification cards |
| `--gl-notification-fg` | `--gl-surface-fg` | text on notification cards |
| `--gl-muted` | `--gl-surface-fg` at 65% | secondary text and meter fills |
| `--gl-dim` | `--gl-surface-fg` at 38% | less important text |
| `--gl-faint` | `--gl-surface-fg` at 22% | the faintest text, empty meter tracks |
| `--gl-border` | GTK's border color | borders and separators |
| `--gl-hover` | `--gl-surface-fg` at 8% | the background under the pointer |
| `--gl-active` | `--gl-surface-fg` at 16% | the background of a pressed or open item |
| `--gl-control` | `--gl-border`, stronger | the calendar's Today button, shown when you browse away from the current month or select another day |
| `--gl-disabled` | GTK's disabled opacity, 0.5 | the opacity of a control you can't use |

`--gl-muted`, `--gl-dim` and `--gl-faint` come out a little lighter in light mode than in dark,
the same as dimmed text in GTK apps.

## Accent and status

| Token | Default | Paints |
| --- | --- | --- |
| `--gl-accent` | GTK's accent color | the current workspace in a labeled pager, today and the selected day in the calendar, prompt confirm buttons, the notification progress bar, sparkline marks and the checked screenshot tool's foreground |
| `--gl-accent-fg` | GTK's text-on-accent color | text drawn on `--gl-accent` |
| `--gl-accent-text` | GTK's accent text color | accent-colored text, focus outlines, attention dots |
| `--gl-accent-soft` | `--gl-accent` at 18% | badges, highlighted groups, the checked screenshot tool's background |
| `--gl-danger-text` | GTK's destructive color | error text |
| `--gl-danger-soft` | GTK's destructive background at 18% | the background behind errors |
| `--gl-warning-text` | GTK's warning color | warning text |
| `--gl-warning-soft` | GTK's warning background at 18% | the background behind warnings |

## Shape, depth and type

| Token | Default | Sets |
| --- | --- | --- |
| `--gl-radius` | `0.95rem` | the corner radius of popovers, sheets and the screenshot editor's panels |
| `--gl-shadow` | black at 55% | the shadow color the elevations use |
| `--gl-elevation-raised` | a small, soft shadow | the bar, where the outer shadow is clipped and not visible, and notifications in a list |
| `--gl-elevation-floating` | a larger shadow | popovers and notification popups |
| `--gl-popup-paint-outset` | `2.5rem` | the space reserved around a notification popup |
| `--gl-font-family` | `"Adwaita Sans", system-ui, sans-serif` | the font on themed surfaces |
| `--gl-font-family-mono` | `"Adwaita Mono", monospace` | numbers that must line up and monospace row text |
| `--gl-text-caption` | `0.85rem` | small print |
| `--gl-text-body` | `1rem` | regular text |
| `--gl-text-title` | `1.2rem` | the large heading at the top of a popover, the title of what is playing |
| `--gl-text-display` | `2.4rem` | large numbers; the lock screen clock is 3.2 times this |

## Popovers

| Token | Default | Sets |
| --- | --- | --- |
| `--applet-popover-width` | `27rem` | the width of most popovers |
| `--applet-popover-width-wide` | `40rem` | the width of wide popovers |
| `--applet-popover-padding` | `0.8rem` | popover padding |
| `--applet-popover-bg` | `--gl-surface` | the popover body's background |
| `--applet-popover-fg` | `--gl-surface-fg` | popover text |
| `--applet-popover-radius` | `--gl-radius` | the popover body's corner radius |
| `--popover-shell-header-padding` | `0.55rem 0.8rem` | popover header padding |
| `--popover-body-spacing` | `0.35rem` | spacing in the popover body |

## Rows, meters and containers

| Token | Default | Sets |
| --- | --- | --- |
| `--row-height` | `2.85rem` | the minimum row height |
| `--row-padding` | `0 0.8rem` | row padding |
| `--row-radius` | `0.7rem` | row corner radius |
| `--row-gap` | `10px` | spacing inside rows |
| `--row-hover-bg` | `--gl-hover` | an interactive row's background under the pointer |
| `--row-active-bg` | `--gl-active` | an interactive row's background while pressed |
| `--row-tone-warning` | `--gl-warning-text` | warning row text and meter fills |
| `--row-tone-error` | `--gl-danger-text` | error row text and meter fills |
| `--row-tone-destructive` | `--gl-danger-text` | destructive row titles and meter fills |
| `--row-mono-font` | `--gl-font-family-mono` | the font of monospace rows |
| `--meter-row-height` | `0.3rem` | meter bar thickness |
| `--meter-row-trough` | `--gl-faint` | meter tracks |
| `--meter-row-fill` | `--gl-muted` | meter fills |
| `--container-radius` | `--row-radius` | container corner radius |
| `--container-red`, `--container-orange`, `--container-yellow`, `--container-green`, `--container-blue`, `--container-purple` | `--red-3`, `--orange-3`, `--yellow-3`, `--green-3`, `--blue-3`, `--purple-3` | container backgrounds and text of the corresponding color |
| `--container-light`, `--container-dark` | `--light-1`, `--dark-5` | light and dark container backgrounds and text |

## Per-widget tokens

These classes redeclare their own tokens. Set each token on the class shown, not in `:root`; a
`:root` setting has no effect.

| Set on | Token | Default | Sets |
| --- | --- | --- | --- |
| `.pager` | `--gl-pager-dot`, `--gl-pager-wide`, `--gl-pager-pill` | `0.5rem`, `1.3rem`, `1.8rem` | pager dot, wide item and label pill sizes |
| `.pager` | `--gl-pager-gap` | `0.18rem` | spacing around pager items |
| `.pager` | `--gl-pager-idle`, `--gl-pager-occupied`, `--gl-pager-here` | `--gl-panel-fg` at 25%, 50% and 90% | idle, occupied and current pager dots |
| `.pager` | `--gl-pager-text` | `--gl-panel-fg` at 55% | pager label text |
| `.calendar` | `--gl-calendar-control` | `2.4rem` | calendar header control size |
| `.calendar` | `--gl-calendar-cell` | `3rem` | day and month cell size |
| `.calendar` | `--gl-calendar-radius` | `0.6rem` | calendar control and cell corner radius |
| `.calendar` | `--gl-calendar-gap` | `0.2rem` | space above calendar event dots |
| `.notice` | `--gl-notice-radius` | `0.7rem` | notice corner radius |
| `.notification` | `--gl-notification-radius` | `1.1rem` | notification corner radius |

For example, reduce the notification corner radius with:

```css
.notification {
    --gl-notification-radius: 0.5rem;
}
```

## Motion and blur

| Token | Default | Sets |
| --- | --- | --- |
| `--gl-duration` | `150ms` | the length of every transition; set by `animation-speed`, not by CSS |
| `--gl-ease` | `cubic-bezier(0.2, 0, 0, 1)` | the easing of every transition |
| `--gl-popup-motion` | `0.75rem` | how far a notification popup slides in and out |
| `--gl-blur-opacity` | `60%` | how much of a blurred surface's color stays over the blur |
| `--gl-blur-radius` | `12px` | blur on or off; below `10px` there is no blur |

## Lock screen

Set these in `lock.css`, the file the lock screen reads.

| Token | Default | Paints |
| --- | --- | --- |
| `--gl-lock-fg` | white | text and icons |
| `--gl-lock-dim` | black | the layer that darkens the wallpaper |
| `--gl-lock-field` | white at 16% | the password field |
| `--gl-lock-glass-hover` | white at 18% | buttons under the pointer |
| `--gl-lock-shade` | black at 55% | the default text shadow color and the background of error and Caps Lock notes |
| `--gl-lock-text-shadow` | `0 1px 3px var(--gl-lock-shade)` | the shadow on lock stage text |
| `--gl-lock-error` | `#ffc4bf` | error text |
| `--gl-lock-warning` | `#ffda97` | warning text |

## Screenshot editor

| Token | Default | Paints |
| --- | --- | --- |
| `--gl-shot-red`, `--gl-shot-yellow`, `--gl-shot-green`, `--gl-shot-blue`, `--gl-shot-white` | red, yellow, green, blue, white | the color swatches in the toolbar |
| `--gl-shot-ink` | black | the mark drawn on the yellow and white swatches |
| `--gl-shot-ring` | black at 45% | the outline around a selected region |
| `--gl-scrim` | black at 55% | the dimmed area outside a selection and behind the size readout |
| `--gl-knob` | white | the selection outline, the size readout text, and the ring on today when it is selected in the calendar |
