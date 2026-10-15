# Lock screen

`[lock]` and its subtables set what the lock screen shows.

The screen locks when you ask it to, for example with `loginctl lock-session` or from the session
menu, and always before the computer sleeps, so it wakes up locked. To lock after a period of
inactivity, add a listener to [`[idle]`](./idle).

## Lock screen

<ConfigTable path="lock" />

<ConfigTable path="lock.background" describe />

Without `image` or `image-dark` here, the lock screen uses the wallpaper's images, so it matches
your desktop.

<ConfigTable path="lock.clock" describe />

<ConfigTable path="lock.status" describe />

<ConfigTable path="lock.notifications" describe />

<ConfigTable path="lock.media" describe />

<ConfigTable path="lock.session" describe />

::: warning PAM
The lock screen checks your password through the PAM service named by `pam-service`. The packages
install `/etc/pam.d/glimpse-lock`. Point `pam-service` somewhere else only if that file exists,
or no password will unlock the screen. `glimpse-lock check` tests the setup without locking.
:::
