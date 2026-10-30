# Screenshots

glimpse-shot freezes the screen, lets you mark it up and then copies or saves the result. Every
option is in the [command reference](/config/cli/glimpse-shot), and the defaults it starts with are
under [`[screenshot]`](/config/services/screenshots).

<ThemeImage
  light="/screenshots/ecosystem/shot-editor-light.png"
  dark="/screenshots/ecosystem/shot-editor-dark.png"
  alt="The glimpse-shot editor over a frozen screen: a toolbar at the top, a weather panel circled in red with an arrow and a label, one row boxed in yellow and numbered, and the media title in the bar blurred"
/>

## Take a screenshot

| Run | To |
| --- | --- |
| `glimpse-shot --screen` | freeze the screen you are on and mark it up |
| `glimpse-shot --region` | drag a rectangle, then mark it up |
| `glimpse-shot --window` | click a window, then mark it up |
| `glimpse-shot --screen --delay 5` | count down five seconds first, to catch a menu or tooltip |
| `glimpse-shot` | open the screenshot app and choose there |
| `glimpse-shot picture.png` | mark up a picture you already have |
| `glimpse-shot annotate --pick` | choose an image to mark up |

The screen freezes the moment the screenshot is taken, so a video or a closing menu stays put while
you work. The delay goes up to 60 seconds.

The screenshot app's window should float. A window rule in your compositor's configuration does it
(the syntax is your compositor's):

```kdl
// ~/.config/niri/config.kdl
window-rule {
    match app-id="^me\\.aresa\\.Glimpse\\.Shot$"
    open-floating true
}
```

```lua
-- ~/.config/hypr/hyprland.lua
hl.window_rule({
    name  = "glimpse-shot",
    match = { class = "^me\\.aresa\\.Glimpse\\.Shot$" },
    float = true,
})
```

```ini
# ~/.config/hypr/hyprland.conf
windowrule = match:class ^(me\.aresa\.Glimpse\.Shot)$, float on
```

## Mark it up

Pick a tool from the bar at the top, or press the letter shown in its tooltip: arrows, boxes,
ovals, numbered steps, text, a highlighter, a pen, crop, and blur for anything private.
<kbd>1</kbd> to <kbd>5</kbd> switch the color, <kbd>[</kbd> and <kbd>]</kbd> the thickness, and
<kbd>Ctrl</kbd>+<kbd>Z</kbd> undoes the last mark.

## Copy or save it

| Press | To |
| --- | --- |
| <kbd>Enter</kbd> | copy the picture and close |
| <kbd>Shift</kbd>+<kbd>Enter</kbd> | save it and close |
| <kbd>Ctrl</kbd>+<kbd>C</kbd> | copy it and close |
| <kbd>Ctrl</kbd>+<kbd>S</kbd> | save it and keep editing |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd> | choose where to save it |
| <kbd>Ctrl</kbd>+<kbd>E</kbd> | move it into a normal window to keep working |
| <kbd>Esc</kbd> | close without saving |

In the window that <kbd>Ctrl</kbd>+<kbd>E</kbd> opens, <kbd>Ctrl</kbd>+<kbd>C</kbd> copies the picture and keeps the window open.

Saved pictures go to `Screenshots` inside your Pictures folder, named after the time they were
taken, for example `Screenshot from 2026-10-15 21-19-04.png`. Change the folder or the name in the
app's preferences or in [`[screenshot]`](/config/services/screenshots), or pass `-o` to save one
picture somewhere else.

## Screenshots other apps ask for

When an app asks the system for a screenshot, for example a chat app's screenshot button, the same
editor usually opens, with a banner naming the app that asked. Mark it up if you like, then press
<kbd>Enter</kbd> to hand it over or <kbd>Esc</kbd> to refuse. An app you already allowed to take
screenshots without asking gets its picture straight away, with no editor.

## Use it in scripts

`glimpse-shot capture` takes the screenshot straight away and shows nothing:

```sh
# Copy the screen you are on, no file
glimpse-shot capture --copy --no-save

# Save the focused window
glimpse-shot capture --window -o window.png

# Drag a rectangle with slurp and pipe the PNG on, here to ImageMagick
glimpse-shot capture --region "$(slurp)" -o - | magick - -resize 50% half.png
```

`glimpse-shot list --json` prints every screen and window with its name and id, and its position
when the compositor reports one, so a script can pick its target first. `--region`, `--window`,
`--scale logical` and `list` ask your compositor for that layout, so they need a compositor glimpse
supports; on any other, name the screen with `--screen NAME`.

Under `capture`, `--annotate` draws marks without opening the editor. The marks are a JSON list, and their
coordinates are pixels of the captured picture, counted from its top-left corner:

```sh
glimpse-shot capture --window --copy --no-save --annotate - <<'EOF'
[{"kind": "blur", "x": 40, "y": 12, "width": 300, "height": 24},
 {"kind": "oval", "x": 500, "y": 200, "width": 220, "height": 60, "color": "red", "stroke": 3}]
EOF
```

Add `--json` to get a description of the result: the saved path, whether it was copied, the
picture's size and every mark on it. `-o -` and `--json` both write to standard output, so use one
or the other.

| Exit code | Meaning |
| --- | --- |
| 0 | saved, copied or written out |
| 1 | failed, including a mark list that does not parse |
| 2 | a command or option was used wrongly |
| 3 | the configuration is broken |
| 4 | you closed the editor without saving or copying |
| 5 | the screen or window was not found |
| 7 | nobody answered before `--timeout` ran out |

## Use it with AI agents

`glimpse-shot prime` prints a guide written for agents: every command, the mark format, the result
object and the exit codes. Point your agent at it once and it can take screenshots on its own.

To show an agent your screen, let it run `capture`. `--scale logical` saves the picture at the size
you see rather than at full resolution, which costs the model fewer tokens. Saving under
`$XDG_RUNTIME_DIR` keeps these pictures out of your Pictures folder, which may be synced:

```sh
glimpse-shot capture --scale logical -o "$XDG_RUNTIME_DIR/screen.png"
```

An agent can also ask you to point something out. The editor opens with its question as a banner;
you draw on the screen and press <kbd>Enter</kbd>, and the agent reads your marks back:

```sh
glimpse-shot --screen --prompt "Circle the button that does nothing" --timeout 60 --json \
  | jq '.annotations'
```

The marks come back in the same format `--annotate` takes, so the agent can draw its own answer
over the same screen.
