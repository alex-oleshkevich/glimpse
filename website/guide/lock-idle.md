# Lock the screen and handle idle

Idle timers live under `[idle]`, the lock screen under `[lock]`. The full setting lists are in the
[idle](/config/services/idle) and [lock screen](/config/services/lock) references.

## Change the timeouts

Each listener runs a command after its `timeout` in seconds without keyboard or pointer input.
Writing `listeners` for a profile replaces that profile's defaults, so list every listener you want.
This blanks the screens after 5 minutes and locks after 10:

```toml
[[idle.profiles.ac.listeners]]
timeout = 300
on-idle = "glimpse-dpms off"
on-resume = "glimpse-dpms on"

[[idle.profiles.ac.listeners]]
timeout = 600
on-idle = "loginctl lock-session"
```

`glimpse-dpms` turns the screens off and on, and `on-resume` runs on the next input. The default AC
profile blanks at 15 minutes, locks at 30 and suspends at 60. Lock now from a key or a terminal with
`loginctl lock-session`.

## Use different timeouts on battery

The battery profile has the same shape under `[idle.profiles.battery]`. This blanks after 5 minutes
and suspends after 15 on battery:

```toml
[[idle.profiles.battery.listeners]]
timeout = 300
on-idle = "glimpse-dpms off"
on-resume = "glimpse-dpms on"

[[idle.profiles.battery.listeners]]
timeout = 900
on-idle = "systemctl suspend"
```

The battery profile applies while UPower reports the machine on battery; without UPower the AC
profile always applies. Switching between AC and battery, or changing the active listener set, runs
the `on-resume` commands of listeners that already ran. The replacement timers start from zero.

## Hold idle and suspend while presenting

```sh
glimpsectl idle inhibit
```

The hold blocks idle listeners and suspend. End it with Ctrl-C, or give it a limit up front, as in
`glimpsectl idle inhibit --for 2h`. See the [idle reference](/config/services/idle#hold-idle-and-suspend)
for duration formats and limits.

Without a command, `inhibit` prints the hold's id, and `glimpsectl idle release <id>` ends that hold
from another terminal. With `glimpsectl idle inhibit -- CMD...`, the hold lasts only while `CMD`
runs, and `glimpsectl` exits with the command's status.

Listeners that were held back run as soon as the last hold ends, if you haven't touched the keyboard
or pointer since. Video players and calls hold the timers off on their own while
`respect-inhibitors` is on, and `glimpsectl idle status` shows who is holding what.

## Change the lock screen background

Without images of its own, the lock screen uses your wallpaper so it matches the desktop. Give it
its own with `[lock.background]`:

```toml
[lock.background]
image = "/home/alex/Pictures/lock-day.jpg"
image-dark = "/home/alex/Pictures/lock-night.jpg"
blur-radius = 24
```

`image-dark` shows while the desktop is dark, and `blur-radius` softens the picture. Adjust `fit`
to change how the picture fills the screen, `dim` to darken it, and `dim-dark` for its darkness in
the dark scheme. The [background settings](/config/services/lock#lock-background) list the choices
and limits.

## Choose what the lock screen shows

The lock screen always shows the clock, the battery level, notifications, the current track and
the session button, plus the active keyboard layout when you have two or more. Only notifications
that arrive while the screen is locked appear, as one icon with a count for each app; the app's
name is the icon's tooltip and accessible name. No summary or body is shown.

The media area shows the track title and artist to anyone, and its pause and next buttons work
without a password.

The session menu offers `Suspend...`, `Restart...`, and `Power Off...`, and every action asks for
confirmation. Configuration edits apply live while the screen is locked.

## Let the compositor accept a restarted lock screen

If the lock screen crashes or restarts while the screen is locked, the compositor refuses the new lock unless `misc:allow_session_lock_restore` is on, and the session stays stuck on its lock-dead screen. In the hyprlang config, set `misc { allow_session_lock_restore = true }`. In the Lua config, set `hl.config({ misc = { allow_session_lock_restore = true } })`.
