# Debug a running applet

Use `glimpsectl applet` to inspect installed exec applets and their running systemd user units. For an applet still in development, start with the terminal that ran `applets run`.

## Check discovery and placement

```sh
glimpsectl applet list --installed
glimpsectl applet inspect --installed me.aresa.GlimpseTodo
```

The list includes installed and placed applets, their desktop entries, commands, and state. Inspect separates desktop metadata, configuration placements and options, and runtime units. A refused desktop entry includes its reason. `runtime: unavailable` means the CLI could not obtain runtime information; it does not establish that the applet is stopped.

## Read installed applet logs

Follow logs for applet units:

```sh
journalctl --user -u 'app-glimpse-*.service' -f
```

To narrow the output, copy a unit name from `applets inspect` and pass that exact name to `-u`. Units use the form `app-glimpse-<panel>-<applet>-<slot>-<pid>-<n>.service`.

The SDK redirects console methods to stderr. Installed applet stderr goes to the journal; an applet attached through `glimpsectl applet run` sends stderr to that command's terminal. Build errors from the generated start script appear there too.

## Restart after a fix

Restart all running units for an installed desktop id:

```sh
glimpsectl applet restart me.aresa.GlimpseTodo
```

This command requires running units for that id. During a [development run](/applets/exec/live-reload), save the source or type `r` and Enter in the development terminal instead. See [the CLI reference](/config/cli/glimpsectl#glimpsectl-applets) for command options.
