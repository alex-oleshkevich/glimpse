# Manage notifications

Where popups appear, how long they stay, and which apps stay quiet. The full setting list is in the
[notifications reference](/config/services/notifications).

## Do not disturb

Toggle do not disturb from the notifications applet, or from a command you can bind to a key:

```sh
glimpsectl notifications dnd on
glimpsectl notifications dnd on --until 18:00
glimpsectl notifications dnd off
```

Without `--until` it stands until you turn it off. Popups are held back while the history keeps the
notifications for you.

## Keep more history

The history holds the last 100 read or dismissed notifications. Raise or lower it with `keep` (1 to
1000):

```toml
[notifications]
keep = 200
```

## Choose where popups appear and how long they stay

```toml
[notifications]
edge = "bottom-right"
hide-delay = 8
```

`edge` takes `top-left`, `top-center`, `top-right`, `bottom-left`, `bottom-center` or
`bottom-right`. `hide-delay` is the seconds a popup stays (4 by default); hovering a popup pauses
the countdown. `max-items` caps how many popups show at once, and `monitor` pins popups to one
monitor instead of the focused one.

## Silence one app

`suppress` is a list of regular expressions matched against the app id, the app name and each
notification's title and body. A match is neither shown nor stored:

```toml
[notifications]
suppress = ["^Spotify$", "Software Updates"]
```
