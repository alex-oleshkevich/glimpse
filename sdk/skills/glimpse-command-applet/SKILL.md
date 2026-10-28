---
name: glimpse-command-applet
description: Use when the user wants a glimpse panel button, launcher or shortcut chip that runs commands on click or scroll, configured in panel.toml with no code. A live or updating applet (text that changes, popovers) needs glimpse-exec-applet instead.
---

# glimpse command applet

A command applet is a chip with an icon and/or a label. Each click or scroll gesture runs one program. It is a table in `panel.toml`; there is no project, build or install step.

## Command or exec?

- Use a command applet when the chip is fixed (icon and label never change) and each gesture only starts a program: launch a terminal, take a screenshot, change volume, toggle something.
- Use `glimpse-exec-applet` when the chip must show live state (a counter, a status, a changing icon), open a popover, or read command output. A command applet cannot display output. Do not fake it with polling shell scripts.

## Where it goes

File: `${XDG_CONFIG_HOME:-~/.config}/glimpse/panel.toml`. Add `[applets.<name>]` with `extends = "command"`, then put `<name>` in a panel zone (`left`, `center` or `right`).

Keys (all optional; unknown keys are a load error):

- `tooltip-format`: tooltip text, plain text. Unset means no tooltip.
- `icon`: a themed icon name, or an absolute path to an image. Prefer a `-symbolic` name, which follows the bar's color. A missing absolute file falls back to `system-run-symbolic`.
- `label`: text beside the icon.
- `env`: table of variables set for every program the chip runs, replacing same-named ones.
- `inherit-env`: default `true`, programs start from the panel's environment. When `false`, a program gets only `XDG_RUNTIME_DIR`, `WAYLAND_DISPLAY`, `DISPLAY`, its activation token and `env`.
- `on-click`: left click. `on-middle-click`: middle click. Other buttons do nothing.
- `on-scroll-up`, `on-scroll-down`, `on-scroll-left`, `on-scroll-right`: run once per wheel notch.

A gesture with no program does nothing.

## Programs are argv lists, not shell strings

Write each gesture as a list: the program, then its arguments. No shell runs, so an argument with spaces is one list item and nothing needs quoting. `on-click = "foot -e htop"` is wrong; write `["foot", "-e", "htop"]`.

When you need pipes, `&&`, `$VARIABLES` or globbing, run the shell yourself: `["sh", "-c", "<command line>"]`.

## Zones

Writing a zone replaces that zone's default list, and `[[panels]]` replaces the default panel. A zone you leave out of a `[[panels]]` table keeps its default, so write only the zone you change. Always copy that zone's default list and insert your name where it belongs. Do not write `right = ["mybutton"]` alone: the user loses the tray, network, audio and the rest.

Each zone lists names in the order they appear: `left` and `right` run left to right, and on a side panel `left` is the top end. The default panel (also in `/usr/share/glimpse/panel.commented.toml`) is:

- `left`: `workspace-name`, `pager`, `places`, `mpris`
- `center`: `clock`, `weather`, `notifications`, `privacy`
- `right`: `next-event`, `tray`, `removable`, `clipboard`, `printing`, `bluetooth`, `network`, `display`, `audio`, `idle`, `power`, `keyboard`, `session`

The clock is in `center`. When the user names a place that does not exist ("on the right, before the clock"), pick the closest reading, do it, and tell the user which one you chose. This example puts a button right before the clock, keeping every default entry:

```toml
[applets.terminal]
extends = "command"
icon = "utilities-terminal-symbolic"
tooltip-format = "Open a terminal"
on-click = ["foot"]
on-scroll-up = ["wpctl", "set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SINK@", "5%+"]
on-scroll-down = ["wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", "5%-"]

[[panels]]
center = ["terminal", "clock", "weather", "notifications", "privacy"]
```

`-l 1.0` keeps `wpctl` from raising the volume above 100%.

Check the existing file first. If the user already has a `[[panels]]` table, edit its zone and do not add a second table. If several panels exist, each is its own `[[panels]]` table.

Shell and environment example. It defines the chip only; place it in a zone as above:

```toml
[applets.louder]
extends = "command"
label = "Vol+"
env = { STEP = "5%" }
on-click = ["sh", "-c", "wpctl set-volume @DEFAULT_AUDIO_SINK@ \"$STEP+\" && notify-send Louder"]
on-middle-click = ["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]
```

## Naming

- No dot in the name. A dotted name (`me.example.Foo`) is read as a desktop-file id: in a zone with no table it resolves to an exec applet, and an unquoted dotted header (`[applets.me.example.Foo]`) is a load error.
- Do not use a built-in kind name. The kinds are the `extends` values in `/usr/share/glimpse/panel.schema.json`: audio, bluetooth, clipboard, clock, command, display, exec, heartbeat, idle, keyboard, mpris, network, next-event, notifications, pager, places, power, printing, privacy, removable, session, tray, weather, widget-demo, workspace-name.
- Use lowercase words with hyphens, one table per chip.

## Workflow

1. Read the existing `panel.toml` (it may not exist yet; create it).
2. Add the `[applets.<name>]` table and insert the name into a zone, keeping the default entries.
3. Validate. With `--log error` it prints nothing when the file is good:

```sh
glimpsectl --log error config validate
```

4. If the user's panel is running, try a gesture without touching the mouse:

```sh
glimpsectl applet click terminal
glimpsectl applet click terminal --button middle
glimpsectl applet scroll terminal up --steps 3
```

The panel hot-reloads `panel.toml` on save; do not restart it. Tell the user the chip should appear in its zone, and how to remove it (delete the table and its name from the zone).

## Traps

- A zone list you write replaces the default; omitting the defaults silently drops applets.
- `[[panels]]` replaces all panels, and the last file containing any `[[panels]]` wins. A drop-in with `[[panels]]` overrides the main file's panels entirely.
- A shell string where an argv list belongs is a load error ("expected a sequence").
- Quoting or `$VAR` inside a plain argv list is literal; only `sh -c` expands.
- A name the zone uses with no table and no matching kind is reported and skipped; the chip just does not appear.
- A misspelled key is a load error, so run `glimpsectl config validate` after every edit.
- Command applets cannot update their text or icon. Switch to `glimpse-exec-applet` for that.

## Full reference

- https://alex-oleshkevich.github.io/glimpse/config/applets/custom.md
- https://alex-oleshkevich.github.io/glimpse/applets/command/recipes.md
