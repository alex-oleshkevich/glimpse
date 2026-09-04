---
name: gortex-src-broker-handle
description: "Work in the src/broker · handle area — 60 symbols across 2 files (82% cohesion)"
---

# src/broker · handle

60 symbols | 2 files | 82% cohesion

## When to Use

Use this skill when working on files in:
- `crates/glimpsed/src/broker/mod.rs`
- `crates/glimpsed/src/broker/store.rs`

## Key Files

| File | Symbols |
|------|---------|
| `crates/glimpsed/src/broker/mod.rs` | handle, state, methods, topic, Publish, ... |
| `crates/glimpsed/src/broker/store.rs` | services |

## Connected Communities

- **glimpse-contracts/src +2 dirs · store** (11 cross-edges)
- **src/applet +16 dirs** (3 cross-edges)
- **glimpse-ipc/src +5 dirs** (2 cross-edges)
- **src/broker · send** (2 cross-edges)
- **glimpsed/src +2 dirs** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-187")
explore(operation:"context", task:"understand src/broker · handle", format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
