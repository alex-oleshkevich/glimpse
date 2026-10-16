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
    "tokyo", "next-event", "tray", "removable", "clipboard", "printing", "bluetooth",
    "network", "display", "audio", "idle", "power", "keyboard", "session",
]
```

This is the default right zone with `"tokyo"` added at the front. Writing a zone replaces its whole
default list, so keep the names you want to stay on the bar.

`timezone` takes an IANA name such as `UTC` or `Asia/Tokyo`; unset is the local zone. A name glimpse
doesn't know shows your local time, so check the spelling (`Asia/Tokyo`, not `Tokyo`). The instance
takes every other clock setting too, so `label-format` can give it a different format. It starts
from the built-in defaults and does not read `[applets.clock]`, so set what you want on the instance
itself.

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

A row says `Tomorrow` or `Yesterday` when its day is not yours and shows no word on the same day.
The `note` follows that word, or stands alone. An empty list hides the section.

Every clock setting is in the [information applets reference](/config/applets/information#clock).
