---
name: gortex-glimpse-widgets-find
description: "Work in the glimpse-widgets · find area — 60 symbols across 5 files (72% cohesion)"
---

# glimpse-widgets · find

60 symbols | 5 files | 72% cohesion

## When to Use

Use this skill when working on files in:
- `crates/glimpse-widgets/examples/preview.rs`
- `crates/glimpse-widgets/src/calendar/imp.rs`
- `crates/glimpse-widgets/src/event_list/imp.rs`
- `crates/glimpse-widgets/src/fact_list/mod.rs`
- `crates/glimpse-widgets/src/world_clock/imp.rs`

## Key Files

| File | Symbols |
|------|---------|
| `crates/glimpse-widgets/examples/preview.rs` | root, selected, T, name, title, ... |
| `crates/glimpse-widgets/src/calendar/imp.rs` | button, date, dots, Day, label |
| `crates/glimpse-widgets/src/event_list/imp.rs` | EventList, constructed, signals, activatable, klass, ... |
| `crates/glimpse-widgets/src/fact_list/mod.rs` | new, default |
| `crates/glimpse-widgets/src/world_clock/imp.rs` | Type, NAME, twelve_hour, klass, now, ... |

## Connected Communities

- **glimpse-widgets/src +29 dirs** (9 cross-edges)
- **src/applet +16 dirs** (3 cross-edges)
- **glimpse-widgets · Transport** (3 cross-edges)
- **glimpse-widgets/src · constructed** (2 cross-edges)
- **src/section · Section** (2 cross-edges)
- **src/calendar +8 dirs** (2 cross-edges)
- **src/forecast · ForecastHour** (1 cross-edges)
- **glimpse-widgets/src +7 dirs** (1 cross-edges)
- **glimpse-lock/src +15 dirs** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-128")
explore(operation:"context", task:"understand glimpse-widgets · find", format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
