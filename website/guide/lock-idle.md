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
profile always applies.

## Keep the screen awake while presenting

```sh
glimpsectl idle inhibit
```

The hold lasts while the command runs: end it with Ctrl-C, or give it a limit up front, as in
`glimpsectl idle inhibit --for 2h`. Video players and calls hold the timers off on their own while
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

`image-dark` shows while the desktop is dark, `blur-radius` softens the picture (0 to 128, 0 by
default) and `dim` darkens it.

## Choose what the lock screen shows

The clock, the status island (weather, battery, keyboard layout, Bluetooth and network), pending
notifications, media controls and the session buttons each have a table with an `enabled` switch:

```toml
[lock.notifications]
enabled = false

[lock.media]
enabled = false
```

Notification text is never shown on the lock screen; `[lock.notifications] privacy` chooses between
naming the sending apps (`"apps"`, the default) and showing only a count (`"count"`).
