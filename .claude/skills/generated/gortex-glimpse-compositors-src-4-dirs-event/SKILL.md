---
name: gortex-glimpse-compositors-src-4-dirs-event
description: "Work in the glimpse-compositors/src +4 dirs · Event area — 82 symbols across 7 files (74% cohesion)"
---

# glimpse-compositors/src +4 dirs · Event

82 symbols | 7 files | 74% cohesion

## When to Use

Use this skill when working on files in:
- `crates/glimpse-compositors/src/event.rs`
- `crates/glimpse-compositors/src/hyprland/event.rs`
- `crates/glimpse-compositors/src/hyprland/mod.rs`
- `crates/glimpse-compositors/src/model.rs`
- `crates/glimpse-compositors/src/niri/event.rs`
- `crates/glimpse-panel/src/applet/runtime.rs`
- `crates/glimpse-widgets/src/workspaces_popover/mod.rs`

## Key Files

| File | Symbols |
|------|---------|
| `crates/glimpse-compositors/src/event.rs` | WindowFocusChanged, WorkspacesChanged, 0, Event, WindowLayoutsChanged, ... |
| `crates/glimpse-compositors/src/hyprland/event.rs` | line, EventState, address, layout_codes, decode, ... |
| `crates/glimpse-compositors/src/hyprland/mod.rs` | pid, address, lastwindow, into_model, urgent, ... |
| `crates/glimpse-compositors/src/model.rs` | name, is_focused, pid, title, output, ... |
| `crates/glimpse-compositors/src/niri/event.rs` | workspaces_changed, workspaces |
| `crates/glimpse-panel/src/applet/runtime.rs` | CommandOutput |
| `crates/glimpse-widgets/src/workspaces_popover/mod.rs` | summary, plural, count, workspaces |

## Connected Communities

- **glimpse-lock/src +15 dirs** (1 cross-edges)
- **src/hyprland · events_from** (1 cross-edges)
- **src/applet +16 dirs** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-3")
explore(operation:"context", task:"understand glimpse-compositors/src +4 dirs · Event", format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
