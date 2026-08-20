# glimpse-devtools

Widget previewer. Development only — never packaged, never installed, no systemd unit.

```bash
glimpse-devtools --list
glimpse-devtools battery-indicator --theme both
glimpse-devtools tray-item --fixture fixtures/tray.toml --state no-icon
```

## What it does

- Renders any widget from `glimpse-widgets` in an ordinary window, with no daemon and no
  layer-shell
- Reloads CSS and Blueprint output without restarting or losing the current state
- Drives a widget from fixtures through the states that are awkward to reproduce live: `degraded`,
  `stale`, empty, overflowing text, a 40-character SSID, a tray item with no icon
- `--socket` swaps fixtures for a real daemon connection when checking against live data

Spec: [`specs/008_glimpse_devtools.md`](../../specs/008_glimpse_devtools.md)
