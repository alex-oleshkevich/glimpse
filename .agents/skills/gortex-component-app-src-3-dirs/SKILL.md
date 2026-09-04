---
name: gortex-component-app-src-3-dirs
description: "Work in the component-app/src +3 dirs area — 83 symbols across 12 files (82% cohesion)"
---

# component-app/src +3 dirs

83 symbols | 12 files | 82% cohesion

## When to Use

Use this skill when working on files in:
- `.agents/skills/ratatui-tui/assets/templates/component-app/src/action.rs`
- `.agents/skills/ratatui-tui/assets/templates/component-app/src/app.rs`
- `.agents/skills/ratatui-tui/assets/templates/component-app/src/config.rs`
- `.agents/skills/ratatui-tui/assets/templates/component-app/src/event.rs`
- `.agents/skills/ratatui-tui/assets/templates/component-app/src/logging.rs`
- `.agents/skills/ratatui-tui/assets/templates/component-app/src/main.rs`
- `.agents/skills/ratatui-tui/assets/templates/component-app/src/tui.rs`
- `.agents/skills/ratatui-tui/assets/templates/component-app/src/ui.rs`
- `.agents/skills/ratatui-tui/assets/templates/hello-world/src/main.rs`
- `crates/glimpse-ipc/src/codec.rs`
- `crates/glimpse-ipc/src/frame.rs`
- `crates/glimpse-panel/src/applets/heartbeat.rs`

## Key Files

| File | Symbols |
|------|---------|
| `.agents/skills/ratatui-tui/assets/templates/component-app/src/action.rs` | Down, Quit, Navigate, Action, Render, ... |
| `.agents/skills/ratatui-tui/assets/templates/component-app/src/app.rs` | config, should_quit, events, update, handle_key, ... |
| `.agents/skills/ratatui-tui/assets/templates/component-app/src/config.rs` | path, load |
| `.agents/skills/ratatui-tui/assets/templates/component-app/src/event.rs` | 0, Resize, new, AppEvent, 1, ... |
| `.agents/skills/ratatui-tui/assets/templates/component-app/src/logging.rs` | init_logging, debug, _config |
| `.agents/skills/ratatui-tui/assets/templates/component-app/src/main.rs` | main |
| `.agents/skills/ratatui-tui/assets/templates/component-app/src/tui.rs` | draw, f, enter, F, terminal, ... |
| `.agents/skills/ratatui-tui/assets/templates/component-app/src/ui.rs` | render_status, frame, render_list, render, app, ... |
| `.agents/skills/ratatui-tui/assets/templates/hello-world/src/main.rs` | main, frame, render |
| `crates/glimpse-ipc/src/codec.rs` | Item |
| `crates/glimpse-ipc/src/frame.rs` | a_hello_from_before_the_version_was_dropped_still_connects, id, body, round_trip, Frame, ... |
| `crates/glimpse-panel/src/applets/heartbeat.rs` | stepped, scrolling_walks_the_period_and_stops_at_the_ends, a_horizontal_scroll_leaves_the_period_alone, direction, period_ms |

## Entry Points

- `.agents/skills/ratatui-tui/assets/templates/component-app/src/main.rs::main`

## Connected Communities

- **src/applet +5 dirs** (1 cross-edges)
- **glimpse-widgets/src +29 dirs** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-1")
explore(operation:"context", task:"understand component-app/src +3 dirs", format:"gcx")
relations(operation:"usages", target:{symbol:".agents/skills/ratatui-tui/assets/templates/component-app/src/main.rs::main"}, format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
