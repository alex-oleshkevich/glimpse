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

## See Also

| Page | Covers |
|---|---|
| [Exec Applet](../custom-applets/exec.md) | Applet config and options. |
| [Line Protocol](../custom-applets/exec-protocol.md) | Raw protocol commands, message shapes, and events. |
| [Components](../custom-applets/exec-components.md) | Popover component fields and component types. |
