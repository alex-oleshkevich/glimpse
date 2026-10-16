# More command buttons

Define each button in `config.toml`, then add its name to an existing panel zone. These recipes assume you've [added a command button](/applets/command/) before.

Each gesture is an argv list: the first string is the program and each remaining string is one argument. There is no implicit shell. Pipes, variable expansion, and shell operators require an explicit `sh -c`.

## Switch power profiles

With `powerprofilesctl` installed, check which profiles your machine provides:

```sh
powerprofilesctl list
```

If it lists `power-saver` and `balanced`, use left-click for power saving and right-click to return to balanced:

```toml
[applets.power-profile]
extends = "command"
icon = "power-profile-power-saver-symbolic"
tooltip-format = "Left: power saver. Right: balanced."
on-click = ["powerprofilesctl", "set", "power-saver"]
on-right-click = ["powerprofilesctl", "set", "balanced"]
```

The icon stays fixed; this button does not display the active profile.

## Hold dark mode and resume the configured policy

Use two gestures for the two operations:

```toml
[applets.dark-mode]
extends = "command"
icon = "weather-clear-night-symbolic"
tooltip-format = "Left: dark mode. Right: resume the configured policy."
on-click = ["glimpsectl", "settings", "mode", "dark"]
on-right-click = ["glimpsectl", "settings", "resume"]
```

The dark override lasts until you resume or the settings daemon shuts down. Resume follows your configured policy, which may still choose dark mode. See [the settings commands](/config/cli/glimpsectl#glimpsectl-settings).

## Pick a color into the clipboard

Install wl-clipboard, which provides `wl-copy`, alongside `glimpse-lens`. This recipe deliberately starts a shell because it needs a pipe:

```toml
[applets.pick-color]
extends = "command"
icon = "color-select-symbolic"
tooltip-format = "Pick a color"
on-click = ["sh", "-c", "glimpse-lens --tool picker | wl-copy"]
```

Click the button, then click a color on screen. The picker prints the color and `wl-copy` puts it on the clipboard. See [the color picker](/ecosystem/picker) for its controls.

## Open a terminal app

If you use Alacritty, launch `top` inside it:

```toml
[applets.processes]
extends = "command"
icon = "utilities-terminal-symbolic"
tooltip-format = "Open the process monitor"
on-click = ["alacritty", "-e", "top"]
```

Use your terminal's own command-execution arguments if you choose another terminal. To place all four buttons, append their names to a panel's existing zone list:

```toml
right = ["power-profile", "dark-mode", "pick-color", "processes", "network", "audio"]
```

Save the configuration to apply it. The [command settings reference](/config/applets/custom#command) lists all available gestures.
