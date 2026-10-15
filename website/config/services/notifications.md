# Notifications

`[notifications]` controls the popups apps send and the history kept behind them. Do not disturb is
not a setting here; toggle it from the notifications applet or with `glimpsectl notifications dnd`.

<ConfigTable path="notifications" />

To hide notifications from one app entirely, match it in `suppress`. The pattern is a regular
expression tested against the app's id, its name, the title and the body:

```toml
[notifications]
suppress = ["^Spotify$", "Software Updates"]
```

The [notifications applet](/config/applets/information#notifications) has its own table for how the
panel shows unread notifications.
