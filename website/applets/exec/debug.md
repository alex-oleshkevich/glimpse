# Debug a running applet

Use `glimpsectl applet` to inspect installed exec applets and their running systemd user units. For an applet still in development, start with the terminal that ran `applet run`.

## Check discovery and placement

```sh
glimpsectl applet list --installed
glimpsectl applet inspect --installed me.aresa.GlimpseTodo
```

The list includes installed and placed applets, their desktop entries, commands, and state. Inspect separates desktop metadata, configuration placements and options, and runtime units. A refused desktop entry includes its reason. `runtime: unavailable` means the CLI could not obtain runtime information. Ignore the state column then: its `stopped` values do not confirm that an applet has stopped.

From your project directory, run `glimpsectl applet run --dev .`. It loads the desktop entry and local configuration, and it reports an entry the host refuses.

## Read installed applet logs

Follow logs for applet units:

```sh
journalctl --user -u 'app-glimpse-*.service' -f
```

To narrow the output, copy a unit name from `applet inspect` and pass that exact name to `-u`. Units use the form `app-glimpse-<panel>-<applet>-<slot>-<pid>-<n>.service`.

The SDK redirects console methods to stderr. Installed applet stderr goes to the journal; an applet attached through `glimpsectl applet run` sends stderr to that command's terminal. Build errors from the generated start script appear there too.

If the panel refuses a tree, it kills the applet unit without printing the reason in the development terminal. The reason appears in the panel log. Look there for a rejected tree, such as `section cannot hold section`. Limit errors caught by the SDK and exceptions thrown in event handlers print to stderr and end the process.

## Restart after a fix

Restart all running units for an installed desktop id:

```sh
glimpsectl applet restart me.aresa.GlimpseTodo
```

This command requires running units for that id. During a [development run](/applets/exec/live-reload), save the source or type `r` and Enter in the development terminal instead. Type `t` and Enter to print the current tree as the panel holds it. See [the CLI reference](/config/cli/glimpsectl#glimpsectl-applet) for command options.
