# Configure an applet in the panel

Place an installed applet on a bar, pass it options and environment variables, and run it twice
with different settings. Every key is in the [exec settings reference](/config/applets/custom#exec).

## Place it on a bar

Add the applet's desktop-file id to a zone of a panel in `~/.config/glimpse/panel.toml`:

```toml
[[panels]]
right = ["me.aresa.GlimpseTodo", "network", "audio"]
```

Writing a zone replaces its default list, so keep the names you want to stay on the bar. Save the
file and the panel starts the applet; there is nothing to restart.

## Pass options

A table named by the same id holds the options. The applet receives them as a JSON object:

```toml
[applets."me.aresa.GlimpseTodo"]
options = { title = "Groceries" }
```

Changing `options` restarts the applet with the new values. The applet reads them with
`useOptions()` in TypeScript, `ctx.options` in Python and Rust, and `ctx.Options` in Go.

## Set environment variables

```toml
[applets."me.aresa.GlimpseTodo"]
env = { TODO_FILE = "/home/me/notes/todo.txt" }
```

`env` adds to the panel's environment. Changing it restarts the applet.

## Run it twice with different settings

Give the second copy its own name, `extends = "exec"`, and the id in `applet`:

```toml
[[panels]]
right = ["me.aresa.GlimpseTodo", "work-todo", "network", "audio"]

[applets."me.aresa.GlimpseTodo"]
options = { title = "Groceries" }

[applets.work-todo]
extends = "exec"
applet = "me.aresa.GlimpseTodo"
options = { title = "Work" }
```

Each name runs as its own process with its own options. Both share one state directory, so an
applet that saves data keys it by its instance name when two copies must not mix.

The same name on two bars runs once and both bars show it, as long as it sits in the same zone of
bars on the same edge with the same size. See [How an applet runs](/developers/exec-applets#one-process-several-bars).

## Check what is placed

```sh
glimpsectl applet list
glimpsectl applet inspect me.aresa.GlimpseTodo
```

If no chip appears, see [Debug a running applet](/applets/exec/debug).
