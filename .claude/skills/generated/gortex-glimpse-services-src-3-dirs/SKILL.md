---
name: gortex-glimpse-services-src-3-dirs
description: "Work in the glimpse-services/src +3 dirs area — 92 symbols across 10 files (72% cohesion)"
---

# glimpse-services/src +3 dirs

92 symbols | 10 files | 72% cohesion

## When to Use

Use this skill when working on files in:
- `crates/glimpse-dbus/src/dbus.rs`
- `crates/glimpse-services/src/broker.rs`
- `crates/glimpse-services/src/context.rs`
- `crates/glimpse-services/src/service.rs`
- `crates/glimpse-services/src/services/calendar.rs`
- `crates/glimpse-services/src/services/geolocation.rs`
- `crates/glimpse-services/src/services/heartbeat.rs`
- `crates/glimpse-services/src/services/solar.rs`
- `crates/glimpse-services/src/testing.rs`
- `crates/glimpsed/src/daemon.rs`

## Key Files

| File | Symbols |
|------|---------|
| `crates/glimpse-dbus/src/dbus.rs` | unavailable, session_bus, system, reason, Buses, ... |
| `crates/glimpse-services/src/broker.rs` | data, health, report_health, _id, service, ... |
| `crates/glimpse-services/src/context.rs` | a_burst_collapses_to_its_newest_payload, cancel, an_undecodable_payload_leaves_the_subscription_running, buses, shutdown, ... |
| `crates/glimpse-services/src/service.rs` | cancel, run, S, sender, broker, ... |
| `crates/glimpse-services/src/services/calendar.rs` | mock, running, a_calendar_with_no_sources_publishes_an_empty_list_and_stays_healthy, sources, published, ... |
| `crates/glimpse-services/src/services/geolocation.rs` | published, a_geoclue_event_arriving_after_a_switch_to_manual_is_ignored, mock |
| `crates/glimpse-services/src/services/heartbeat.rs` | set_interval_reports_the_period_it_replaced, command, call, set_interval_refuses_a_period_outside_the_supported_range |
| `crates/glimpse-services/src/services/solar.rs` | located, phases, coordinates, mock, a_location_publishes_a_phase_without_waiting_for_a_tick, ... |
| `crates/glimpse-services/src/testing.rs` | wired_probe, Ping, value |
| `crates/glimpsed/src/daemon.rs` | allows, register, S, name |

## Entry Points

- `crates/glimpse-services/src/services/geolocation.rs::a_geoclue_event_arriving_after_a_switch_to_manual_is_ignored`
- `crates/glimpse-services/src/service.rs::a_source_declared_by_a_handler_is_started_by_the_runtime`
- `crates/glimpse-services/src/service.rs::a_panicking_handler_stops_its_own_service`
- `crates/glimpse-services/src/service.rs::a_service_without_a_bus_degrades_and_keeps_running`
- `crates/glimpse-services/src/services/calendar.rs::a_source_that_cannot_be_read_degrades_without_naming_its_uri`

## Connected Communities

- **src/applet +16 dirs** (18 cross-edges)
- **src/broker +5 dirs** (5 cross-edges)
- **glimpse-services/src · event** (5 cross-edges)
- **glimpse-ipc/src +5 dirs** (3 cross-edges)
- **glimpse-services/src · stream · context** (3 cross-edges)
- **glimpse-lock/src +15 dirs** (3 cross-edges)
- **src/services · Config** (2 cross-edges)
- **glimpsed/src +2 dirs** (2 cross-edges)
- **src/hyprland +3 dirs** (1 cross-edges)
- **glimpse-widgets/src +29 dirs** (1 cross-edges)
- **src/services +3 dirs** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-105")
explore(operation:"context", task:"understand glimpse-services/src +3 dirs", format:"gcx")
relations(operation:"usages", target:{symbol:"crates/glimpse-services/src/services/geolocation.rs::a_geoclue_event_arriving_after_a_switch_to_manual_is_ignored"}, format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
