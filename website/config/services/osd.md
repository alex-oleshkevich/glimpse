# On-screen display

`[osd]` controls the short-lived, click-through display of volume, brightness and keyboard layout
changes. It appears on the monitor set by [`[notifications] monitor`](/config/services/notifications)
and keeps no history.

<ConfigTable path="osd" />

These are the defaults:

```toml
[osd]
enabled = true
edge = "bottom-center"
duration = 1500
```

Set `enabled` to `false` to hide the OSD. The `edge` is one of `top-left`, `top-center`, `top-right`,
`bottom-left`, `bottom-center` or `bottom-right`. The `duration` is the number of milliseconds it
stays after the last change, from 500 to 10000. Each change replaces the visible content and restarts
that duration.

The panel shows the OSD for volume and mute changes on the default output and input, default audio
device and port changes, display and keyboard backlight changes, and keyboard layout switches.
It keeps volume and brightness feedback quiet while its own audio or display popover is open.
Per-window keyboard layout restores do not show an OSD. The OSD never appears while the session is
locked.

Volume, brightness and layout keybinds need no OSD command: the panel shows the change. Use
`glimpsectl osd show --icon <icon> --label <label> --value <percent>` for custom cases such as Caps
Lock. Each option is optional; send an icon, a label, a value or a mix of them. The value is a percent
from 0 to 200.

```sh
glimpsectl osd show --icon input-keyboard-symbolic --label "Caps Lock on"
glimpsectl osd show --icon audio-volume-overamplified-symbolic --value 130
glimpsectl osd hide
```

`glimpsectl osd hide` hides the visible OSD immediately.
