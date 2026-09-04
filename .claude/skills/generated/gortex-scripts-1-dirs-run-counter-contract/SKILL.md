---
name: gortex-scripts-1-dirs-run-counter-contract
description: "Work in the scripts +1 dirs · run_counter_contract area — 63 symbols across 5 files (92% cohesion)"
---

# scripts +1 dirs · run_counter_contract

63 symbols | 5 files | 92% cohesion

## When to Use

Use this skill when working on files in:
- ``
- `external-call::dep:dbus_next.aio.MessageBus`
- `scripts/mpris-fake-players.py`
- `scripts/printing-mock-cups.py`
- `scripts/sdk-e2e.py`

## Key Files

| File | Symbols |
|------|---------|
| `` | loads, Event, get, insert, strip, ... |
| `external-call::dep:dbus_next.aio.MessageBus` | dbus_next.aio.MessageBus |
| `scripts/mpris-fake-players.py` | run |
| `scripts/printing-mock-cups.py` | do_POST, _info, IppHandler, _current, build_ok, ... |
| `scripts/sdk-e2e.py` | env_overrides, send, socket_path, run_ipc_contract, __init__, ... |

## Connected Communities

- **. +1 dirs · attrs** (3 cross-edges)
- **scripts +1 dirs · default_config_path** (2 cross-edges)
- **demo/components +3 dirs** (2 cross-edges)
- **. +3 dirs · make_artwork** (1 cross-edges)
- **. +1 dirs · main · . · calendar-fake-event** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-231")
explore(operation:"context", task:"understand scripts +1 dirs · run_counter_contract", format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
