# Getting Started: Build Your First Applet

This walkthrough takes you from an empty directory to a working
**counter applet** in your panel. The counter shows a number; clicking
"Increment" inside its popover bumps the number by one. Total time:
roughly ten minutes.

You can do this in any of four languages — Rust, Python, TypeScript,
or Go. Pick the one you're comfortable with; the protocol underneath
is identical, so applets behave the same regardless.

If you only want a launcher button that runs a single command, you
don't need an SDK at all — read [Command Applet](./command.md) first
and come back here only if you need live state or custom controls.

## Prerequisites

- A working Glimpse install (`glimpse-shell` running, your panel
  visible). See [Installation](../installation.md).
- A `~/.config/glimpse/config.toml` you can edit.
- One language toolchain installed:
  - **Rust**: `rustc` 1.93+ and `cargo`.
  - **Python**: 3.14+.
  - **TypeScript**: Node.js 20+.
  - **Go**: 1.24+.

## What you're building

Your applet is a separate program that Glimpse spawns. It reads JSON
lines from stdin (events) and writes JSON lines to stdout (status
items and popover content). The SDK hides the line protocol behind a
small applet base class — you implement `render()` and event
handlers, and the SDK takes care of stdio + the JSON.

The final result, in your panel:

- A status item with a refresh icon and the current count.
- Left-click opens a popover with a hero, the value, and an
  Increment button.
- Clicking Increment updates the panel and the popover.

---

## Path A — Rust

Create a new binary crate:

```sh
cargo new --bin counter
cd counter
```

Edit `Cargo.toml`:

