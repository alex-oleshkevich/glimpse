---
name: gortex-demo-components-3-dirs
description: "Work in the demo/components +3 dirs area — 247 symbols across 32 files (96% cohesion)"
---

# demo/components +3 dirs

247 symbols | 32 files | 96% cohesion

## When to Use

Use this skill when working on files in:
- ``
- `external-call::dep:kit.column`
- `external-call::dep:kit.label`
- `external-call::dep:kit.rowbox`
- `external-call::dep:kit.tree`
- `external-call::stdlib:pkgutil`
- `var/demo/components/__init__.py`
- `var/demo/components/back_row.py`
- `var/demo/components/buttons.py`
- `var/demo/components/card.py`
- `var/demo/components/choice_row.py`
- `var/demo/components/empty_state.py`
- `var/demo/components/entry.py`
- `var/demo/components/footer_row.py`
- `var/demo/components/heading.py`
- `var/demo/components/icon_button.py`
- `var/demo/components/lede.py`
- `var/demo/components/link_row.py`
- `var/demo/components/notif_parts.py`
- `var/demo/components/notification.py`
- `var/demo/components/row.py`
- `var/demo/components/rule.py`
- `var/demo/components/slider.py`
- `var/demo/components/status_line.py`
- `var/demo/components/title_row.py`
- `var/demo/components/toast_popup.py`
- `var/demo/demo.py`
- `var/demo/kit.py`
- `var/demo/popovers.py`
- `var/demo/widgets/controls.py`
- `var/demo/widgets/menu.py`
- `var/demo/widgets/notifications.py`

## Key Files

| File | Symbols |
|------|---------|
| `` | items, sort, remove, lower, append, ... |
| `external-call::dep:kit.column` | kit.column |
| `external-call::dep:kit.label` | kit.label |
| `external-call::dep:kit.rowbox` | kit.rowbox |
| `external-call::dep:kit.tree` | kit.tree |
| `external-call::stdlib:pkgutil` | pkgutil |
| `var/demo/components/__init__.py` | discover |
| `var/demo/components/back_row.py` | o, build |
| `var/demo/components/buttons.py` | o, build |
| `var/demo/components/card.py` | o, build |
| `var/demo/components/choice_row.py` | o, build |
| `var/demo/components/empty_state.py` | build, o |
| `var/demo/components/entry.py` | build, o |
| `var/demo/components/footer_row.py` | build, o |
| `var/demo/components/heading.py` | o, build |
| `var/demo/components/icon_button.py` | o, build |
| `var/demo/components/lede.py` | o, build |
| `var/demo/components/link_row.py` | o, build |
| `var/demo/components/notif_parts.py` | o, build |
| `var/demo/components/notification.py` | build, o |
| `var/demo/components/row.py` | o, build |
| `var/demo/components/rule.py` | build, o |
| `var/demo/components/slider.py` | o, build |
| `var/demo/components/status_line.py` | o, build |
| `var/demo/components/title_row.py` | o, build |
| `var/demo/components/toast_popup.py` | o, build |
| `var/demo/demo.py` | values, _on_axis, accents, axis, _fill_sidebar, ... |
| `var/demo/kit.py` | default_options |
| `var/demo/popovers.py` | sound, tray, keyboard, width, privacy, ... |
| `var/demo/widgets/controls.py` | on_click, icon, icon, __init__, value, ... |
| `var/demo/widgets/menu.py` | __init__, bad, Lede, toggle, Heading, ... |
| `var/demo/widgets/notifications.py` | Toast, image, notification_kwargs, on_click, __init__, ... |

## Connected Communities

- **. +3 dirs · gi.repository.Gtk** (12 cross-edges)
- **scripts +1 dirs · run_counter_contract** (3 cross-edges)
- **. +1 dirs · main · . · printing-mock-cups** (2 cross-edges)
- **demo · build** (1 cross-edges)
- **. +1 dirs · __init__** (1 cross-edges)
- **. +3 dirs · make_artwork** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-249")
explore(operation:"context", task:"understand demo/components +3 dirs", format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
