# What an applet can do

An exec applet runs as you, with no sandbox. Installing one is the same trust decision as installing
any program. The panel protects the bar from a broken applet, not your files from a hostile one.

## What it can reach

Everything you can: your files, the network, the session bus, any program on `PATH`. The unit gets
the panel's environment plus the `env` from its config table, and a 256 MiB memory limit. Its state
directory is mode 0700, but nothing stops it writing elsewhere.

## What goes through the panel

Some requests travel over the socket instead of being done by the applet itself:

| Request | Why the panel does it |
| --- | --- |
| `open-uri`, `launch` | The panel holds the activation token, so the opened window gets focus. The new app also starts outside the applet's 256 MiB unit, so it does not die with the applet. |
| `close-popover` | The popover belongs to the panel. |

The panel opens only `http`, `https`, `mailto` and `file` URIs of at most 2,048 bytes, and refuses a
file that would run a program. Notifications and clipboard copies need nothing from the panel: the
SDKs run `notify-send` and `wl-copy` from the applet process.

## What the panel enforces

Slowed down, applet left running:

- More than 120 lines a second: the panel stops reading until the next second.
- A message with an unknown type is ignored, so a newer SDK works with an older panel.

The unit is killed, then restarted by systemd (a development copy stays down):

- A line that is not valid JSON, or longer than 1 MiB.
- A `hello` with an unsupported version, or a `commit` or request sent before `hello`.
- A `commit` the panel rejects: see [the limits](/developers/protocol#caps).
- More than 5 `hello`s within 10 seconds.
- An applet that stops reading its socket until the panel's queue to it is full.

Any of these affects only that applet. The rest of the bar carries on.

## Text from other apps

Titles and names from other programs (windows, notifications, media players, networks) are hostile
input. The panel cleans and caps every string an applet sends, renders it as plain text, never as
markup, and drops icon names with unexpected characters.
