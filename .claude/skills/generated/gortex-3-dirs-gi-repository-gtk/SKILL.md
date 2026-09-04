---
name: gortex-3-dirs-gi-repository-gtk
description: "Work in the . +3 dirs · gi.repository.Gtk area — 66 symbols across 18 files (81% cohesion)"
---

# . +3 dirs · gi.repository.Gtk

66 symbols | 18 files | 81% cohesion

## When to Use

Use this skill when working on files in:
- ``
- `external-call::dep:common.init`
- `external-call::dep:common.render`
- `external-call::dep:gi.repository.GLib`
- `external-call::dep:gi.repository.Gdk`
- `external-call::dep:gi.repository.Graphene`
- `external-call::dep:gi.repository.Gsk`
- `external-call::dep:gi.repository.Gtk`
- `external-call::dep:gi.repository.Gtk4LayerShell`
- `external-call::stdlib:demo`
- `var/demo/components/tile.py`
- `var/demo/demo.py`
- `var/demo/kit.py`
- `var/demo/popovers.py`
- `var/demo/sheet.py`
- `var/demo/shot.py`
- `var/demo/widgets/calendar.py`
- `var/demo/widgets/controls.py`

## Key Files

| File | Symbols |
|------|---------|
| `` | Calendar, calendar, traceback, monthdatescalendar, print_exc |
| `external-call::dep:common.init` | common.init |
| `external-call::dep:common.render` | common.render |
| `external-call::dep:gi.repository.GLib` | gi.repository.GLib |
| `external-call::dep:gi.repository.Gdk` | gi.repository.Gdk |
| `external-call::dep:gi.repository.Graphene` | gi.repository.Graphene |
| `external-call::dep:gi.repository.Gsk` | gi.repository.Gsk |
| `external-call::dep:gi.repository.Gtk` | gi.repository.Gtk |
| `external-call::dep:gi.repository.Gtk4LayerShell` | gi.repository.Gtk4LayerShell |
| `external-call::stdlib:demo` | demo |
| `var/demo/components/tile.py` | o, build |
| `var/demo/demo.py` | __init__, Demo, do_activate |
| `var/demo/kit.py` | spacing, classes, text, label, children, ... |
| `var/demo/popovers.py` | year, events, calendar_grid, month, selected, ... |
| `var/demo/sheet.py` | _controller, main, viewer, on_key, width, ... |
| `var/demo/shot.py` | setup, __init__, capture, Shot, do_activate |
| `var/demo/widgets/calendar.py` | selected, outside, DayCell, events, day, ... |
| `var/demo/widgets/controls.py` | __init__, active, on_toggle, icon, Tile, ... |

## Connected Communities

- **demo/components +3 dirs** (6 cross-edges)
- **. +3 dirs · run** (3 cross-edges)
- **. +1 dirs · main · . · printing-mock-cups** (1 cross-edges)
- **. +1 dirs · __init__** (1 cross-edges)
- **scripts +1 dirs · run_counter_contract** (1 cross-edges)
- **demo/components +1 dirs** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-246")
explore(operation:"context", task:"understand . +3 dirs · gi.repository.Gtk", format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
