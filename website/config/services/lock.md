# Lock screen

`[lock]` and its subtables set what the lock screen shows.

The screen locks when you ask it to, for example with `loginctl lock-session` or from the session
menu, and before the computer sleeps. If its password checks fail, it refuses to lock and warns
you instead. To lock after a period of inactivity, add a listener to [`[idle]`](./idle).

## Lock screen

<ConfigTable path="lock" />

<ConfigTable path="lock.background" describe />

Without `image` or `image-dark` here, the lock screen uses the wallpaper's images, so it matches
your desktop. The background is a solid `color` while the image decodes, so the picture can appear
shortly after the lock screen.

The selected theme's `lock.css` styles the lock screen. See [Write a theme](/guide/theming).

<ConfigTable path="lock.clock" describe />

`glimpsectl config validate` does not check the clock formats, background colors, image paths, or
`pam-service`. An invalid `time-format` hides the time. Only `{day}` is replaced in `date-format`;
an unknown placeholder is printed as typed.

<ConfigTable path="lock.status" describe />

Turning off this table hides the status island but leaves the session button.

<ConfigTable path="lock.notifications" describe />

Only notifications that arrive after the lock starts appear. `"apps"` shows one icon with a count
for each app; the app's name is the icon's tooltip and accessible name. Apps without an icon use a
generic bell, and groups are ordered critical first, then newest. `"count"` shows one icon with the
total. Neither mode shows a summary or a body.

<ConfigTable path="lock.media" describe />

The track title and artist are visible to anyone. The pause and next buttons work without a
password; turn this table off to hide both the information and the controls.

<ConfigTable path="lock.session" describe />

Every session action asks for confirmation. Logind decides whether it is available: `yes` shows the
action, `no`, `na`, and `challenge` hide it, and `inhibited`, `inhibitor-blocked`, or
`challenge-inhibitor-blocked` shows it disabled with the blocker's reason when available.

::: warning PAM
The lock screen checks your password through the PAM service named by `pam-service`, which it reads
once when it starts. The packages install `/etc/pam.d/glimpse-lock`. Give the bare file name, not a
path. Point `pam-service` somewhere else only if that file exists in `/etc/pam.d`. If it is missing
or unsafe, the lock screen refuses to lock and posts a "Screen not locked" notification.
`glimpse-lock check` tells you what is wrong. If the session is already locked when the daemon
starts, it re-acquires the lock even if the checks fail and shows instructions for using a text
console.
:::
