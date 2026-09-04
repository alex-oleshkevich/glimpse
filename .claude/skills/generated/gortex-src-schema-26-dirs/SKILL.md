---
name: gortex-src-schema-26-dirs
description: "Work in the src/schema +26 dirs area — 474 symbols across 55 files (77% cohesion)"
---

# src/schema +26 dirs

474 symbols | 55 files | 77% cohesion

## When to Use

Use this skill when working on files in:
- `.agents/skills/ratatui-tui/assets/templates/component-app/src/action.rs`
- `crates/glimpse-compositors/src/event.rs`
- `crates/glimpse-compositors/src/hyprland/mod.rs`
- `crates/glimpse-compositors/src/lib.rs`
- `crates/glimpse-compositors/src/model.rs`
- `crates/glimpse-compositors/src/niri/mod.rs`
- `crates/glimpse-config/examples/gen_config_default.rs`
- `crates/glimpse-config/src/error.rs`
- `crates/glimpse-config/src/lib.rs`
- `crates/glimpse-config/src/schema/appearance.rs`
- `crates/glimpse-config/src/schema/applets.rs`
- `crates/glimpse-config/src/schema/backdrop.rs`
- `crates/glimpse-config/src/schema/calendar.rs`
- `crates/glimpse-config/src/schema/geolocation.rs`
- `crates/glimpse-config/src/schema/idle.rs`
- `crates/glimpse-config/src/schema/keyboard.rs`
- `crates/glimpse-config/src/schema/lock.rs`
- `crates/glimpse-config/src/schema/mod.rs`
- `crates/glimpse-config/src/schema/monitors.rs`
- `crates/glimpse-config/src/schema/night_light.rs`
- `crates/glimpse-config/src/schema/panels.rs`
- `crates/glimpse-config/src/schema/power.rs`
- `crates/glimpse-config/src/schema/wallpaper.rs`
- `crates/glimpse-config/src/watch.rs`
- `crates/glimpse-contracts/src/commands.rs`
- `crates/glimpse-contracts/src/types.rs`
- `crates/glimpse-ipc/src/frame.rs`
- `crates/glimpse-ipc/src/server.rs`
- `crates/glimpse-panel/src/app.rs`
- `crates/glimpse-panel/src/applet/mod.rs`
- `crates/glimpse-panel/src/applets/clock/indicator.rs`
- `crates/glimpse-panel/src/applets/clock/popover.rs`
- `crates/glimpse-panel/src/cli.rs`
- `crates/glimpse-panel/src/components/panel.rs`
- `crates/glimpse-services/src/broker.rs`
- `crates/glimpse-services/src/service.rs`
- `crates/glimpse-services/src/services/calendar.rs`
- `crates/glimpse-services/src/services/compositor.rs`
- `crates/glimpse-services/src/services/geolocation.rs`
- `crates/glimpse-services/src/services/heartbeat.rs`
- `crates/glimpse-services/src/services/solar.rs`
- `crates/glimpse-services/src/testing.rs`
- `crates/glimpse-utils/src/args.rs`
- `crates/glimpse-utils/src/log.rs`
- `crates/glimpse-widgets/examples/preview.rs`
- `crates/glimpse-widgets/src/choice_list/mod.rs`
- `crates/glimpse-widgets/src/event_list/mod.rs`
- `crates/glimpse-widgets/src/fact_list/mod.rs`
- `crates/glimpse-widgets/src/forecast/mod.rs`
- `crates/glimpse-widgets/src/notice/imp.rs`
- `crates/glimpse-widgets/src/player_list/mod.rs`
- `crates/glimpse-widgets/src/row/imp.rs`
- `crates/glimpse-widgets/src/split_row/mod.rs`
- `crates/glimpse-widgets/src/transport/imp.rs`
- `crates/glimpse-widgets/src/workspace_list/mod.rs`

## Key Files

