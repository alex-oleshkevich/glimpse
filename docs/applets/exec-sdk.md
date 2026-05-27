# Exec SDK

The Exec SDKs wrap the raw exec applet protocol with typed applet classes,
state, render methods, widget builders, events, and action helpers.

Use this page when you want to understand the SDK shape in each language. Use
[Applet Tooling](../custom-applets/tooling.md) to create, run, and link applet
projects.

## SDK Locations

| Language | Package | Source path |
|---|---|---|
| Python | `glimpse-applet-sdk` | `sdk/sdk-py` |
| TypeScript | `glimpse-sdk` | `sdk/sdk-ts` |
| Rust | `glimpse-sdk` | `sdk/sdk-rs` |
| Go | `github.com/alex-oleshkevich/glimpse/sdk/sdk-go` | `sdk/sdk-go` |

Generated projects include the right language manifest and dependency entries.
Do not start by hand-writing package files; start with the applet tooling:

```sh
glimpse-shell applets new counter --lang python
cd counter
glimpse-shell applets dev
```

Then link the project when it is ready for local use:

```sh
glimpse-shell applets link
```

For distribution, share an `applet.toml` with the executable or script. See
[Applet Tooling](../custom-applets/tooling.md) for the package shape.

## Applet Package Shape

The tooling creates an `applet.toml` package for the exec host. A typical
generated package looks like this:

```toml
id = "counter"
type = "exec"

[exec]
command = ["uv", "run", "main.py"]

[exec.options]
start = 0
```

`[exec.options]` is passed to the SDK during initialization. Keep local applet
settings there instead of hardcoding them in your program.

## SDK Responsibilities

| Responsibility | Detail |
|---|---|
| State | Each SDK owns applet state and re-renders after state changes. |
| Status | `status` returns the full list of panel items for the applet. |
| Popover | `popover` returns the full widget tree or `None`/`null` when there is no content. |
| Events | Interactive widgets route `click`, `toggle`, `change`, and lifecycle events to handlers. |
| Actions | SDK action helpers emit shell-side effects such as opening URIs, copying text, showing notifications, dismissing notifications, and closing the popover. |
| Transport | SDK runtimes own stdin/stdout protocol parsing and serialization. Applet diagnostics should go to stderr. |

## Python

The distribution name is `glimpse-applet-sdk`; the import name is
`glimpse_sdk`. A generated Python counter applet uses this shape:

```python
from __future__ import annotations

from dataclasses import dataclass

from glimpse_sdk import Applet, AppletState, Column, Hero, Label, StatusItem, Tile


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
                icon="view-refresh-symbolic",
                label=str(state.count),
            )
        ]

    async def popover(self, state: CounterState):
        return Column(
            children=[
                Hero(
                    icon="view-refresh-symbolic",
                    title="Counter",
                    subtitle=f"Value: {state.count}",
                ),
                Label(label=f"Count = {state.count}"),
                Tile(
                    primary="Increment",
                    left_icon="list-add-symbolic",
                    on_click=self.on_increment,
                ),
            ],
        )

    async def on_increment(self, state: CounterState, _event) -> None:
        await self.set_state(count=state.count + 1)


if __name__ == "__main__":
    CounterApplet().run()
```

## TypeScript

The package name is `glimpse-sdk`. A generated TypeScript counter applet uses
this shape:

```ts
import {
  Applet,
  Column,
  Hero,
  Label,
  StatusItem,
  Tile,
  type TreeNode,
} from "glimpse-sdk";

interface CounterState {
  count: number;
}

class CounterApplet extends Applet<CounterState> {
  constructor() {
    super();
  }

  protected initialState(): CounterState {
    return { count: 0 };
  }

  protected async status(state: CounterState): Promise<StatusItem[]> {
    return [
      new StatusItem({
        id: "counter",
        icon: "view-refresh-symbolic",
        label: String(state.count),
      }),
    ];
  }

  protected async popover(state: CounterState): Promise<TreeNode | null> {
    return new Column({
      children: [
        new Hero({
          icon: "view-refresh-symbolic",
          title: "Counter",
          subtitle: `Value: ${state.count}`,
        }),
        new Label(`Count = ${state.count}`),
        new Tile({
          primary: "Increment",
          left_icon: "list-add-symbolic",
          on_click: async () => {
            await this.setState({ count: this.state.count + 1 });
          },
        }),
      ],
    });
  }
}

void new CounterApplet().run();
```

## Rust

The crate name is `glimpse-sdk`. A generated Rust counter applet uses typed
messages for interaction:

```rust
use async_trait::async_trait;
use glimpse_sdk::{
    Applet, AppletResult, Column, Hero, Label, MsgMapper, StatusItem, Tile, TreeNode, run, tree,
};

#[derive(Debug, Clone, Default)]
struct CounterState {
    count: u32,
}

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Increment,
}

struct CounterApplet;

#[async_trait]
impl Applet for CounterApplet {
    type State = CounterState;
    type Msg = Msg;

    async fn status(&self, state: &Self::State) -> AppletResult<Vec<StatusItem>> {
        Ok(vec![
            StatusItem::new("counter")
                .icon("view-refresh-symbolic")
                .label(state.count.to_string()),
        ])
    }

    async fn update(&mut self, state: &mut CounterState, msg: Msg) -> AppletResult<()> {
        if msg == Msg::Increment {
            state.count += 1;
        }
        Ok(())
    }

    async fn popover(&self, state: &Self::State) -> AppletResult<Option<TreeNode<Msg>>> {
        Ok(Some(
            Column::new(tree![
                {
                    let mut hero = Hero::new("Counter", format!("Value: {}", state.count));
                    hero.icon = Some("view-refresh-symbolic".into());
                    hero
                },
                Label::new(format!("Count = {}", state.count)),
                {
                    let mut tile = Tile::new("Increment");
                    tile.left_icon = Some("list-add-symbolic".into());
                    tile.on_click = Some(MsgMapper::new(|()| Msg::Increment));
                    tile
                },
            ])
            .into(),
        ))
    }
}

#[tokio::main]
async fn main() -> AppletResult<()> {
    run(CounterApplet, CounterState::default()).await
}
```

