---
name: gortex-glimpse-lock-src-15-dirs
description: "Work in the glimpse-lock/src +15 dirs area — 101 symbols across 25 files (61% cohesion)"
---

# glimpse-lock/src +15 dirs

101 symbols | 25 files | 61% cohesion

## When to Use

Use this skill when working on files in:
- `.agents/skills/ratatui-tui/assets/templates/component-app/src/main.rs`
- `crates/glimpse-config/src/error.rs`
- `crates/glimpse-config/src/theme.rs`
- `crates/glimpse-contracts/src/topics.rs`
- `crates/glimpse-dbus/src/clients/notifications.rs`
- `crates/glimpse-ipc/src/lib.rs`
- `crates/glimpse-lock/src/app.rs`
- `crates/glimpse-lock/src/cli.rs`
- `crates/glimpse-lock/src/main.rs`
- `crates/glimpse-panel/src/app.rs`
- `crates/glimpse-panel/src/applet/mod.rs`
- `crates/glimpse-panel/src/applets/clock/popover.rs`
- `crates/glimpse-panel/src/cli.rs`
- `crates/glimpse-panel/src/main.rs`
- `crates/glimpse-services/src/broker.rs`
- `crates/glimpse-services/src/service.rs`
- `crates/glimpse-sunset/src/cli.rs`
- `crates/glimpse-sunset/src/main.rs`
- `crates/glimpse-wallpaper/src/app.rs`
- `crates/glimpse-wallpaper/src/cli.rs`
- `crates/glimpse-wallpaper/src/main.rs`
- `crates/glimpse-widgets/examples/preview.rs`
- `crates/glimpse-widgets/src/placeholder/imp.rs`
- `crates/glimpse-widgets/src/world_clock/imp.rs`
- `crates/glimpsectl/src/render.rs`

## Key Files

| File | Symbols |
|------|---------|
| `.agents/skills/ratatui-tui/assets/templates/component-app/src/main.rs` | config, debug, Cli |
| `crates/glimpse-config/src/error.rs` | text, a_parse_error_names_the_position_without_quoting_the_line, an_offset_off_the_end_or_mid_character_does_not_panic, offset, text, ... |
| `crates/glimpse-config/src/theme.rs` | theme_from_env |
| `crates/glimpse-contracts/src/topics.rs` | Payload |
| `crates/glimpse-dbus/src/clients/notifications.rs` | app_name, hints, now_ms, body, T, ... |
| `crates/glimpse-ipc/src/lib.rs` | explicit, runtime_dir, socket_path, resolve, explicit |
| `crates/glimpse-lock/src/app.rs` | config, AppInit, config_path, socket |
| `crates/glimpse-lock/src/cli.rs` | color, Cli, log, config, socket |
| `crates/glimpse-lock/src/main.rs` | main, cli, run |
| `crates/glimpse-panel/src/app.rs` | config_path, AppInit, socket, Init, config |
| `crates/glimpse-panel/src/applet/mod.rs` | a_payload_decodes_only_for_the_topic_that_declares_it, topic, T, payload, event, ... |
| `crates/glimpse-panel/src/applets/clock/popover.rs` | run, command |
| `crates/glimpse-panel/src/cli.rs` | socket, Parser, config, color, log, ... |
| `crates/glimpse-panel/src/main.rs` | main, run, cli |
| `crates/glimpse-services/src/broker.rs` | ok, output, T |
| `crates/glimpse-services/src/service.rs` | config, reconfigure |
| `crates/glimpse-sunset/src/cli.rs` | Cli, color, config, socket, log |
| `crates/glimpse-sunset/src/main.rs` | main, cli, run |
| `crates/glimpse-wallpaper/src/app.rs` | AppInit, config_path, config, socket |
| `crates/glimpse-wallpaper/src/cli.rs` | color, log, socket, config, Cli |
| `crates/glimpse-wallpaper/src/main.rs` | cli, main, run |
| `crates/glimpse-widgets/examples/preview.rs` | blueprint, scheme, Cli, fixture |
| `crates/glimpse-widgets/src/placeholder/imp.rs` | error |
| `crates/glimpse-widgets/src/world_clock/imp.rs` | constructed |
| `crates/glimpsectl/src/render.rs` | text, warn |

## Connected Communities

- **src/applet +16 dirs** (5 cross-edges)
- **src/calendar +8 dirs** (3 cross-edges)
- **glimpse-ipc/src +5 dirs** (2 cross-edges)
- **glimpsectl/src · ansi** (1 cross-edges)
- **src/applet +5 dirs** (1 cross-edges)
- **src/broker +5 dirs** (1 cross-edges)
- **glimpsed/src +2 dirs** (1 cross-edges)
- **glimpse-widgets/src +29 dirs** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-101")
explore(operation:"context", task:"understand glimpse-lock/src +15 dirs", format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
