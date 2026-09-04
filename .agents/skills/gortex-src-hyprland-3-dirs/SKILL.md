---
name: gortex-src-hyprland-3-dirs
description: "Work in the src/hyprland +3 dirs area — 72 symbols across 5 files (71% cohesion)"
---

# src/hyprland +3 dirs

72 symbols | 5 files | 71% cohesion

## When to Use

Use this skill when working on files in:
- `crates/glimpse-compositors/src/hyprland/event.rs`
- `crates/glimpse-compositors/src/hyprland/mod.rs`
- `crates/glimpse-compositors/tests/live.rs`
- `crates/glimpse-services/src/services/compositor.rs`
- `scripts/mpris-fake-players.py`

## Key Files

| File | Symbols |
|------|---------|
| `crates/glimpse-compositors/src/hyprland/event.rs` | active_keymap, matches, code, parenthesized, value |
| `crates/glimpse-compositors/src/hyprland/mod.rs` | width, scale, command, make, to, ... |
| `crates/glimpse-compositors/tests/live.rs` | every_addon_is_a_request_the_compositor_still_accepts |
| `crates/glimpse-services/src/services/compositor.rs` | dispatch, responder, command |
| `scripts/mpris-fake-players.py` | Next |

## Entry Points

- `crates/glimpse-compositors/tests/live.rs::every_addon_is_a_request_the_compositor_still_accepts`

## Connected Communities

- **src/hyprland · snapshot** (6 cross-edges)
- **glimpse-compositors/src · every_target_serializes_to_the_…** (4 cross-edges)
- **src/hyprland · every_addon_serializes_to_the_d…** (3 cross-edges)
- **glimpse-ipc/src +5 dirs** (3 cross-edges)
- **glimpse-compositors/src +4 dirs · WorkspaceTarget** (3 cross-edges)
- **src/broker +5 dirs** (2 cross-edges)
- **glimpse-lock/src +15 dirs** (2 cross-edges)
- **glimpse-compositors/src · Compositor** (1 cross-edges)
- **glimpse-compositors/src +3 dirs** (1 cross-edges)
- **src/schema +26 dirs** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-4")
explore(operation:"context", task:"understand src/hyprland +3 dirs", format:"gcx")
relations(operation:"usages", target:{symbol:"crates/glimpse-compositors/tests/live.rs::every_addon_is_a_request_the_compositor_still_accepts"}, format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
