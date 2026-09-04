---
name: gortex-src-calendar-8-dirs
description: "Work in the src/calendar +8 dirs area — 196 symbols across 14 files (80% cohesion)"
---

# src/calendar +8 dirs

196 symbols | 14 files | 80% cohesion

## When to Use

Use this skill when working on files in:
- `crates/glimpse-dbus/src/clients/mpris.rs`
- `crates/glimpse-panel/src/applet/catcher.rs`
- `crates/glimpse-panel/src/applet/runtime.rs`
- `crates/glimpse-panel/src/applets/clock/popover.rs`
- `crates/glimpse-services/src/publisher.rs`
- `crates/glimpse-services/src/services/compositor.rs`
- `crates/glimpse-services/src/subscription.rs`
- `crates/glimpse-widgets/examples/preview.rs`
- `crates/glimpse-widgets/src/calendar/grid.rs`
- `crates/glimpse-widgets/src/calendar/imp.rs`
- `crates/glimpse-widgets/src/calendar/mod.rs`
- `crates/glimpse-widgets/src/dots.rs`
- `crates/glimpse-widgets/src/indicator_group/imp.rs`
- `crates/glimpse-widgets/src/range_bar.rs`

## Key Files

| File | Symbols |
|------|---------|
| `crates/glimpse-dbus/src/clients/mpris.rs` | play |
| `crates/glimpse-panel/src/applet/catcher.rs` | center, arrow, a_popover_centers_on_its_item, layout, horizontal, ... |
| `crates/glimpse-panel/src/applet/runtime.rs` | settle, group, launch, configure, dy, ... |
| `crates/glimpse-panel/src/applets/clock/popover.rs` | ymd, date, date, date |
| `crates/glimpse-services/src/publisher.rs` | set, value |
| `crates/glimpse-services/src/services/compositor.rs` | publish |
| `crates/glimpse-services/src/subscription.rs` | Start, S |
| `crates/glimpse-widgets/examples/preview.rs` | calendar_events, calendar |
| `crates/glimpse-widgets/src/calendar/grid.rs` | weekday, day, year, day, year, ... |
| `crates/glimpse-widgets/src/calendar/imp.rs` | year, format_month_short, instance_init, scope_label, toggle_view, ... |
| `crates/glimpse-widgets/src/calendar/mod.rs` | set_events, step, events, select, today, ... |
| `crates/glimpse-widgets/src/dots.rs` | uniform, set_colors, default, set_uniform, colors |
| `crates/glimpse-widgets/src/indicator_group/imp.rs` | items, NAME, class_init, signals, klass, ... |
| `crates/glimpse-widgets/src/range_bar.rs` | default |

## Entry Points

- `crates/glimpse-panel/src/applet/runtime.rs::an_applet_reaches_its_group`

## Connected Communities

- **src/applet +16 dirs** (12 cross-edges)
- **glimpse-widgets/src +7 dirs** (4 cross-edges)
- **component-app/src +3 dirs** (2 cross-edges)
- **src/applet +5 dirs** (2 cross-edges)
- **glimpse-compositors/src +4 dirs · workspace** (2 cross-edges)
- **glimpse-lock/src +15 dirs** (1 cross-edges)
- **glimpse-widgets/src · Dots** (1 cross-edges)
- **src/broker +5 dirs** (1 cross-edges)
- **src/schema +26 dirs** (1 cross-edges)
- **src/schema +2 dirs** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-86")
explore(operation:"context", task:"understand src/calendar +8 dirs", format:"gcx")
relations(operation:"usages", target:{symbol:"crates/glimpse-panel/src/applet/runtime.rs::an_applet_reaches_its_group"}, format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
