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

`glimpsectl config validate` does not check the date format, background colors or image paths. The
time follows your system's 12- or 24-hour clock (`LC_TIME`). `date-format` is a strftime pattern.

The lock screen always shows the clock, notifications, the current track and the session button.
Only notifications that arrive after the lock starts appear, as one icon with a count for each app;
the app's name is the icon's tooltip and accessible name. Apps without an icon use a generic bell,
and groups are ordered critical first, then newest. No summary or body is shown. The track title
and artist are visible to anyone, and the pause and next buttons work without a password. The
session menu offers suspend, restart and power off, and every action asks for confirmation. Logind
decides whether each is available: `yes` shows the action, `no`, `na`, and `challenge` hide it, and
`inhibited`, `inhibitor-blocked`, or `challenge-inhibitor-blocked` shows it disabled with the
blocker's reason when available.

::: warning PAM
The lock screen checks your password through `/etc/pam.d/glimpse-lock`, which the packages install.
If it is missing or unsafe, the lock screen refuses to lock and posts a "Screen not locked"
notification.
`glimpse-lock check` tells you what is wrong. If the session is already locked when the daemon
starts, it re-acquires the lock even if the checks fail and shows instructions for using a text
console.
:::