```toml
[package]
name = "counter"
version = "0.1.0"
edition = "2024"

[dependencies]
async-trait = "0.1"
glimpse-sdk = "0.3"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Replace `src/main.rs` with:

```rust
use async_trait::async_trait;
use glimpse_sdk::{
    Applet, AppletResult, Button, ButtonVariant, CallbackEvent, Column, Hero, Icon, Label, Section,
    StatusItem, TreeNode, run, tree,
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

    async fn popover(&self, state: &Self::State) -> AppletResult<Option<TreeNode>> {
        let count = state.count;
        Ok(Some(
            Column::new(tree![
                Hero::new("Counter", format!("Value: {count}")),
                Section::new(
                    "Controls",
                    tree![
                        Label::new(format!("Current: {count}")),
                        Button::new("increment")
                            .label("Increment")
                            .icon("list-add-symbolic")
                            .variant(ButtonVariant::Primary),
                    ],
                ),
            ])
            .spacing(8)
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

Build a release binary:

```sh
cargo build --release
```

The binary is at `target/release/counter`. Skip to [Wire it into your
panel](#wire-it-into-your-panel) and use that path as `command`.

---

## Path B — Python

Create a new project directory:

```sh
mkdir counter && cd counter
```

Install the SDK (uv recommended, but `pip install --user` works too):

```sh
uv init --bare
uv add glimpse-applet-sdk
```

Create `main.py`:

```python
from dataclasses import dataclass

from glimpse_sdk import (
    Applet,
    AppletState,
    Button,
    ButtonVariant,
    Column,
    Hero,
    Icon,
    Label,
    Section,
    StatusItem,
    click,
)


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
        return Column(
            spacing=8,
            children=[
                Hero(title="Counter", subtitle=f"Value: {state.count}"),
                Section(
                    title="Controls",
                    children=[
                        Label(text=f"Current: {state.count}"),
                        Button(
                            id="increment",
                            label="Increment",
                            icon="list-add-symbolic",
                            variant=ButtonVariant.PRIMARY,
                        ),
                    ],
                ),
            ],
        )

    @click("increment")
    async def on_increment(self, _event) -> None:
        await self.set_state(count=self.state.count + 1)


if __name__ == "__main__":
    CounterApplet().run()
```

Verify it runs (send EOF to exit):

```sh
echo "" | uv run python main.py
```

You should see one `status {...}` line on stdout. Skip to [Wire it
into your panel](#wire-it-into-your-panel) and use
`["uv", "run", "--directory", "/path/to/counter", "python", "main.py"]`
as `command`, or absolute paths to a venv interpreter.

---

## Path C — TypeScript

Create a new project:

```sh
mkdir counter && cd counter
npm init -y
npm pkg set type=module
npm install glimpse-sdk
npm install --save-dev typescript @types/node
```

Create `tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "outDir": "dist",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true
  },
  "include": ["src/**/*.ts"]
}
```

Create `src/main.ts`:

```ts
import {
  Applet,
  Button,
  Column,
  Hero,
  Icon,
  Label,
  Section,
  StatusItem,
  type TreeNode,
} from "glimpse-sdk";

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

  protected async popover(state: CounterState): Promise<TreeNode | null> {
    return new Column({
      spacing: 8,
      children: [
        new Hero({ title: "Counter", subtitle: `Value: ${state.count}` }),
        new Section({
          title: "Controls",
          children: [
            new Label(`Current: ${state.count}`),
            new Button({
              id: "increment",
              label: "Increment",
              icon: "list-add-symbolic",
              variant: "primary",
            }),
          ],
        }),
      ],
    });
  }
}

await new CounterApplet().run();
```

Build:

```sh
npx tsc
```

Skip to [Wire it into your panel](#wire-it-into-your-panel) and use
`["node", "/path/to/counter/dist/main.js"]` as `command`.

---

## Path D — Go

Initialize a module:

```sh
mkdir counter && cd counter
go mod init example.com/counter
go get github.com/alex-oleshkevich/glimpse/sdk/sdk-go
```

Create `main.go`:

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
        a.SetState(func(s *counterState) { s.Count++ })
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

func (a *counterApplet) Popover(_ context.Context, state *counterState) (sdk.Widget, error) {
    return sdk.Column{
        Spacing: 8,
        Children: []sdk.Widget{
            sdk.Hero{Title: "Counter", Subtitle: fmt.Sprintf("Value: %d", state.Count)},
            sdk.Section{
                Title: "Controls",
                Children: []sdk.Widget{
                    sdk.Label{Text: fmt.Sprintf("Current: %d", state.Count)},
                    sdk.Button{
                        CommonProps: sdk.CommonProps{ID: "increment"},
                        Label:       "Increment",
                        Icon:        "list-add-symbolic",
                        Variant:     sdk.ButtonVariantPrimary,
                    },
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

Build:

```sh
go build -o counter
```

Skip to the next section and use `/path/to/counter/counter` as
`command`.

---

## Wire it into your panel

Open `~/.config/glimpse/config.toml` and add an exec applet that
points at your binary or script:

```toml
[applets.counter]
extends = "exec"
command = ["/absolute/path/to/your/counter"]
```

Add the applet name to a panel section:

```toml
[[panels]]
right = ["counter", "network", "battery"]
```

Save the config. Glimpse picks up changes on the next panel reload —
either click your existing reload button or restart `glimpse-shell`
(`systemctl --user restart glimpse-shell`).

You should now see your counter in the panel. Left-click opens the
popover; click **Increment** and the number updates in both places.

## What just happened

When Glimpse started your applet it sent one `init` line on stdin
with the applet's name and any `options` from your config (we didn't
set any). Your `render()` then printed a `status` line and (because
the popover opened later) a `popover` line. When you clicked
Increment, Glimpse sent an `event` line back; the SDK routed it to
your click handler; you mutated state; the SDK re-rendered and pushed
fresh `status` + `popover` lines.

You never wrote a single byte of JSON or touched stdin/stdout — the
SDK handles all of it, and the `render()` function is your only
required entry point.

## Common follow-ups

- **Pass per-applet config.** Add a `[applets.counter.options]` table
  in your TOML; the SDK exposes it to your applet during `init`.
- **Refresh on a timer**, not just on events. Spawn a background task
  that mutates state every N seconds — the SDK re-renders
  automatically.
- **Use richer widgets.** The full component reference is at
  [Components](./exec-components.md). The most useful ones for live
  state are `item` (display row with a right slot), `action_item`
  (clickable row with a render-only right slot), `meter` (progress +
  slider), `property_list` (key/value facts), `slider` (numeric control),
  and `select` (mode switcher).
- **Restart on crash.** The `restart_delay_ms` config option controls
  how soon Glimpse re-spawns your applet after it exits. The default
  is 1 second.
- **Debug.** Write to stderr from your applet — Glimpse logs it but
  doesn't display it. Don't write non-protocol bytes to stdout; they
  corrupt the line stream.

## Where to go from here

| Topic | Page |
|---|---|
| Configure the exec applet | [Exec Applet](./exec.md) |
| Line protocol reference | [Line Protocol](./exec-protocol.md) |
| Every widget with its fields | [Components](./exec-components.md) |
| Single-page SDK reference per language | [Exec SDK](../applets/exec-sdk.md) |
| LLM-friendly single-file reference | [LLM exec docs](../llms/exec.md) |
