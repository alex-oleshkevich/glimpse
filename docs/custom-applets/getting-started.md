# Getting started: build your first applet

This walkthrough builds a small **counter applet**. It shows a number in the
panel; opening the popover and clicking **Increment** bumps the number.

Use an exec applet when you need live state, a custom popover, or interaction
that is more than "run this command". If you only need a launcher button, start
with [Command Applet](./command.md).

## Prerequisites

- A working Glimpse install and a visible panel. See [Installation](../installation.md).
- `glimpse-shell` in your `PATH`.
- One supported language toolchain:
  - **Python**: 3.14+.
  - **TypeScript**: Node.js 20+.
  - **Rust**: `rustc` 1.93+ and `cargo`.
  - **Go**: 1.24+.

Run the doctor before creating your first applet:

```sh
glimpse-shell applets doctor
```

## Create the project

Create a generated Python applet:

```sh
glimpse-shell applets new counter --lang python
cd counter
```

Start it in development mode:

```sh
glimpse-shell applets dev
```

The default panel includes `__dev__`, which displays applets started by the dev
tool. If you use a custom panel layout, keep it or add it to one panel section:

```toml
[[panels]]
left = ["workspaces", "__dev__"]
```

Prefer another language? Use the same workflow:

| Language | Create command |
|---|---|
| Python | `glimpse-shell applets new counter --lang python` |
| TypeScript | `glimpse-shell applets new counter --lang typescript` |
| Rust | `glimpse-shell applets new counter --lang rust` |
| Go | `glimpse-shell applets new counter --lang go` |

The generated project includes its manifest, dependencies, run command, and
default applet code. Read [Applet Tooling](./tooling.md) for the full command
reference.

## Link for local use

Development mode is temporary. When the applet is ready for local use, link the
generated project:

```sh
glimpse-shell applets link
```

Then add the applet id to a panel section:

```toml
[[panels]]
right = ["counter", "network", "battery"]
```

`link` symlinks the project `applet.toml` into the applet search path, so
the panel can reference the applet by id while you keep working in the project
directory.

For sharing an applet with other users, ship `applet.toml` together with the
executable or script. [Applet Tooling](./tooling.md) covers that distribution
shape.

## What the applet does

The generated counter applet demonstrates the normal exec applet shape:

- `status` returns the compact panel item.
- `popover` returns a widget tree shown when the panel item opens.
- Interactive widgets have stable ids.
- Events mutate applet state.
- The SDK re-renders status and popover output after state changes.

The SDK handles stdin, stdout, JSON lines, and protocol details. Applet logs and
diagnostics should go to stderr.

## Python shape

When you scaffold with `--lang python`, the applet shape looks like this:

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

## TypeScript shape

When you scaffold with `--lang typescript`, the applet shape looks like this:

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

## Rust shape

When you scaffold with `--lang rust`, the applet shape looks like this:

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

## Go shape

When you scaffold with `--lang go`, the applet shape looks like this:

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

## What happened

The applet tooling created an applet package and language project for you.
During development, the dev runner starts the child process and exposes it
through the `__dev__` panel slot. After linking, any panel section can
load the applet by id.

When Glimpse starts the applet, it sends an `init` message. The applet responds
with `status` output for the panel. When the popover opens, the SDK renders the
popover tree. When you click the `increment` tile, Glimpse sends an event back
to the child; the SDK routes it to your handler and pushes fresh output after
the state changes.

## Common follow-ups

- **Pass per-applet config.** Add an `[exec.options]` table in the generated
  applet package; the SDK exposes those values during initialization.
- **Refresh on a timer.** Spawn a background task that updates state on an
  interval; the SDK re-renders after state changes.
- **Use richer widgets.** See [Components](./exec-components.md) for `tile`,
  `switch_tile`, `slider_tile`, `choice_list`, `meter`, `key_value_grid`,
  `badge`, and `popover_shell`.
- **Debug startup.** Use `glimpse-shell applets doctor --strict` and write
  applet diagnostics to stderr.

## Where to go next

| Topic | Page |
|---|---|
| Scaffold, run, link, and remove applets | [Applet Tooling](./tooling.md) |
| Configure the exec applet host | [Exec Applet](./exec.md) |
| Line protocol reference | [Line Protocol](./exec-protocol.md) |
| Every widget with fields | [Components](./exec-components.md) |
| SDK reference per language | [Exec SDK](../applets/exec-sdk.md) |
| LLM-friendly single-file reference | [LLM Exec Reference](../llms/exec.md) |
