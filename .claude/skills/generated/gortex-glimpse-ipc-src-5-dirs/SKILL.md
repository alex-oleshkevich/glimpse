---
name: gortex-glimpse-ipc-src-5-dirs
description: "Work in the glimpse-ipc/src +5 dirs area — 162 symbols across 11 files (74% cohesion)"
---

# glimpse-ipc/src +5 dirs

162 symbols | 11 files | 74% cohesion

## When to Use

Use this skill when working on files in:
- `crates/glimpse-ipc/src/client.rs`
- `crates/glimpse-ipc/src/frame.rs`
- `crates/glimpse-ipc/src/outbox.rs`
- `crates/glimpse-ipc/src/server.rs`
- `crates/glimpse-services/src/broker.rs`
- `crates/glimpse-services/src/service.rs`
- `crates/glimpse-services/src/services/compositor.rs`
- `crates/glimpse-services/src/services/heartbeat.rs`
- `crates/glimpsectl/src/errors.rs`
- `crates/glimpsed/src/broker/mod.rs`
- `crates/glimpsed/src/handler.rs`

## Key Files

| File | Symbols |
|------|---------|
| `crates/glimpse-ipc/src/client.rs` | Client, command, gone, fmt, matched, ... |
| `crates/glimpse-ipc/src/frame.rs` | NotReady, Unsupported, message, Unavailable, UnknownCommand, ... |
| `crates/glimpse-ipc/src/outbox.rs` | pattern, remove_pattern |
| `crates/glimpse-ipc/src/server.rs` | H, id, Get, correlation, reply_subscribe, ... |
| `crates/glimpse-services/src/broker.rs` | reply, a_responder_that_answered_does_not_answer_again_on_drop, a_dropped_responder_answers_instead_of_leaving_the_caller_waiting, reply, outcome, ... |
| `crates/glimpse-services/src/service.rs` | Command, T, args, unknown_command, dispatch, ... |
| `crates/glimpse-services/src/services/compositor.rs` | error, a_capability_a_compositor_lacks_is_refused_without_inviting_a_retry, code |
| `crates/glimpse-services/src/services/heartbeat.rs` | responder, set_interval, period_ms |
| `crates/glimpsectl/src/errors.rs` | exit, Unknown, Timeout, Unreachable, exit, ... |
| `crates/glimpsed/src/broker/mod.rs` | call, method, responder, args |
| `crates/glimpsed/src/handler.rs` | topic, settle, client, args, disconnected, ... |

## Connected Communities

- **glimpse-lock/src +15 dirs** (5 cross-edges)
- **glimpse-ipc/src · push_response** (4 cross-edges)
- **src/broker · send** (3 cross-edges)
- **glimpsed/src +2 dirs** (2 cross-edges)
- **glimpse-contracts/src +2 dirs · store** (2 cross-edges)
- **glimpse-ipc/src · Body** (1 cross-edges)
- **glimpse-ipc/src · frame** (1 cross-edges)
- **src/clients +1 dirs · lock** (1 cross-edges)
- **src/applet +16 dirs** (1 cross-edges)
- **glimpse-widgets/src +7 dirs** (1 cross-edges)
- **glimpse-ipc/src · serve_client** (1 cross-edges)
- **glimpse-services/src · stream · context** (1 cross-edges)
- **glimpse-config/src +4 dirs** (1 cross-edges)
- **src/broker +5 dirs** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-71")
explore(operation:"context", task:"understand glimpse-ipc/src +5 dirs", format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