| File | Symbols |
|------|---------|
| `.agents/skills/ratatui-tui/assets/templates/component-app/src/action.rs` | Clone, PartialEq, Copy, Eq |
| `crates/glimpse-compositors/src/event.rs` | Outputs, Resync, Structure, Keyboard |
| `crates/glimpse-compositors/src/hyprland/mod.rs` | id, WireActiveWorkspace, WireKeyboard, id, WireWorkspaceRef, ... |
| `crates/glimpse-compositors/src/lib.rs` | workspace_reorder, NONE, floating, Capabilities |
| `crates/glimpse-compositors/src/model.rs` | x, WindowId, 0, Prev, Next, ... |
| `crates/glimpse-compositors/src/niri/mod.rs` | WireMode, width, refresh_rate, height, name, ... |
| `crates/glimpse-config/examples/gen_config_default.rs` | main |
| `crates/glimpse-config/src/error.rs` | count, Fixture |
| `crates/glimpse-config/src/lib.rs` | default_document |
| `crates/glimpse-config/src/schema/appearance.rs` | Light, theme, Appearance, ColorScheme, default, ... |
| `crates/glimpse-config/src/schema/applets.rs` | TwentyFour, urgent_label, Command, Battery, label, ... |
| `crates/glimpse-config/src/schema/backdrop.rs` | path, blur_radius, Backdrop, enabled, default |
| `crates/glimpse-config/src/schema/calendar.rs` | Calendar, default, poll_interval, id, Ical, ... |
| `crates/glimpse-config/src/schema/geolocation.rs` | latitude, longitude, Geolocation, Geoclue, Manual |
| `crates/glimpse-config/src/schema/idle.rs` | enabled, on_idle, on_resume, ac, timeout, ... |
| `crates/glimpse-config/src/schema/keyboard.rs` | labels, Remember, Global, Keyboard, remember, ... |
| `crates/glimpse-config/src/schema/lock.rs` | buttons, background, controls, Clock, default, ... |
| `crates/glimpse-config/src/schema/mod.rs` | a_configured_applet_survives_being_written_back_out, monitors, idle, a_settings_command_that_names_no_program_is_refused, a_setting_no_applet_declares_is_refused, ... |
| `crates/glimpse-config/src/schema/monitors.rs` | Default, Monitors, builtin_connector, JsonSchema |
| `crates/glimpse-config/src/schema/night_light.rs` | Automatic, temperature, schedule, start_time, Off, ... |
| `crates/glimpse-config/src/schema/panels.rs` | margin, Hash, size, center, left, ... |
| `crates/glimpse-config/src/schema/power.rs` | lock_on_request, lock_before_sleep, Power, Serialize, Deserialize, ... |
| `crates/glimpse-config/src/schema/wallpaper.rs` | Cover, Contain, color, default, Fit, ... |
| `crates/glimpse-config/src/watch.rs` | Logged, 0 |
| `crates/glimpse-contracts/src/commands.rs` | Args |
| `crates/glimpse-contracts/src/types.rs` | previous_ms, all_day, Id, summary, service, ... |
| `crates/glimpse-ipc/src/frame.rs` | seq, ts, topic, data, stale, ... |
| `crates/glimpse-ipc/src/server.rs` | 0, ClientId |
| `crates/glimpse-panel/src/app.rs` | index, monitor, Key |
| `crates/glimpse-panel/src/applet/mod.rs` | Right, Pointer, Middle, Button, 0, ... |
| `crates/glimpse-panel/src/applets/clock/indicator.rs` | hour_format, twelve_hour |
| `crates/glimpse-panel/src/applets/clock/popover.rs` | shown, marker, reads_as_twelve_hour, locale_is_twelve_hour, a_locale_that_writes_a_meridiem_into_its_own_time_reads_as_twelve_hour |
| `crates/glimpse-panel/src/cli.rs` | Debug |
| `crates/glimpse-panel/src/components/panel.rs` | Zone, Center, Start, End |
| `crates/glimpse-services/src/broker.rs` | 0, SubscriptionId |
| `crates/glimpse-services/src/service.rs` | NoConfig, Config, Armed, SubKey, Config, ... |
| `crates/glimpse-services/src/services/calendar.rs` | SubKey, Watch, period, attempt, uri, ... |
| `crates/glimpse-services/src/services/compositor.rs` | reference, capabilities, Config, capabilities, SubKey, ... |
| `crates/glimpse-services/src/services/geolocation.rs` | SubKey, Command, Watch, Config, Geoclue, ... |
| `crates/glimpse-services/src/services/heartbeat.rs` | period_ms, Tick, Config, SubKey |
| `crates/glimpse-services/src/services/solar.rs` | Refresh, Command, Tick, Location, SubKey, ... |
| `crates/glimpse-services/src/testing.rs` | Config, First, Second, SubKey, Watch |
| `crates/glimpse-utils/src/args.rs` | socket, as_deref, SocketArg, ConfigArg, config, ... |
| `crates/glimpse-utils/src/log.rs` | Plain, init_app_tracing, level, format, LogFormat, ... |
| `crates/glimpse-widgets/examples/preview.rs` | cycle, repeat |
| `crates/glimpse-widgets/src/choice_list/mod.rs` | Choice, label, detail, icon_name |
| `crates/glimpse-widgets/src/event_list/mod.rs` | detail, color, when, summary, Event |
| `crates/glimpse-widgets/src/fact_list/mod.rs` | label, value, Fact |
| `crates/glimpse-widgets/src/forecast/mod.rs` | label, Hour, label, icon_name, precipitation, ... |
| `crates/glimpse-widgets/src/notice/imp.rs` | ParentType |
| `crates/glimpse-widgets/src/player_list/mod.rs` | artist, name, playing, Player, icon_name, ... |
| `crates/glimpse-widgets/src/row/imp.rs` | ParentType |
| `crates/glimpse-widgets/src/split_row/mod.rs` | detail |
| `crates/glimpse-widgets/src/transport/imp.rs` | action, Repeat, Previous, Repeat, constructed, ... |
| `crates/glimpse-widgets/src/workspace_list/mod.rs` | urgent, id, title, app_id, Window, ... |

## Connected Communities

- **src/applet +16 dirs** (3 cross-edges)
- **src/schema · new** (2 cross-edges)
- **src/hyprland · snapshot** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-25")
explore(operation:"context", task:"understand src/schema +26 dirs", format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