## Go

The Go SDK module is `github.com/alex-oleshkevich/glimpse/sdk/sdk-go`. A
generated Go counter applet embeds `BaseApplet` and implements `Status` and
`Popover`.

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
	return &counterApplet{
		BaseApplet: sdk.NewBaseApplet(counterState{}),
	}
}

func (a *counterApplet) Status(_ context.Context, state *counterState) ([]sdk.StatusItem, error) {
	return []sdk.StatusItem{
		{
			ID:    "counter",
			Icon:  "view-refresh-symbolic",
			Label: fmt.Sprintf("%d", state.Count),
		},
	}, nil
}

func (a *counterApplet) Popover(_ context.Context, state *counterState) (sdk.Widget, error) {
	return sdk.Column{
		Children: []sdk.Widget{
			sdk.Hero{Title: "Counter", Subtitle: fmt.Sprintf("Value: %d", state.Count)},
			sdk.Label{Label: fmt.Sprintf("Count = %d", state.Count)},
			sdk.Tile{
				Primary:     "Increment",
				LeftIcon:    "list-add-symbolic",
				OnClick: func(sdk.CallbackEvent) error {
					a.SetState(func(state *counterState) {
						state.Count++
					})
					return nil
				},
			},
		},
	}, nil
}

func main() {
	if err := sdk.Run[counterState](context.Background(), newCounterApplet()); err != nil {
		panic(err)
	}
}
```

## IPC Client

The IPC client lets applets listen to shell events and dispatch commands over
the Glimpse socket.

`ipc()` / `IPC()` takes a service name. Use `"shell"` for the panel. The socket
path is `$GLIMPSE_IPC_DIR/<service>.sock`, or `$XDG_RUNTIME_DIR/glimpse/ipc.sock`
for the shell.

| Operation | Meaning |
|---|---|
| `listen(channel)` | Subscribe by exact name, prefix pattern such as `"audio.*"`, or wildcard `"*"`. |
| `dispatch(action, params)` | Send a command and wait for acknowledgment. |

### Python

```python
from glimpse_sdk import ipc

async with app.background():
    sub = ipc("shell")
    async for event in await sub.listen("audio.*"):
        volume = event.fields.get("volume")
        await self.set_state(volume=int(volume or 0))
```

### TypeScript

```ts
import { ipc } from "glimpse-sdk";

const sub = ipc("shell");
for await (const event of sub.listen("audio.*")) {
  await this.setState({ volume: Number(event.fields.volume ?? 0) });
}

await sub.dispatch("set_volume", { level: "50" });
```

### Rust

```rust
use glimpse_sdk::ipc;

let sub = ipc("shell")?;
let mut stream = sub.listen("audio.*").await?;
while let Some(event) = stream.next().await {
    let event = event?;
    // event.name, event.ts, event.fields
}

let _ack = sub.dispatch("set_volume", [("level", "50")]).await?;
```

### Go

```go
sub := sdk.IPC("shell")
ctx, cancel := context.WithCancel(ctx)
defer cancel()

events, err := sub.Listen(ctx, "audio.*")
for event := range events {
	// event.Name, event.Ts, event.Fields
}

ack, err := sub.Dispatch(ctx, "set_volume", map[string]string{"level": "50"})
```

## Golden Fixture Workflow

The four SDKs share canonical JSON fixtures under `sdk/fixtures`. Update them
when adding widgets, events, common props, or action helpers.

| Check | Command |
|---|---|
| Rust SDK fixture tests | `cargo test` in `sdk/sdk-rs` |
| TypeScript SDK fixture tests | `npm test` in `sdk/sdk-ts` |
| Python SDK fixture tests | `python -m unittest discover -s tests` in `sdk/sdk-py` |
| Go SDK fixture tests | `go test ./...` in `sdk/sdk-go` |
| Rust renderer fixture test | `cargo test -p glimpse-shell golden_widget_fixtures_render_without_errors -- --nocapture` from the repo root |

Fixture rules:

- Widget fixtures must match every SDK serializer.
- Event fixtures must match every SDK parser.
- The Rust renderer fixture test must deserialize and render every widget
  fixture without a renderer error.
- If a fixture and an SDK disagree, fix the SDK unless the fixture violates the
  documented protocol.
- Interactive renderer widgets that emit events require stable `id` fields.

## See Also

| Page | Covers |
|---|---|
| [Getting Started](../custom-applets/getting-started.md) | First applet walkthrough using the tooling. |
| [Exec Applet](../custom-applets/exec.md) | Exec host config and options. |
| [Applet Tooling](../custom-applets/tooling.md) | Project, dev, link, and diagnostics workflows. |
| [Line Protocol](../custom-applets/exec-protocol.md) | Raw protocol commands, message shapes, and events. |
| [Components](../custom-applets/exec-components.md) | Popover component fields and component types. |
