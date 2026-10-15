# Ecosystem

Besides the shell itself, glimpse comes with two tools you start when you need them: **glimpse-shot**
for screenshots and **glimpse-lens** for looking closely at the screen. They don't run in the
background. Bind them to keys, put them on a panel button, or run them from a terminal.

## glimpse-shot: screenshots

glimpse-shot takes a screenshot and lets you mark it up before you save or copy it: arrows,
boxes, numbered steps, text, and blur over anything private.

<ThemeImage
  light="/screenshots/ecosystem/shot-editor.png"
  dark="/screenshots/ecosystem/shot-editor.png"
  alt="A Files window frozen in the glimpse-shot editor, with an oval, an arrow and a note pointing at a file, two numbered markers, blurred folder names, and the tool bar along the top"
/>

### Take a screenshot

| Run | To |
| --- | --- |
| `glimpse-shot --screen` | freeze the whole screen and mark it up |
| `glimpse-shot --region` | drag a rectangle, then mark it up |
| `glimpse-shot --window` | pick a window, then mark it up |
| `glimpse-shot --screen --delay 5` | wait five seconds first, for menus and tooltips |
| `glimpse-shot` | open the screenshot app and choose what to capture |

The screen freezes the moment you run it, so whatever was on screen stays put while you work.

### Mark it up

Pick a tool from the bar at the top, or press its letter:

| Key | Tool |
| --- | --- |
| <kbd>a</kbd> | arrow |
| <kbd>r</kbd> | rectangle, <kbd>f</kbd> for a filled one |
| <kbd>o</kbd> | oval |
| <kbd>n</kbd> | numbered step: each click places the next number |
| <kbd>t</kbd> | text |
| <kbd>b</kbd> | blur, for passwords, names and anything private |
| <kbd>h</kbd> | highlighter |
| <kbd>p</kbd> | freehand pen |
| <kbd>c</kbd> | crop |

<kbd>1</kbd> to <kbd>5</kbd> change the color, <kbd>[</kbd> and <kbd>]</kbd> the thickness.
<kbd>Ctrl</kbd>+<kbd>Z</kbd> undoes the last mark.

### Save or copy

| Press | To |
| --- | --- |
| <kbd>Enter</kbd> | copy the picture and close |
| <kbd>Shift</kbd>+<kbd>Enter</kbd> | save it to your screenshots folder and close |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd> | save it somewhere else |
| <kbd>Ctrl</kbd>+<kbd>E</kbd> | keep working on it in a normal window |
| <kbd>Esc</kbd> | close without saving |

Screenshots go to the Screenshots folder inside your Pictures folder unless you choose another folder in the app's
preferences or in [`[screenshot]`](/config/services/screenshots). A notification lets you open the
picture or show it in your file manager.

### Screenshots from other apps

Apps that ask the system for a screenshot, such as a browser's screen recorder or a chat app's
"share screenshot" button, get the same editor. You see what is being asked for, mark it up if you
like, and press <kbd>Enter</kbd> to share it or <kbd>Esc</kbd> to refuse.

### Without the editor

`glimpse-shot capture` takes the screenshot straight away and shows nothing, which suits key
bindings and scripts:

```sh
# Copy the whole screen to the clipboard, no file
glimpse-shot capture --copy --no-save

# Save the focused window to a file
glimpse-shot capture --window focused -o window.png
```

The [command reference](/config/cli/glimpse-shot) lists every option.

## glimpse-lens: magnifier, color picker and ruler

glimpse-lens freezes the screen and shows a magnifying lens under your pointer. Scroll to zoom in,
up to 200 times; from six times on, a pixel grid appears.

<ThemeImage
  light="/screenshots/ecosystem/lens-picker.png"
  dark="/screenshots/ecosystem/lens-picker.png"
  alt="The glimpse-lens magnifier over the screen, showing enlarged pixels under the pointer"
/>

### Pick a color

```sh
glimpse-lens --tool picker | wl-copy
```

Move to the color you want and click, or press <kbd>Enter</kbd>. The color is printed, `#E0563F`
by default, and here goes straight to the clipboard. Add `--format rgb`, `hsl`, `hsv`, `oklch` or
`cmyk` for another notation. The arrow keys move one pixel at a time; <kbd>Esc</kbd> or a right
click cancels.

The [color picker applet](/config/applets/tools#color-picker) does the same from the bar and keeps
a palette of the colors you picked.

### Measure

```sh
glimpse-lens --tool ruler
```

Click where the measurement starts, then click again at each point along the way. Every click
measures one segment and starts the next, so you can trace a path. Hold <kbd>Shift</kbd> to keep
the line straight across or up and down. A right click starts over, and <kbd>Esc</kbd> finishes.

<ThemeImage
  light="/screenshots/ecosystem/lens-ruler.png"
  dark="/screenshots/ecosystem/lens-ruler.png"
  alt="The glimpse-lens ruler measuring the distance between two points on screen"
/>

The [ruler applet](/config/applets/tools#ruler) keeps your recent measurements.

## Bind them to keys

glimpse doesn't set any key bindings itself. Add them in your compositor's configuration, for
example:

| Key | Command |
| --- | --- |
| <kbd>Print</kbd> | `glimpse-shot --screen` |
| <kbd>Shift</kbd>+<kbd>Print</kbd> | `glimpse-shot --region` |
| <kbd>Alt</kbd>+<kbd>Print</kbd> | `glimpse-shot --window` |
| <kbd>Super</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> | `sh -c "glimpse-lens --tool picker \| wl-copy"` |
