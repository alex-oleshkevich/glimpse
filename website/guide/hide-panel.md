# Hide the panel until you need it

The panel can slide off the screen and come back when you ask for it, so windows get the whole
display in between. Every setting is in the [panels reference](/config/panels#hiding-the-panel).

## Start hidden and peek from the edge

Set `visibility = "hide"` to start with the panel out of sight, and `edge-reveal = true` to bring it
back by resting the mouse on the screen edge it hides behind:

```toml
[[panels]]
visibility = "hide"
edge-reveal = true
hide-delay-ms = 1000
```

Rest the pointer on that edge for a moment and the panel slides in over your windows. A quick pass
along the edge does nothing, so you do not summon it by accident.

The panel stays while the pointer is on it or one of its popovers is open. Once both are over, it
waits `hide-delay-ms` (1000 by default, so one second) and slides out. Set it to `0` to hide at
once, up to `60000` for a minute.

Without `edge-reveal`, a hidden panel ignores the pointer completely, so it never takes a click
meant for a window. With it, only a thin strip, 2 pixels wide, on the screen edge listens.

## Show or hide it from a key

Your compositor binds the keys; glimpse supplies the commands. Two of them do different things:

- `glimpsectl panel toggle` shows a hidden panel and hides a shown one. A shown panel keeps its
  space, so windows make room for it and resize smoothly as it slides.
- `glimpsectl panel reveal` slides the panel in over your windows, which do not move. It stays until
  `glimpsectl panel conceal`, or for a set time with `--for`.

```kdl
// ~/.config/niri/config.kdl
binds {
    Mod+B { spawn "glimpsectl" "panel" "reveal" "--for" "2000"; }
    Mod+Shift+B { spawn "glimpsectl" "panel" "toggle"; }
}
```

```ini
# ~/.config/hypr/hyprland.conf
bind = SUPER, B, exec, glimpsectl panel reveal --for 2000
bind = SUPER SHIFT, B, exec, glimpsectl panel toggle
```

`--for 2000` holds the panel for two seconds, then the usual hide delay applies, so with the default
it slides out three seconds after the key. Leave `--for` out and the panel stays until you run
`conceal`. Running `toggle` on a panel that is only revealed makes it shown.

`show` and `hide` do the two halves of `toggle` on their own. Add `--output` and the monitor name
to pick one monitor's panel, and `--edge` (`top`, `bottom`, `left` or `right`) when a monitor has
several panels. Without them a command applies to every panel. Every option is in the
[glimpsectl reference](/config/cli/glimpsectl).

## Show it while the overview is open

glimpse does not watch your compositor, so a small script does it: reveal the panel when the
overview opens and conceal it when the overview closes. Save it as `~/.local/bin/panel-overview`,
make it executable, and start it with your compositor's startup list.

```sh
#!/bin/sh
niri msg --json event-stream \
  | jq --unbuffered -r '.OverviewOpenedOrClosed.is_open // empty' \
  | while read -r open; do
      if [ "$open" = true ]; then glimpsectl panel reveal; else glimpsectl panel conceal; fi
    done
```

```kdl
// ~/.config/niri/config.kdl
spawn-at-startup "sh" "-c" "$HOME/.local/bin/panel-overview"
```

The script needs `jq`.

## How long a change lasts

A change from `glimpsectl` lasts until the panel restarts or you edit your `[[panels]]` tables. After
that the panel goes back to what the file says, so put the setting you want at login into
`visibility`.

::: tip
A compositor rule that draws a shadow or blur behind the panel keeps drawing it while the panel is
hidden.
:::
