---
state: draft
---

# 008 — glimpse-devtools

The widget previewer. Development only, never installed.

## Problem

Iterating on a widget by restarting the panel is slow and needs a working daemon, a compositor with
layer-shell, and the right topics carrying plausible values. Most widget work is layout and CSS,
none of which needs any of that.

## Goals

- Render any widget from `glimpse-widgets` in an ordinary window, with no daemon and no layer-shell.
- Reload the stylesheet and Blueprint templates without restarting.
- Drive a widget through its states from fixtures, including the states that are hard to reproduce
  live: `degraded`, `stale`, empty, overflowing text.

## Non-goals

- Not shipped. Not in any package, not in `data/`, no systemd unit.
- Not a test runner. It is looked at by a human.

## Tech

### Invocation

```
glimpse-devtools [OPTIONS] [WIDGET]
```

### Arguments

| Argument   | Purpose                                                       |
| ---------- | ------------------------------------------------------------- |
| `[WIDGET]` | Widget to open at startup. Omitted, it opens the widget list. |

### Flags

| Flag                 | Default         | Purpose                                                          |
| -------------------- | --------------- | ---------------------------------------------------------------- |
| `-l`, `--list`       | off             | Print the available widgets and exit                             |
| `--css <PATH>`       | panel stylesheet| Stylesheet to apply, watched for changes                         |
| `--fixture <PATH>`   | built-in        | TOML or JSON file of sample topic values to feed the widget      |
| `--state <NAME>`     | `default`       | Select a named state from the fixture                            |
| `--socket <PATH>`    | none            | Connect to a real daemon instead of fixtures                     |
| `--theme <MODE>`     | `light`         | `light`, `dark`, or `both` for a side-by-side window             |
| `--scale <FACTOR>`   | `1`             | Render at a fractional scale to check layout                     |
| `--inspector`        | off             | Open the GTK inspector at startup                                |
| `-h`, `--help`       |                 |                                                                  |

### Behaviour

- **Hot reload.** CSS and Blueprint output are watched; a change re-applies without losing the
  current widget or state.
- **Fixtures.** A fixture file maps topic names to values, with named states. This is where the
  awkward cases live: a 40-character SSID, a tray item with no icon, a battery at 3%, a `stale`
  value, a `degraded` service.
- **Live mode.** `--socket` swaps fixtures for a real daemon connection, for the last mile of
  checking against real data.

### Exit codes

| Code | Meaning                            |
| ---- | ---------------------------------- |
| 0    | clean exit                          |
| 1    | fixture invalid, or unknown widget  |
| 2    | usage error                         |

## Changelog

- 2026-08-20 — created, split out of `001_architecture.md`.
