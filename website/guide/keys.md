# Bind glimpse to keys

glimpse's tools are ordinary commands, so any key your compositor can bind can drive them.

| Command | What it does |
| --- | --- |
| `loginctl lock-session` | Lock the screen now |
| `glimpse-shot` | Open the screenshot app |
| `glimpse-shot --region` | Drag a rectangle, then mark it up |
| `glimpse-shot capture --copy --no-save` | Copy the focused output, no file, no window |
| `glimpse-lens --tool picker \| wl-copy` | Pick a color and copy its hex value |
| `glimpsectl panel popover toggle eDP-1 clock` | Open or close the clock's calendar popover |
| `glimpsectl notifications dnd on` | Silence notifications until you turn it off |
| `glimpsectl notifications dnd on --until 18:00` | Silence them until a local time, today or tomorrow |
| `glimpsectl notifications dnd off` | Let notifications through again |
| `glimpsectl settings mode dark` | Hold the desktop in dark mode |
| `glimpsectl settings resume` | Follow the configured light/dark policy again |
| `glimpsectl sunset pause` | Keep daylight color until the current night ends |

`panel popover toggle` takes the output connector and the applet name; `glimpsectl panel applets`
lists both. Every command and flag is in the command reference: [glimpsectl](/config/cli/glimpsectl),
[glimpse-shot](/config/cli/glimpse-shot), [glimpse-lens](/config/cli/glimpse-lens).

A binding in your compositor's configuration pairs a key with one of the commands above (the syntax
is your compositor's):

```kdl
// ~/.config/niri/config.kdl
binds {
    Mod+Shift+X { spawn "loginctl" "lock-session"; }
    Print { spawn "glimpse-shot" "--region"; }
    Mod+Shift+C { spawn "sh" "-c" "glimpse-lens --tool picker | wl-copy"; }
    Mod+Shift+D { spawn "glimpsectl" "panel" "popover" "toggle" "eDP-1" "clock"; }
}
```
