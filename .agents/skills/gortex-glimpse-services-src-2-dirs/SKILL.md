---
name: gortex-glimpse-services-src-2-dirs
description: "Work in the glimpse-services/src +2 dirs area — 139 symbols across 12 files (76% cohesion)"
---

# glimpse-services/src +2 dirs

139 symbols | 12 files | 76% cohesion

## When to Use

Use this skill when working on files in:
- `crates/glimpse-panel/src/applet/mod.rs`
- `crates/glimpse-panel/src/applet/runtime.rs`
- `crates/glimpse-services/src/context.rs`
- `crates/glimpse-services/src/publisher.rs`
- `crates/glimpse-services/src/service.rs`
- `crates/glimpse-services/src/services/calendar.rs`
- `crates/glimpse-services/src/services/compositor.rs`
- `crates/glimpse-services/src/services/geolocation.rs`
- `crates/glimpse-services/src/services/heartbeat.rs`
- `crates/glimpse-services/src/services/solar.rs`
- `crates/glimpse-services/src/subscription.rs`
- `crates/glimpse-services/src/testing.rs`

## Key Files

| File | Symbols |
|------|---------|
| `crates/glimpse-panel/src/applet/mod.rs` | configure, config, orient, view, start, ... |
| `crates/glimpse-panel/src/applet/runtime.rs` | Strip |
| `crates/glimpse-services/src/context.rs` | S, T, tasks, degraded, events, ... |
| `crates/glimpse-services/src/publisher.rs` | last, P, topic, topic, broker, ... |
| `crates/glimpse-services/src/service.rs` | handle, NAME, Event, Service, start, ... |
| `crates/glimpse-services/src/services/calendar.rs` | declared_topics_and_methods_exist |
| `crates/glimpse-services/src/services/compositor.rs` | Compositor, status, start, ctx, TOPICS, ... |
| `crates/glimpse-services/src/services/geolocation.rs` | declared_topics_and_methods_exist |
| `crates/glimpse-services/src/services/heartbeat.rs` | _config, TOPICS, declared_topics_and_methods_exist, count, Heartbeat, ... |
| `crates/glimpse-services/src/services/solar.rs` | TOPICS, coordinates, NAME, ctx, _config, ... |
| `crates/glimpse-services/src/subscription.rs` | key, map, topic, T |
| `crates/glimpse-services/src/testing.rs` | start, _config, handle, NAME, _input, ... |

## Connected Communities

- **src/calendar +8 dirs** (2 cross-edges)
- **src/services +3 dirs** (2 cross-edges)
- **src/schema +26 dirs** (1 cross-edges)
- **src/applet +16 dirs** (1 cross-edges)
- **glimpse-lock/src +15 dirs** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-104")
explore(operation:"context", task:"understand glimpse-services/src +2 dirs", format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
