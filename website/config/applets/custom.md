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
tooltip-format = "Take a screenshot"
on-click = ["glimpse-shot"]

[[panels]]
right = ["screenshot", "network", "audio"]
```

Give every command applet its own name and `extends = "command"`.

<ConfigTable path="applets.command" describe />

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

