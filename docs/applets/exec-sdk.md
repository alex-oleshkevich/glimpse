# Exec SDK

The Exec SDKs wrap the raw exec applet protocol with typed applet classes, state, render methods, widget builders, and event handlers.

Use this page when you want to build an applet with one of the SDK languages. For project scaffolding and live reload, read [Applet Tooling](../custom-applets/tooling.md). For config, protocol lines, component JSON, and event payloads, read [Exec Applet](../custom-applets/exec.md), [Line Protocol](../custom-applets/exec-protocol.md), and [Components](../custom-applets/exec-components.md).

## SDK Locations

| Language | Package | Source path |
|---|---|---|
| Python | `glimpse-applet-sdk` | `sdk/sdk-py` |
| TypeScript | `glimpse-sdk` | `sdk/sdk-ts` |
| Rust | `glimpse-sdk` | `sdk/sdk-rs` |
| Go | `github.com/alex-oleshkevich/glimpse/sdk/sdk-go` | `sdk/sdk-go` |

## Configure An SDK Applet

SDK applets run through an `exec` applet package. Point `command` at your SDK
program:

```toml
# ~/.config/glimpse/applets/counter.toml
id = "counter"
type = "exec"

[exec]
command = ["/home/alex/.config/glimpse/applets/counter"]

[exec.options]
start = 0
```

The SDK receives `options` during initialization and handles the line transport for you.

## Develop With `glimpse-shell applets`

The SDKs are easiest to use from an applet project directory:

```sh
glimpse-shell applets new counter --lang python
cd counter
glimpse-shell applets dev
```

`glimpse-shell applets dev` writes a temporary `~/.config/glimpse/applets/<id>.dev.toml`, watches source files, rebuilds when needed, restarts the child process, and replays the cached `init` line. Add `__dev__` to a panel section to display active dev applets:

```toml
[[panels]]
right = ["network", "__dev__", "battery"]
```

When the applet is ready for normal use, run:

```sh
glimpse-shell applets link
```

That symlinks the project's `applet.toml` into `~/.config/glimpse/applets`. Add the applet id to a panel section to keep it visible after the dev command exits.

## SDK Responsibilities

| Responsibility | Detail |
|---|---|
| State | Each SDK owns applet state and re-renders after state changes. |
| Status | `status` returns the full list of panel items for the applet. |
| Popover | `popover` returns the full widget tree or `None`/`null` when there is no content. |
| Events | Interactive widgets route `click`, `toggle`, `change`, and lifecycle events to handlers. |
| Actions | SDK action helpers emit `action` protocol lines for shell-side effects such as opening URIs, copying text, showing notifications, dismissing notifications, and closing the popover. |
| Transport | SDK runtimes own stdin/stdout protocol parsing and serialization. Applet diagnostics should go to stderr. |

## IPC Client

The IPC client lets applets listen to shell events and dispatch commands over the Glimpse socket.

`ipc()` / `IPC()` takes a service name (`"shell"` for the panel, the default). No connection is made until `listen` or `dispatch` is called. The socket path is `$GLIMPSE_IPC_DIR/<service>.sock`, or `$XDG_RUNTIME_DIR/glimpse/ipc.sock` for the shell.

- **`listen(channel)`** — subscribes to events by exact name, prefix pattern (`"audio.*"`), or wildcard (`"*"`). Returns an async stream of `Event` objects with `.name`, `.ts`, and `.fields`.
- **`dispatch(action, params)`** — sends a command and waits for acknowledgment, returning the ack fields.

**Python**

```python
from glimpse_sdk import ipc

async with app.background():
    sub = ipc("shell")
    async for event in await sub.listen("audio.*"):
        volume = event.fields.get("volume")
        await self.set_state(volume=int(volume or 0))
```

`ipc()` returns a `Subscriber`. `listen(channel)` is an async generator yielding `Event` objects. `dispatch(action, params)` sends a command and returns the ack fields.

**TypeScript**

