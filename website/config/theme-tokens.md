# Theme tokens

Tokens are the CSS custom properties a theme redefines in `:root`. Every rule in the built-in
stylesheet reads them, so changing a token changes every part that uses it. Where a default names a
GTK color, glimpse follows your GTK theme until you set the token. How to write a theme is in
[Write a theme](/guide/theming).

## Surfaces and text

| Token | Default | Paints |
| --- | --- | --- |
| `--gl-panel` | GTK's header bar color | the bar's background |
| `--gl-panel-fg` | GTK's window text color | text and icons on the bar |
| `--gl-surface` | GTK's popover color | popovers and other floating cards |
| `--gl-surface-fg` | GTK's popover text color | text on popovers |
| `--gl-notification` | `--gl-surface` mixed 8% toward `--gl-surface-fg` | notification cards |
| `--gl-notification-fg` | `--gl-surface-fg` | text on notification cards |
| `--gl-muted` | `--gl-surface-fg` at 65% | secondary text |
| `--gl-dim` | `--gl-surface-fg` at 38% | less important text |
| `--gl-faint` | `--gl-surface-fg` at 22% | the faintest text, empty meter tracks |
| `--gl-border` | GTK's border color | borders and separators |
| `--gl-hover` | `--gl-surface-fg` at 8% | the background under the pointer |
| `--gl-active` | `--gl-surface-fg` at 16% | the background of a pressed or open item |
| `--gl-control` | `--gl-border`, stronger | today's date in the calendar |
| `--gl-disabled` | GTK's disabled opacity, 0.5 | the opacity of a control you can't use |

`--gl-muted`, `--gl-dim` and `--gl-faint` come out a little lighter in light mode than in dark,
the same as dimmed text in GTK apps.

## Accent and status

| Token | Default | Paints |
| --- | --- | --- |
| `--gl-accent` | GTK's accent color | the current workspace, today and the selected day in the calendar, confirm buttons, progress bars |
| `--gl-accent-fg` | GTK's text-on-accent color | text drawn on `--gl-accent` |
| `--gl-accent-text` | GTK's accent text color | accent-colored text, focus outlines, attention dots |
| `--gl-accent-soft` | `--gl-accent` at 18% | badges, highlighted groups, the chosen screenshot tool |
| `--gl-danger-text` | GTK's destructive color | error text |
| `--gl-danger-soft` | GTK's destructive background at 18% | the background behind errors |
| `--gl-warning-text` | GTK's warning color | warning text |
| `--gl-warning-soft` | GTK's warning background at 18% | the background behind warnings |

## Shape, depth and type

| Token | Default | Sets |
| --- | --- | --- |
| `--gl-radius` | `0.95rem` | the corner radius of popovers, sheets and the screenshot editor's panels |
| `--gl-notification-radius` | `1.1rem` | the corner radius of notifications |
| `--gl-shadow` | black at 55% | the shadow color the elevations use |
| `--gl-elevation-raised` | a small, soft shadow | the bar and notifications in a list |
| `--gl-elevation-floating` | a larger shadow | popovers, notification popups, the picker and ruler readouts |
| `--gl-font-family` | `"Adwaita Sans", system-ui, sans-serif` | the font everywhere |
| `--gl-font-family-mono` | `"Adwaita Mono", monospace` | numbers that must line up, like color values and measurements |
| `--gl-text-caption` | `0.85rem` | small print |
| `--gl-text-body` | `1rem` | regular text |
| `--gl-text-title` | `1.2rem` | the large heading at the top of a popover, the title of what is playing |
| `--gl-text-display` | `2.4rem` | large numbers; the lock screen clock is 3.2 times this |
| `--applet-popover-width` | `27rem` | the width of most popovers |
| `--applet-popover-width-wide` | `40rem` | the width of wide popovers |

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
| `--gl-lock-shade` | black at 55% | text shadows and the background of error and Caps Lock notes |
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
