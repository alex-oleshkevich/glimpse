# Bind glimpse to keys

glimpse's tools are ordinary commands, so any key your compositor can bind can drive them.

| Command | What it does |
| --- | --- |
| `loginctl lock-session` | Lock the screen now |
| `glimpse-shot --screen` | Freeze the screen and mark it up |
| `glimpse-shot --window` | Click a window, then mark it up |
| `glimpse-shot --region` | Drag a rectangle (Space takes the whole screen), then mark it up |
| `glimpse-shot capture --copy --no-save` | Copy the focused output, no file, no window |
| `glimpse-lens --tool picker \| wl-copy -n` | Pick a color and copy its hex value |
| `glimpsectl applet toggle clock` | Open or close the clock's calendar popover |
| `glimpsectl notifications dnd on` | Silence notifications until you turn it off |
| `glimpsectl notifications dnd on --until 18:00` | Silence them until a local time, today or tomorrow |
| `glimpsectl notifications dnd off` | Let notifications through again |
| `glimpsectl appearance set dark` | Hold the desktop in dark mode |
| `glimpsectl appearance resume` | Follow the configured light/dark policy again |
| `glimpsectl night-light pause` | Keep daylight color until the current night ends |

`applet toggle` takes the applet name; with a bar on several monitors, add `--output` and the
connector. With two bars on one monitor, add `--edge` too (`top`, `bottom`, `left` or `right`).
glimpsectl refuses an ambiguous name. `glimpsectl applet list` lists the names, outputs and edges.
Every command and flag is in the command reference: [glimpsectl](/config/cli/glimpsectl),
[glimpse-shot](/config/cli/glimpse-shot), [glimpse-lens](/config/cli/glimpse-lens).

A binding in your compositor's configuration pairs a key with one of the commands above (the syntax
is your compositor's):

```kdl
// ~/.config/niri/config.kdl
binds {
    Mod+Shift+X { spawn "loginctl" "lock-session"; }
    Print { spawn "glimpse-shot" "--screen"; }
    Mod+Shift+C { spawn "sh" "-c" "glimpse-lens --tool picker | wl-copy -n"; }
    Mod+Shift+D { spawn "glimpsectl" "applet" "toggle" "clock"; }
}
```
