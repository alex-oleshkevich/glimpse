# glimpse-sdk for Python

Write a glimpse panel applet in Python. The applet is a Python program: the panel starts it as
a systemd user unit, and the SDK speaks the panel's NDJSON protocol on the unix socket named by
`GLIMPSE_APPLET_SOCKET`, which the panel sets; without it the applet exits with code 1. The wire format is specified
in [PROTOCOL.md](../sdk-ts/PROTOCOL.md) and is shared with the TypeScript SDK.

```python
from glimpse_sdk import Indicator, Popover, Row, Section, run


class App:
    def build(self, ctx):
        return [
            Indicator(icon="face-smile-symbolic", tooltip="Hello", children=["Hello"]),
            Popover(
                icon="face-smile-symbolic",
                title="Hello",
                children=[
                    Section(title="Hello", children=[Row(title="It works")]),
                ],
            ),
        ]


run(App())
```

## Model

- `build(ctx)` returns a fresh widget tree on every rebuild. Widgets are plain values: props
  flow down, callbacks (`on_toggle`, `on_change`, …) flow up.
- Handlers mutate state; the runtime rebuilds after every host message and diffs the new tree
  against the mounted one, emitting one protocol commit per rebuild.
- A widget keeps its identity across rebuilds when its kind and `key` match in the same parent.
  Keyed rows that reorder emit `move`, and their controlled-edit sequence follows them. A kind
  change at a slot unmounts and remounts. A move across parents is a remove plus an insert.
- Controlled props (`value`, `active`, `open`, `selected`) always come from state. An event's
  `seq` is copied onto the confirming `set`; an event older than the last applied `seq` is
  dropped; a rejected edit still confirms, so the panel restores the state value.
- `ctx` carries `name`, `state_dir`, `options`, `placement`, `popover_open`, and `entry`, plus
  the host requests (`open_uri`, `launch`, `close_popover`) and the client-side helpers
  (`notify`, `copy`), which spawn `notify-send` and `wl-copy` directly.
- Subclass `BaseApplet[YourState]` with a frozen-dataclass `state = YourState()` and `build(ctx)`;
  `Applet.main()` runs it. `set_state(field=value)` replaces the state, rebuilds when it changed,
  and writes the fields listed in `persist` to `state.json` (loaded before `start`). Call it on the
  loop thread; replace lists and dicts rather than mutating them.
- An app may define `async def start(self, ctx)`: it is spawned once after the host `hello`, with
  `ctx.options` and `state_dir` filled in. It is a one-shot setup hook: start long-running work
  from it with `ctx.spawn(coro)`; spawned tasks are cancelled when the applet exits. Load saved state and
  run tickers, watchers and network loops there; keep `build` free of side effects.
- The runtime is an asyncio loop. A handler may be `async def`, and `ctx.spawn(coro)` starts a
  background coroutine for network, D-Bus or timer work; both run on the loop, so they mutate state
  without locks. A rebuild follows each completion; a long-running task calls `ctx.wake()` after each state change. An exception in a spawned coroutine exits
  the applet with code 1, like a raising handler. Never block `build` or a handler: use an async
  library, or `await asyncio.to_thread(fn, ...)` for a blocking call. Plain threads call `ctx.wake()`
  after updating state; `wake()`, the host requests and the client-side helpers (`notify`,
  `copy`, …) are safe from any thread.
- Nothing builds before the host `hello`. A handler that raises exits with code 1 and a tree
  over a protocol limit with code 70; a closed socket exits 0.

## Distribution

An exec applet process reads its own directory, not a sibling SDK directory. Ship the
`glimpse_sdk/` package inside the applet directory (vendored, as in the kitchen sink example
and the Python `applets new` template) or install it into site-packages. The vendored package
stays byte-identical to `sdk/sdk-py/glimpse_sdk/`. The example desktop entry runs
`uv run applet.py`, and the file carries PEP 723 inline metadata with a `uv` shebang, so
`./applet.py` also runs it directly with no environment to set up.

## Example and tests

`examples/kitchen_sink/` exercises every element kind, every handler, host requests and the
client-side helpers, persistence, a one-second timer, and keyed reordering. Run it against an isolated
second panel — never the session bar — as described in `docs/live-testing.md`.

```
python3 -m unittest discover -s sdk/sdk-py/tests
```

The same model maps to Rust (handlers return messages that an `update` method applies, because
they cannot capture `&mut self`) and to Go (handlers are closures over the app pointer, widgets
are struct values built by helper functions). All three share the diff, `seq`, and commit rules
above and the protocol fixtures of the TypeScript SDK as the conformance oracle.
