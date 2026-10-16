# Ecosystem

Besides the shell, glimpse comes with tools you start when you need them, from a key, a panel
button or a terminal.

| Tool | What it does | Start it with |
| --- | --- | --- |
| [Screenshots](/ecosystem/shot) | Takes a screenshot and lets you mark it up before you copy or save it | `glimpse-shot` |
| [Color picker](/ecosystem/picker) | Prints the color of any pixel on screen | `glimpse-lens --tool picker` |
| [Ruler](/ecosystem/ruler) | Measures distances on screen in pixels | `glimpse-lens --tool ruler` |
| [Magnifier](/ecosystem/lens) | Freezes the screen and zooms in under the pointer | `glimpse-lens` |

The screenshot tool, the color picker and the ruler print plain text or JSON and report what
happened in their exit code, so scripts and AI agents can drive them too.

## Bind them to keys

glimpse sets no key bindings. Add them in your compositor's configuration:

| Key | Command |
| --- | --- |
| <kbd>Print</kbd> | `glimpse-shot --screen` |
| <kbd>Ctrl</kbd>+<kbd>Print</kbd> | `glimpse-shot --window` |
| <kbd>Shift</kbd>+<kbd>Print</kbd> | `glimpse-shot capture --copy --no-save` |
| <kbd>Super</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> | `sh -c "glimpse-lens --tool picker \| wl-copy"` |

[Bind glimpse to keys](/guide/keys) shows the full syntax and more commands worth binding.
