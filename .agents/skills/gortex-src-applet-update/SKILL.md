---
name: gortex-src-applet-update
description: "Work in the src/applet · update area — 61 symbols across 3 files (76% cohesion)"
---

# src/applet · update

61 symbols | 3 files | 76% cohesion

## When to Use

Use this skill when working on files in:
- `crates/glimpse-panel/src/applet/catcher.rs`
- `crates/glimpse-panel/src/applet/mod.rs`
- `crates/glimpse-panel/src/applet/runtime.rs`

## Key Files

| File | Symbols |
|------|---------|
| `crates/glimpse-panel/src/applet/catcher.rs` | place, holds, center, widget |
| `crates/glimpse-panel/src/applet/mod.rs` | from_code, shutdown, code, a_pointer_button_keeps_its_gdk_code_when_it_has_no_name, name |
| `crates/glimpse-panel/src/applet/runtime.rs` | sender, input, root, catcher, event, ... |

## Connected Communities

- **src/calendar +8 dirs** (7 cross-edges)
- **glimpse-widgets/src +29 dirs** (4 cross-edges)
- **src/applet +16 dirs** (2 cross-edges)
- **src/applet · drained** (2 cross-edges)
- **src/applet · popover** (1 cross-edges)
- **glimpse-widgets/src +7 dirs** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-84")
explore(operation:"context", task:"understand src/applet · update", format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
