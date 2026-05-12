# Monitors Applet

Lists the displays connected to your compositor and lets you toggle each one on or off from the popover. Glimpse guarantees at least one output stays enabled — if every monitor ends up disabled (by Glimpse or any external tool), the daemon re-enables the built-in display so you don't get stuck with a dark session.

## Panel icon

The applet always renders `video-display-symbolic`. The icon is static and does not change with the on/off state of any output.

## Popover

One row per detected output:

- Row label is the EDID model name when the compositor reports it, otherwise the connector name (e.g. `eDP-1`, `DP-2`).
- Tooltip shows `connector · WxH @ refresh Hz` for enabled outputs, or just the connector name when the output is disabled.
- A toggle on the right edge of each row enables or disables that output.

### Last-monitor warning

Clicking the toggle on the only currently enabled monitor opens a warning dialog and leaves the output enabled. To switch monitors safely, enable the new one first, then disable the old one.

### All-off recovery

If any external client (`kanshi`, `niri msg`, `hyprctl`, scripts) disables every output, the daemon waits 500 ms and then re-enables the built-in display. On a system with no laptop panel, the first output by connector name is re-enabled instead. Failed recoveries are followed by a 2-second cooldown before the watchdog tries again.

## Configuration

Opt in by adding the applet to a panel section:

```toml
[[panels.right.applets]]
type = "monitors"

# Optional override for built-in detection
[monitors]
builtin_connector = "eDP-1"
```

| Option | Default | Meaning |
|---|---|---|
| `builtin_connector` | auto-detected | Connector name to treat as the built-in display for the all-off recovery. |

Glimpse auto-detects the built-in display by looking for connectors matching `eDP`, `LVDS`, or `DSI`. Set `builtin_connector` only on unusual hardware where the laptop panel is wired to a different connector (for example, an internal HDMI link on some convertibles).

## Compositor support

| Compositor | Status | Notes |
|---|---|---|
| niri | full | Toggling uses the IPC `Action::Output` command. |
| Hyprland | full | Toggling uses `keyword monitor`. Toggle state does not survive `hyprctl reload`. |
| GNOME, KDE, others | unsupported | The applet relies on niri or Hyprland IPC. The icon still renders but toggles fail silently and the compositor service stays in the unsupported state. |

## Troubleshooting

- **A toggle does nothing.** The `glimpse-shell` log shows the dispatched command and any compositor error. Confirm `NIRI_SOCKET` or `HYPRLAND_INSTANCE_SIGNATURE` is set in the shell's environment.
- **The popover shows zero monitors.** The compositor IPC isn't reachable. Restart `glimpse-shell` after the compositor is up.
- **An external tool keeps fighting the watchdog.** If `kanshi` or a similar tool keeps re-disabling outputs, either stop that tool or raise the recovery cooldown so Glimpse stops competing with it.
