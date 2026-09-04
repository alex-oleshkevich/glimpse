---
name: gortex-src-commands-2-dirs
description: "Work in the src/commands +2 dirs area — 94 symbols across 9 files (82% cohesion)"
---

# src/commands +2 dirs

94 symbols | 9 files | 82% cohesion

## When to Use

Use this skill when working on files in:
- `crates/glimpse-config/src/schema/applets.rs`
- `crates/glimpsectl/src/commands/call.rs`
- `crates/glimpsectl/src/commands/get.rs`
- `crates/glimpsectl/src/commands/methods.rs`
- `crates/glimpsectl/src/commands/mod.rs`
- `crates/glimpsectl/src/commands/services.rs`
- `crates/glimpsectl/src/commands/topics.rs`
- `crates/glimpsectl/src/commands/watch.rs`
- `crates/glimpsectl/src/render.rs`

## Key Files

| File | Symbols |
|------|---------|
| `crates/glimpse-config/src/schema/applets.rs` | table, name, deserializer, deserialize, D, ... |
| `crates/glimpsectl/src/commands/call.rs` | call, arguments, method, session |
| `crates/glimpsectl/src/commands/get.rs` | json, topic, session, field, get |
| `crates/glimpsectl/src/commands/methods.rs` | session, owner, methods, pattern |
| `crates/glimpsectl/src/commands/mod.rs` | field, absent, empty_reason, pattern, a_pattern_filters_the_keys_of_the_named_object, ... |
| `crates/glimpsectl/src/commands/services.rs` | session, services |
| `crates/glimpsectl/src/commands/topics.rs` | session, pattern, owner, topics |
| `crates/glimpsectl/src/commands/watch.rs` | pattern, watch, json, session, count |
| `crates/glimpsectl/src/render.rs` | headers, row, Stop, default, headers, ... |

## Connected Communities

- **src/applet +16 dirs** (9 cross-edges)
- **glimpse-lock/src +15 dirs** (2 cross-edges)
- **src/applet +5 dirs** (2 cross-edges)
- **glimpse-contracts/src +2 dirs · store** (1 cross-edges)
- **glimpsed/src +2 dirs** (1 cross-edges)
- **glimpse-ipc/src +4 dirs** (1 cross-edges)
- **glimpsectl/src · ansi** (1 cross-edges)
- **glimpse-ipc/src +5 dirs** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-182")
explore(operation:"context", task:"understand src/commands +2 dirs", format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
