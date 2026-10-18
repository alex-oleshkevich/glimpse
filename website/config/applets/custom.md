# Command and exec applets

These are the two kinds of applet you define yourself.

## command

A command applet is a chip with an icon and a label that runs a program when you click or scroll on
it. Each gesture takes a program and its arguments as a list. No shell is involved, so an argument
with spaces is one list item and nothing needs quoting.

```toml
[applets.screenshot]
extends = "command"
icon = "camera-photo-symbolic"
tooltip-format = "Select a screenshot region"
on-click = ["glimpse-shot", "--region"]

[[panels]]
right = ["screenshot", "network", "audio"]
```

Give every command applet its own name and `extends = "command"`.

For pipes, `&&` or `$VARIABLES`, run the shell yourself with `["sh", "-c", "<command line>"]`.
`env` sets variables for every program the chip runs:

```toml
[applets.volume-up]
extends = "command"
icon = "audio-volume-high-symbolic"
env = { PULSE_SINK = "@DEFAULT_SINK@" }
on-click = ["sh", "-c", "pactl set-sink-volume $PULSE_SINK +5% && notify-send Louder"]
```

<ConfigTable path="applets.command" describe />

Left and middle click each run their own command, as do the four scroll directions. Each wheel
notch runs once; other mouse buttons do nothing.

An absolute SVG icon path works. If the file is missing, the chip uses `system-run-symbolic`.

## exec

An exec applet is a program installed with a desktop entry that declares
`Implements=me.aresa.Glimpse.Applet1`. Place it on a panel by its desktop-file id. A table named by
the same id passes it options:

```toml
[[panels]]
right = ["me.aresa.GlimpseTodo", "network", "audio"]

[applets."me.aresa.GlimpseTodo"]
options = { title = "Groceries" }
```

To run a second copy with other options, give it a name, `extends = "exec"`, and the id in
`applet`:

```toml
[applets.work-todo]
extends = "exec"
applet = "me.aresa.GlimpseTodo"
options = { title = "Work" }
```

<ConfigTable path="applets.exec" describe />

Exec applets need the systemd user manager. If an id is not installed or the applet cannot start,
no chip appears. Check the panel log for the reason; `glimpsectl applet list --installed` lists
the installed applets.
