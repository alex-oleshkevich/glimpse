---
name: gortex-src-applet-16-dirs
description: "Work in the src/applet +16 dirs area — 172 symbols across 23 files (60% cohesion)"
---

# src/applet +16 dirs

172 symbols | 23 files | 60% cohesion

## When to Use

Use this skill when working on files in:
- `crates/glimpse-config/src/watch.rs`
- `crates/glimpse-dbus/src/clients/notifications.rs`
- `crates/glimpse-ipc/src/client.rs`
- `crates/glimpse-ipc/src/server.rs`
- `crates/glimpse-panel/src/app.rs`
- `crates/glimpse-panel/src/applet/mod.rs`
- `crates/glimpse-panel/src/applet/popover.rs`
- `crates/glimpse-panel/src/applet/runtime.rs`
- `crates/glimpse-panel/src/applets/clock/indicator.rs`
- `crates/glimpse-panel/src/applets/clock/popover.rs`
- `crates/glimpse-panel/src/applets/pager/mod.rs`
- `crates/glimpse-panel/src/components/panel.rs`
- `crates/glimpse-services/src/broker.rs`
- `crates/glimpse-services/src/context.rs`
- `crates/glimpse-services/src/services/calendar.rs`
- `crates/glimpse-widgets/src/event_list/mod.rs`
- `crates/glimpse-widgets/src/indicator_group/imp.rs`
- `crates/glimpse-widgets/src/indicator_group/mod.rs`
- `crates/glimpse-widgets/src/panel/mod.rs`
- `crates/glimpse-widgets/src/player_list/row.rs`
- `crates/glimpse-widgets/src/reconcile.rs`
- `crates/glimpse-widgets/src/world_clock/mod.rs`
- `crates/glimpsectl/src/render.rs`

## Key Files

| File | Symbols |
|------|---------|
| `crates/glimpse-config/src/watch.rs` | capture |
| `crates/glimpse-dbus/src/clients/notifications.rs` | actions_parse_as_key_label_pairs_and_drop_incomplete_tail, parse_actions, actions |
| `crates/glimpse-ipc/src/client.rs` | event, offer, fan_out, event |
| `crates/glimpse-ipc/src/server.rs` | registry, read, event, publish |
| `crates/glimpse-panel/src/app.rs` | list_gdk_monitors, reconcile_panels, panels, key, config, ... |
| `crates/glimpse-panel/src/applet/mod.rs` | until_boundary, output, name, handle, topics, ... |
| `crates/glimpse-panel/src/applet/popover.rs` | host, client, name, new |
| `crates/glimpse-panel/src/applet/runtime.rs` | sender, name, client, Init, view, ... |
| `crates/glimpse-panel/src/applets/clock/indicator.rs` | config, configure, ctx |
| `crates/glimpse-panel/src/applets/clock/popover.rs` | zones, configured |
| `crates/glimpse-panel/src/applets/pager/mod.rs` | view, workspace, rows, ctx, windows_on |
| `crates/glimpse-panel/src/components/panel.rs` | size, Config, config, slot, orientation, ... |
| `crates/glimpse-services/src/broker.rs` | subscribe, subscribing_to_a_topic_with_no_value_replays_nothing, topic, sink, topic, ... |
| `crates/glimpse-services/src/context.rs` | clone |
| `crates/glimpse-services/src/services/calendar.rs` | publish, subscriptions |
| `crates/glimpse-widgets/src/event_list/mod.rs` | sync_overflow, hidden |
| `crates/glimpse-widgets/src/indicator_group/imp.rs` | constructed |
| `crates/glimpse-widgets/src/indicator_group/mod.rs` | default, items, connect_scrolled, f, f, ... |
| `crates/glimpse-widgets/src/panel/mod.rs` | clear_center, widget, widget, section, append_to_start, ... |
| `crates/glimpse-widgets/src/player_list/row.rs` | constructed |
| `crates/glimpse-widgets/src/reconcile.rs` | build, K, apply, key, held, ... |
| `crates/glimpse-widgets/src/world_clock/mod.rs` | label, Zone, icon_name, timezone, note |
| `crates/glimpsectl/src/render.rs` | path, value, width, render, text, ... |

## Entry Points

- `crates/glimpse-panel/src/applet/runtime.rs::AppletRuntime.init`
- `crates/glimpse-services/src/broker.rs::subscribing_after_a_publish_replays_the_latest_value`

## Connected Communities

- **glimpse-widgets/src +29 dirs** (6 cross-edges)
- **src/applet · update** (5 cross-edges)
- **src/calendar +8 dirs** (4 cross-edges)
- **glimpse-lock/src +15 dirs** (4 cross-edges)
- **src/broker +5 dirs** (3 cross-edges)
- **src/schema +26 dirs** (2 cross-edges)
- **glimpse-ipc/src +5 dirs** (2 cross-edges)
- **src/commands +2 dirs** (1 cross-edges)
- **applets/clock · formatted** (1 cross-edges)
- **glimpse-ipc/src · session** (1 cross-edges)
- **applets/clock · period** (1 cross-edges)
- **glimpse-services/src · stream · service** (1 cross-edges)
- **glimpse-ipc/src · serve_client** (1 cross-edges)
- **glimpse-ipc/src +8 dirs** (1 cross-edges)
- **src/clients +1 dirs · lock** (1 cross-edges)
- **glimpse-contracts/src +2 dirs · call** (1 cross-edges)
- **src/schema +2 dirs** (1 cross-edges)
- **src/calendar_popover +2 dirs** (1 cross-edges)
- **glimpse-widgets/src +7 dirs** (1 cross-edges)
- **applets/pager · workspace_slot** (1 cross-edges)
- **glimpse-config/src +1 dirs** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-102")
explore(operation:"context", task:"understand src/applet +16 dirs", format:"gcx")
relations(operation:"usages", target:{symbol:"crates/glimpse-panel/src/applet/runtime.rs::AppletRuntime.init"}, format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
