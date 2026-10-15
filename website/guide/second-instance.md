# Show a world clock next to your clock

A second clock is a new applet that extends the built-in one with its own timezone.

## Add the second clock

Define an applet under `[applets.<name>]` with `extends = "clock"` and the IANA zone it should read,
then put its name in a zone:

```toml
[applets.tokyo]
extends = "clock"
timezone = "Asia/Tokyo"

[[panels]]
right = [
    "tokyo", "next-event", "tray", "removable", "clipboard", "color-picker", "printing", "bluetooth",
    "network", "display", "audio", "idle", "power", "keyboard", "session",
]
```

This is the default right zone with `"tokyo"` added at the front. Writing a zone replaces its whole
default list, so keep the names you want to stay on the bar.

`timezone` takes an IANA name such as `UTC` or `Asia/Tokyo`; unset is the local zone. The instance
takes every other clock setting too, so `label-format` can give it a different format.

## Add cities to the clock popover

The clock's popover has a world clock section fed by the `timezones` list. Each entry needs a
`label` and a `timezone`; `icon` and `note` are optional:

```toml
[applets.clock]
timezones = [
    { label = "Tokyo", timezone = "Asia/Tokyo" },
    { label = "Denver", timezone = "America/Denver", note = "the office" },
]
```

A row works out for itself whether the time it shows is today, tomorrow or yesterday, and the `note`
sits after that. An empty list hides the section.

Every clock setting is in the [information applets reference](/config/applets/information#clock).
