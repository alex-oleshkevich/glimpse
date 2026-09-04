---
name: gortex-glimpse-widgets-transport
description: "Work in the glimpse-widgets · Transport area — 74 symbols across 6 files (77% cohesion)"
---

# glimpse-widgets · Transport

74 symbols | 6 files | 77% cohesion

## When to Use

Use this skill when working on files in:
- `crates/glimpse-widgets/examples/preview.rs`
- `crates/glimpse-widgets/src/now_playing/mod.rs`
- `crates/glimpse-widgets/src/player_list/imp.rs`
- `crates/glimpse-widgets/src/scrubber/mod.rs`
- `crates/glimpse-widgets/src/transport/imp.rs`
- `crates/glimpse-widgets/src/transport/mod.rs`

## Key Files

| File | Symbols |
|------|---------|
| `crates/glimpse-widgets/examples/preview.rs` | Media, forward, media, r, root, ... |
| `crates/glimpse-widgets/src/now_playing/mod.rs` | transport, scrubber |
| `crates/glimpse-widgets/src/player_list/imp.rs` | NAME, class_init, rows, klass, players, ... |
| `crates/glimpse-widgets/src/scrubber/mod.rs` | f, connect_seek, F |
| `crates/glimpse-widgets/src/transport/imp.rs` | class_init, playing, Transport, set_can_next, enabled, ... |
| `crates/glimpse-widgets/src/transport/mod.rs` | connect_action, F, f |

## Connected Communities

- **glimpse-widgets/src +29 dirs** (7 cross-edges)
- **glimpse-widgets · find** (6 cross-edges)
- **src/now_playing** (4 cross-edges)
- **src/scrubber** (2 cross-edges)
- **src/schema +26 dirs** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-131")
explore(operation:"context", task:"understand glimpse-widgets · Transport", format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
