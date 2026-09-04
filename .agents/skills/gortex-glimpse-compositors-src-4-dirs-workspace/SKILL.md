---
name: gortex-glimpse-compositors-src-4-dirs-workspace
description: "Work in the glimpse-compositors/src +4 dirs · workspace area — 77 symbols across 6 files (83% cohesion)"
---

# glimpse-compositors/src +4 dirs · workspace

77 symbols | 6 files | 83% cohesion

## When to Use

Use this skill when working on files in:
- `crates/glimpse-compositors/src/lib.rs`
- `crates/glimpse-compositors/src/model.rs`
- `crates/glimpse-compositors/src/niri/mod.rs`
- `crates/glimpse-contracts/src/types.rs`
- `crates/glimpse-services/src/services/compositor.rs`
- `crates/glimpse-widgets/src/range_bar.rs`

## Key Files

| File | Symbols |
|------|---------|
| `crates/glimpse-compositors/src/lib.rs` | snapshot |
| `crates/glimpse-compositors/src/model.rs` | connector, built_in, logical, focused_window, Snapshot, ... |
| `crates/glimpse-compositors/src/niri/mod.rs` | model, make, logical, name, current_mode, ... |
| `crates/glimpse-contracts/src/types.rs` | id, output, focused, windows, name, ... |
| `crates/glimpse-services/src/services/compositor.rs` | a_description_is_preferred_to_a_make_and_model, state, output, workspaces_are_ordered_by_output_then_by_position, idx, ... |
| `crates/glimpse-widgets/src/range_bar.rs` | color, from, snapshot, snapshot, y, ... |

## Entry Points

- `crates/glimpse-services/src/services/compositor.rs::a_window_that_opens_focused_takes_focus_from_whatever_had_it`

## Connected Communities

- **src/applet +16 dirs** (4 cross-edges)
- **glimpse-ipc/src · frame** (1 cross-edges)
- **glimpse-widgets/src +29 dirs** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-112")
explore(operation:"context", task:"understand glimpse-compositors/src +4 dirs · workspace", format:"gcx")
relations(operation:"usages", target:{symbol:"crates/glimpse-services/src/services/compositor.rs::a_window_that_opens_focused_takes_focus_from_whatever_had_it"}, format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