```ts
import { ipc } from "glimpse-sdk";

const sub = ipc("shell"); // or ipc() — defaults to "shell"
for await (const event of sub.listen("audio.*")) {
  await this.setState({ volume: Number(event.fields.volume ?? 0) });
}
// Dispatch:
await sub.dispatch("set_volume", { level: "50" });
```

**Rust**

```rust
use glimpse_sdk::ipc;

let sub = ipc("shell")?;
let mut stream = sub.listen("audio.*").await?;
while let Some(event) = stream.next().await {
    let event = event?;
    // event.name, event.ts, event.fields
}
// Dispatch:
let _ack = sub.dispatch("set_volume", [("level", "50")]).await?;
```

**Go**

```go
sub := sdk.IPC("shell") // or sdk.IPC("") — both resolve to shell
ctx, cancel := context.WithCancel(ctx)
defer cancel()
events, err := sub.Listen(ctx, "audio.*")
for event := range events {
    // event.Name, event.Ts, event.Fields
}
// Dispatch:
ack, err := sub.Dispatch(ctx, "set_volume", map[string]string{"level": "50"})
```

## Python

Requires Python 3.14+. Install from PyPI:

```sh
pip install glimpse-applet-sdk
# or with uv:
uv add glimpse-applet-sdk
```

The distribution name is `glimpse-applet-sdk`; the import name is `glimpse_sdk`.

Minimal applet:

```python
from dataclasses import dataclass

from glimpse_sdk import Applet, AppletState, Button, ButtonVariant, Icon, StatusItem, click


@dataclass
class CounterState(AppletState):
    count: int = 0


class CounterApplet(Applet[CounterState]):
    def initial_state(self) -> CounterState:
        return CounterState()

    async def status(self, state: CounterState):
        return [
            StatusItem(
                id="counter",
                icon=Icon.name("view-refresh-symbolic"),
                label=str(state.count),
            )
        ]

    async def popover(self, state: CounterState):
        return Button(
            id="increment",
            label="Increment",
            icon="list-add-symbolic",
            variant=ButtonVariant.PRIMARY,
        )

    @click("increment")
    async def on_increment(self, _event) -> None:
        await self.set_state(count=self.state.count + 1)


if __name__ == "__main__":
    CounterApplet().run()
```

## TypeScript

Requires Node.js 20+. Install from npmjs.org:

```sh
npm install glimpse-sdk
```

Minimal applet:

```ts
import { Applet, Button, Icon, StatusItem, type TreeNode } from "glimpse-sdk";

interface CounterState {
  count: number;
}

class CounterApplet extends Applet<CounterState> {
  protected initialState(): CounterState {
    return { count: 0 };
  }

  constructor() {
    super();
    this.onClick("increment", async () => {
      await this.setState({ count: this.state.count + 1 });
    });
  }

  protected async status(state: CounterState): Promise<StatusItem[]> {
    return [
      new StatusItem({
        id: "counter",
        icon: Icon.name("view-refresh-symbolic"),
        label: String(state.count),
      }),
    ];
  }

  protected async popover(_state: CounterState): Promise<TreeNode | null> {
    return new Button({
      id: "increment",
      label: "Increment",
      icon: "list-add-symbolic",
      variant: "primary",
    });
  }
}

await new CounterApplet().run();
```

## Rust

Add the SDK from crates.io:

```toml
[dependencies]
async-trait = "0.1"
glimpse-sdk = "0.3"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Minimal applet:

```rust
use async_trait::async_trait;
use glimpse_sdk::{
    Applet, AppletResult, Button, ButtonVariant, CallbackEvent, Icon, StatusItem, TreeNode, run,
};

#[derive(Debug, Clone, Default)]
struct CounterState {
    count: u32,
}

struct CounterApplet;

#[async_trait]
impl Applet for CounterApplet {
    type State = CounterState;

    async fn status(&self, state: &Self::State) -> AppletResult<Vec<StatusItem>> {
        Ok(vec![
            StatusItem::new("counter")
                .icon(Icon::name("view-refresh-symbolic"))
                .label(state.count.to_string()),
        ])
    }

