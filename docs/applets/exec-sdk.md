# Exec SDK

The Exec SDKs wrap the raw exec applet protocol with typed applet classes, state, render methods, widget builders, and event handlers.

Use this page when you want to build an applet with one of the SDK languages. For config, protocol lines, component JSON, and event payloads, read [Exec Applet](../custom-applets/exec.md), [Line Protocol](../custom-applets/exec-protocol.md), and [Components](../custom-applets/exec-components.md).

## SDK Locations

| Language | Package | Source path |
|---|---|---|
| Python | `glimpse-applet-sdk` | `sdk/sdk-py` |
| TypeScript | `glimpse-sdk` | `sdk/sdk-ts` |
| Rust | `glimpse-sdk` | `sdk/sdk-rs` |
| Go | `github.com/alex-oleshkevich/glimpse/sdk/sdk-go` | `sdk/sdk-go` |

## Configure An SDK Applet

SDK applets still run through the built-in `exec` applet. Point `command` at your SDK program:

```toml
[applets.counter]
extends = "exec"
command = ["/home/alex/.config/glimpse/applets/counter"]

[applets.counter.options]
start = 0
```

The SDK receives `options` during initialization and handles the line transport for you.

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

from glimpse_sdk import Applet, AppletState, Button, Icon, RenderResult, StatusItem, click


@dataclass
class CounterState(AppletState):
    count: int = 0


class CounterApplet(Applet[CounterState]):
    def initial_state(self) -> CounterState:
        return CounterState()

    async def render(self) -> RenderResult:
        return RenderResult(
            status=[
                StatusItem(
                    id="counter",
                    icon=Icon.name("view-refresh-symbolic"),
                    label=str(self.state.count),
                )
            ],
            tree=Button(id="increment", label="Increment"),
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
import { Applet, Button, Icon, RenderResult, StatusItem } from "glimpse-sdk";

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

  protected async render(): Promise<RenderResult> {
    return new RenderResult({
      status: [
        new StatusItem({
          id: "counter",
          icon: Icon.name("view-refresh-symbolic"),
          label: String(this.state.count),
        }),
      ],
      tree: new Button({ id: "increment", label: "Increment" }),
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
glimpse-sdk = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Minimal applet:

```rust
use async_trait::async_trait;
use glimpse_sdk::{
    Applet, AppletResult, Button, Icon, RenderResult, StateStore, StatusItem, TreeNode, run,
};

#[derive(Debug, Clone, Default)]
struct CounterState {
    count: u32,
}

struct CounterApplet {
    store: StateStore<CounterState>,
}

#[async_trait]
impl Applet for CounterApplet {
    type State = CounterState;

    fn store(&self) -> &StateStore<Self::State> {
        &self.store
    }

    fn store_mut(&mut self) -> &mut StateStore<Self::State> {
        &mut self.store
    }

    async fn render(&self) -> AppletResult<RenderResult> {
        Ok(RenderResult {
            status: vec![StatusItem::new("counter")
                .icon(Icon::name("view-refresh-symbolic"))
                .label(self.state().count.to_string())],
            tree: Some(TreeNode::from(Button::new("increment").label("Increment"))),
        })
    }

    async fn on_callback(
        &mut self,
        event: glimpse_sdk::CallbackEvent,
    ) -> AppletResult<()> {
        if let glimpse_sdk::CallbackEvent::Click(click) = event {
            if click.id == "increment" {
                self.set_state(|state| state.count += 1);
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> AppletResult<()> {
    run(CounterApplet {
        store: StateStore::new(CounterState::default()),
    })
    .await
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

func (a *counterApplet) Render(context.Context) (sdk.RenderResult, error) {
	return sdk.RenderResult{
		Status: []sdk.StatusItem{
			{
				ID:    "counter",
				Icon:  sdk.IconName("view-refresh-symbolic"),
				Label: fmt.Sprintf("%d", a.State().Count),
			},
		},
		Tree: sdk.Button{
			CommonProps: sdk.CommonProps{ID: "increment"},
			Label:       "Increment",
		},
	}, nil
}

func main() {
	if err := sdk.Run[counterState](context.Background(), newCounterApplet()); err != nil {
		panic(err)
	}
}
```

## See Also

| Page | Covers |
|---|---|
| [Exec Applet](../custom-applets/exec.md) | Applet config and options. |
| [Line Protocol](../custom-applets/exec-protocol.md) | Raw protocol commands, message shapes, and events. |
| [Components](../custom-applets/exec-components.md) | Popover component fields and component types. |
