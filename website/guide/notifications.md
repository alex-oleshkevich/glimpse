# Manage notifications

Where popups appear, how long they stay, and which apps stay quiet. The full setting list is in the
[notifications reference](/config/services/notifications).

## Do not disturb

Open the notifications applet and turn off the switch beside its title. The bell turns into a
crossed-out bell and stays that way until you turn the switch back on. For an end time, or for a
key binding, use the command:

```sh
glimpsectl notifications dnd on
glimpsectl notifications dnd on --until 18:00
glimpsectl notifications dnd off
```

Without `--until` it stands until you turn it off. `--until` takes a local time as `HH:MM`; a time
that has already passed today means tomorrow. While do not disturb is on, popups are not shown and
are not replayed afterwards, but every notification still lands in the bell's list. Popups are held
back the same way while the screen is locked or shared.

Do not disturb and the history live in memory. Logging out, or restarting the notifications
service, turns do not disturb off and empties the history.

## Turn popups off

To stop popups without losing notifications, set `enabled = false`. The bell still counts them and
you read them in the applet:

```toml
[notifications]
enabled = false
```

## Keep more history

The history keeps the last 100 notifications you have read or dismissed. Unread ones are never
dropped. Change the limit with `keep`; a value below 1 counts as 1 and one above 1000 as 1000:

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
`bottom-right`. `hide-delay` is the seconds a popup stays (4 by default) when the app doesn't ask
for a time of its own; hovering a popup pauses the countdown. An app that asks for a time gets that
time, and a critical notification stays until you close it.

`max-items` caps how many popups show at once (6 by default). When more arrive, the oldest popups
close; they stay unread in the bell's list. `monitor` pins popups to one monitor instead of the
focused one.

## Silence one app

`suppress` is a list of regular expressions matched against the app id, the app name and each
notification's title and body. A match is neither shown nor stored:

```toml
[notifications]
suppress = ["^Spotify$", "Software Updates"]
```

Matching is case sensitive, so `^Spotify$` does not match `spotify`. Saving a new pattern also
removes matching notifications you already have. A pattern that is not a valid regular expression
is skipped with a warning in the log; `glimpsectl config validate` does not catch it.

## Clear notifications from a script

`glimpsectl notifications dismiss <id>` marks one notification read and keeps it in the history.
`glimpsectl notifications clear` deletes every stored notification, read or not, except one that is
still waiting for you to pick one of its actions. `clear --app` limits it to one app, by the
`app_id` that `glimpsectl notifications list --json` shows. An app without a desktop file has the
id `name:` followed by its name, for example `name:Fractal`.
