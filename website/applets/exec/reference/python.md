# Python SDK

The `glimpse-applet-sdk` package, imported as `glimpse_sdk`. It needs Python 3.14 and
[`uv`](https://docs.astral.sh/uv/): the generated desktop entry runs the applet with `uv run`, which
installs the SDK on the first run, so `uv` must be on the machine that runs the applet. Elements and
their props are in [Elements](/applets/exec/reference/elements); this page covers how a Python
applet is put together.

::: tip Create an applet

```sh
glimpsectl applet new --language py --id me.aresa.GlimpseTodo --yes
cd glimpse-todo
just dev
```

[Start an applet](/applets/exec/start) explains each step and the files it creates.

:::

```python
from dataclasses import dataclass

from glimpse_sdk import BaseApplet, Context, Indicator, Popover, Section, State, ToggleRow, W


@dataclass(frozen=True)
class Todo(State):
    done: bool = False


class App(BaseApplet[Todo]):
    state = Todo()
    persist = ("done",)

    def build(self, ctx: Context) -> list[W]:
        return [
            Indicator(icon="view-list-symbolic", text="Todo"),
            Popover(
                title="Todo",
                children=[
                    Section(
                        children=[
                            ToggleRow(
                                title="Buy groceries",
                                active=self.state.done,
                                on_toggle=lambda active: self.set_state(done=active),
                            )
                        ]
                    )
                ],
            ),
        ]


if __name__ == "__main__":
    App.main()
```

## The applet

`BaseApplet[S]` is the base class. `S` is a frozen dataclass that subclasses `State`.

| Member | What it does |
| --- | --- |
| `state: S` | The current state. Set the initial value as a class attribute. |
| `persist: tuple[str, ...]` | State fields saved to `state.json` in the state directory and loaded before `start`. |
| `build(ctx) -> Sequence[W]` | Returns the whole widget tree. Called after every change; keep it free of side effects. |
| `async start(ctx)` | Runs once after the panel's `hello`, with options and the state directory filled in. Start background work here. |
| `set_state(**changes)` | Replaces fields of `state`, saves the persisted ones, and rebuilds if anything changed. Replace lists and dicts instead of changing them in place. |
| `App.main()` | Runs the applet. Exits 0 when the panel closes the socket. |

Element functions take props as keyword arguments in snake_case (`class_name`, `on_toggle`) and
children as positional arguments or `children=[...]`. `key=` keeps a row's identity when a list
reorders.

## The context

`ctx` is passed to `build` and `start`.

```python
class Context:
    name: str                      # this instance's name in the config
    state_dir: str                 # a directory for your files, mode 0700
    options: dict[str, Any]        # the config's options; updates without a restart
    placement: dict[str, Any] | None  # output, position, orientation, zone, size of the bar
    popover_open: bool             # whether the popover is showing
    entry: AppletEntry | None      # the desktop entry's id, name and icon

    def notify(self, summary: str, body: str | None = None, icon: str | None = None,
               urgency: Urgency | None = None) -> None: ...  # runs notify-send
    def copy(self, text: str) -> None: ...                   # runs wl-copy
    def open_uri(self, uri: str) -> None: ...
    def launch(self, desktop_id: str) -> None: ...
    def close_popover(self) -> None: ...
    def spawn(self, coro: Coroutine[Any, Any, T]) -> asyncio.Task[T]: ...
    def wake(self) -> None: ...  # asks for a rebuild; safe from any thread
```

`open_uri`, `launch` and `close_popover` ask the panel; see
[Notify, copy, open links and launch apps](/applets/exec/host). `spawn` runs a coroutine on the
applet's event loop and cancels it when the applet exits.

## Async work

The applet runs on an asyncio loop. Handlers may be `async def`. Long work (a timer, a network
call, a D-Bus watch) goes in `ctx.spawn(...)` from `start`; call `self.set_state(...)` or
`ctx.wake()` when it changes something. Never block `build` or a handler; wrap a blocking call in
`await asyncio.to_thread(fn)`.

## Exit codes

| Code | Cause |
| --- | --- |
| 0 | The panel closed the socket |
| 1 | A handler or spawned task raised, or `GLIMPSE_APPLET_SOCKET` is not set |
| 70 | The tree broke a protocol limit, or `build` raised |