    async fn popover(&self, _state: &Self::State) -> AppletResult<Option<TreeNode>> {
        Ok(Some(
            Button::new("increment")
                .label("Increment")
                .icon("list-add-symbolic")
                .variant(ButtonVariant::Primary)
                .into(),
        ))
    }

    async fn on_callback(
        &mut self,
        state: &mut Self::State,
        event: CallbackEvent,
    ) -> AppletResult<()> {
        if let CallbackEvent::Click(click) = event {
            if click.id == "increment" {
                state.count += 1;
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> AppletResult<()> {
    run(CounterApplet, CounterState::default()).await
}
```

## Go

Add the SDK module:

```sh
go get github.com/alex-oleshkevich/glimpse/sdk/sdk-go
```

Minimal applet:

```go
package main

import (
	"context"
	"fmt"

	sdk "github.com/alex-oleshkevich/glimpse/sdk/sdk-go/sdk"
)

type counterState struct {
	Count int
}

type counterApplet struct {
	sdk.BaseApplet[counterState]
}

func newCounterApplet() *counterApplet {
	return &counterApplet{BaseApplet: sdk.NewBaseApplet(counterState{})}
}

func (a *counterApplet) OnStart(context.Context) error               { return nil }
func (a *counterApplet) OnInit(context.Context, sdk.InitEvent) error { return nil }

func (a *counterApplet) OnCallback(_ context.Context, event sdk.CallbackEvent) error {
	if click, ok := event.(sdk.ClickEvent); ok && click.ID == "increment" {
		a.SetState(func(state *counterState) {
			state.Count++
		})
	}
	return nil
}

func (a *counterApplet) Status(_ context.Context, state *counterState) ([]sdk.StatusItem, error) {
	return []sdk.StatusItem{
		{
			ID:    "counter",
			Icon:  sdk.IconName("view-refresh-symbolic"),
			Label: fmt.Sprintf("%d", state.Count),
		},
	}, nil
}

func (a *counterApplet) Popover(_ context.Context, _ *counterState) (sdk.Widget, error) {
	return sdk.Button{
		CommonProps: sdk.CommonProps{ID: "increment"},
		Label:       "Increment",
		Icon:        "list-add-symbolic",
		Variant:     sdk.ButtonVariantPrimary,
	}, nil
}

func main() {
	if err := sdk.Run[counterState](context.Background(), newCounterApplet()); err != nil {
		panic(err)
	}
}
```

## Golden Fixture Workflow

The four SDKs share canonical JSON fixtures under `sdk/fixtures`. Use them when adding widgets, events, common props, or action helpers.

| Check | Command |
|---|---|
| Rust SDK fixture tests | `cargo test` in `sdk/sdk-rs` |
| TypeScript SDK fixture tests | `npm test` in `sdk/sdk-ts` |
| Python SDK fixture tests | `python -m pytest` in `sdk/sdk-py` |
| Go SDK fixture tests | `go test ./sdk` in `sdk/sdk-go` |
| Rust renderer fixture test | `cargo test -p glimpse-shell golden_widget_fixtures_render_without_errors -- --nocapture` from the repo root |

Fixture rules:

- Widget fixtures must match every SDK serializer.
- Event fixtures must match every SDK parser.
- The Rust renderer fixture test must be able to deserialize and render every widget fixture without a renderer error.
- If a fixture and an SDK disagree, fix the SDK unless the fixture violates the documented protocol.
- Interactive renderer widgets that emit events require stable `id` fields.

## See Also

| Page | Covers |
|---|---|
| [Exec Applet](../custom-applets/exec.md) | Applet config and options. |
| [Applet Tooling](../custom-applets/tooling.md) | `glimpse-shell applets` project, dev, link, and diagnostics workflows. |
| [Line Protocol](../custom-applets/exec-protocol.md) | Raw protocol commands, message shapes, and events. |
| [Components](../custom-applets/exec-components.md) | Popover component fields and component types. |
| [SDK Golden Fixtures](https://github.com/alex-oleshkevich/glimpse/blob/master/sdk/fixtures/README.md) | Cross-language fixture rules and validation commands. |
