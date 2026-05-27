# Exec Applet Reference

This page is a compact reference for LLMs and maintainers working on Glimpse
exec applets. It intentionally starts from the applet tooling, because generated
projects contain the correct manifests, dependencies, commands, and SDK wiring.

## Tooling-First Workflow

Create, run, and link applets through the CLI:

```sh
glimpse-shell applets new counter --lang python
cd counter
glimpse-shell applets dev
glimpse-shell applets link
```

Supported language values:

| Language | Value |
|---|---|
| Python | `python` |
| TypeScript | `typescript` |
| Rust | `rust` |
| Go | `go` |

Development applets appear in the `__dev__` slot:

```toml
[[panels]]
left = ["workspaces", "__dev__"]
```

For local use, link the project and add the applet id to a panel section:

```toml
[[panels]]
right = ["counter", "network", "battery"]
```

For distribution, share the applet executable or script with an `applet.toml`
that points to it. `link` is a local project workflow, not the distribution
format.

## Choosing Command Or Exec

| Need | Use |
|---|---|
| Run one shell command from a panel button | `command` applet |
| Show live state in the panel | `exec` applet |
| Render a custom popover | `exec` applet |
| Handle clicks, toggles, sliders, or choices | `exec` applet |
| Build a reusable local applet project | `exec` applet with applet tooling |

## Applet Package

Generated applet projects include an `applet.toml` package. The package is what
the panel loads:

```toml
id = "counter"
type = "exec"

[exec]
command = ["uv", "run", "main.py"]
restart_delay_ms = 1000
env_forward = false

[exec.options]
start = 0
```

Useful fields:

| Field | Default | Meaning |
|---|---|---|
| `command` | required | Program and arguments to start. |
| `restart_delay_ms` | `1000` | Delay before restart after child exit. Minimum `50`. |
| `work_dir` | unset | Working directory for the child process. |
| `options` | `{}` | Applet-specific data sent in the `init` message. |
| `env` | `{}` | Extra environment variables for the child process. |
| `env_forward` | `false` | Set `true` only when the child really needs the parent environment. |

## Runtime Model

| Step | What happens |
|---|---|
| Start | The panel reads the applet package and starts `[exec].command`. |
| Init | The child receives `init {"instance":"...","options":{...}}`. |
| Status | The child sends complete `status` updates for panel items. |
| Popover | The child sends complete `popover` trees when it has detail UI. |
| Events | The panel sends clicks, scrolls, toggles, changes, and lifecycle events back to the child. |
| Restart | If the child exits, the panel restarts it after `restart_delay_ms`. |

The SDKs own stdin/stdout parsing and JSON serialization. Applet diagnostics
must go to stderr.

## Protocol Lines

Every raw protocol message is one line:

```txt
command optional-json
```

Common messages:

| Message | Direction | Payload |
|---|---|---|
| `init` | Panel to child | `{"instance":"name","options":{...}}` |
| `event` | Panel to child | `{"id":"...","type":"...","source":"...",...}` |
| `status` | Child to panel | `{"items":[...]}` |
| `popover` | Child to panel | `{"root":{...}}` |
| `class` | Child to panel | Plain CSS class suffix. |
| `close-popover` | Child to panel | No payload. |

Status item shape:

```json
{
  "id": "counter",
  "icon": "view-refresh-symbolic",
  "label": "4",
  "tooltip": "Counter"
}
```

Popover root shape:

```json
{
  "type": "column",
  "data": {
    "children": [
      { "type": "hero", "data": { "title": "Counter", "subtitle": "Value: 4", "icon": "view-refresh-symbolic" } },
      { "type": "tile", "data": { "id": "increment", "primary": "Increment", "left_icon": "list-add-symbolic" } }
    ]
  }
}
```

## Components

All components support common fields:

| Field | Default | Meaning |
|---|---|---|
| `visible` | `true` | Hide the component when `false`. |
| `tooltip` | unset | Hover text. |
| `css_classes` | `[]` | Extra CSS classes for theme authors. |
| `styles` | `{}` | Inline style values for supported renderer paths. Prefer CSS classes for normal theming. |

Layout components:

| Type | Main fields |
|---|---|
| `popover_shell` | `size`, `children`, `footer`, `footer_visible` |
| `row` | `children` |
| `column` | `children` |
| `container` | `children` |
| `boxed_list` | `children` |
| `button_row` | `children` |
| `scroll` | `child` |
| `separator` | common fields only |
| `circle_box` | `color` |

Display components:

| Type | Main fields |
|---|---|
| `label` | `label`, `xalign`, `wrap` |
| `header` | `label` |
| `hero` | `title`, `subtitle`, `icon`, `icon_size`, `toggle`, `toggle_sensitive`, `separator`, `trailing` |
| `badge` | `label`, `kind` |
| `status_dot` | `status` |
| `panel_indicator` | `id`, `icon`, `label`, `active`, `checked`, `needs_attention`, `extra` |
| `empty_state` | `title`, `subtitle` |
| `spinner` | `spinning` |
| `meter` | `id`, `icon`, `label`, `value`, `min`, `max`, `step`, `text` |
| `key_value_grid` | `rows` |

Tile and control components:

| Type | Main fields | Event |
|---|---|---|
| `tile` | `id`, `primary`, `secondary`, `left_icon`, `left`, `right` | `click` when `id` is set |
| `segmented_tile` | `id`, `primary`, `secondary`, `left_icon`, `left`, `right`, `child`, `expanded` | `toggle`; `click` when `id` is set |
| `switch_tile` | `id`, `primary`, `secondary`, `left_icon`, `left`, `active` | `toggle` |
| `expander_tile` | `id`, `primary`, `secondary`, `left_icon`, `left`, `child`, `expanded` | `toggle` |
| `slider_tile` | `id`, `label`, `left_icon`, `left`, `value`, `min`, `max`, `step`, `page`, `digits`, `snap_step` | debounced `change` |
| `choice_tile` | `id`, `primary`, `secondary`, `left_icon`, `left`, `selected` | `click` |
| `choice_list` | `id`, `active`, `choices` | `change` |

Specialized components:

| Type | Purpose |
|---|---|
| `pager_item`, `pager_strip` | Workspace or page indicators. |
| `camera_indicator`, `mic_indicator`, `muted_indicator`, `screencast_indicator`, `location_indicator` | Privacy/status indicators. |
| `calendar` | Calendar view with selected date and marked event days. |
| `battery_hero` | Battery popover header. |
| `date_hero` | Date popover header. |
| `events` | Calendar event list. |
| `weather_forecast_list`, `weather_hourly_strip` | Weather forecast rows. |
| `world_clock` | Timezone rows. |

## Events

| Source | Event | Payload |
|---|---|---|
| Status item with `id` | `click`, `scroll` | `button` or `delta_y`. |
| `tile`, `choice_tile`, `panel_indicator`, `pager_item` | `click` | `id` identifies the component. |
| `switch_tile`, `expander_tile`, `segmented_tile` | `toggle` | `active = true` or `false`. |
| `slider_tile` | `change` | Numeric `value`. |
| `choice_list`, `pager_strip`, `calendar` | `change` | Selected id, item id, or date string in `value`. |
| Popover lifecycle | `open`, `close` | `id = "popover"`. |

## SDK Counter Shapes

Use the generated project as the source of truth. These snippets show the
current API style.

### Python

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
        return [StatusItem(id="counter", icon="view-refresh-symbolic", label=str(state.count))]

    async def popover(self, state: CounterState):
        return Column(
            children=[
                Hero(icon="view-refresh-symbolic", title="Counter", subtitle=f"Value: {state.count}"),
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

### TypeScript

```ts
import { Applet, Column, Hero, Label, StatusItem, Tile, type TreeNode } from "glimpse-sdk";

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
    return [new StatusItem({ id: "counter", icon: "view-refresh-symbolic", label: String(state.count) })];
  }

  protected async popover(state: CounterState): Promise<TreeNode | null> {
    return new Column({
      children: [
        new Hero({ icon: "view-refresh-symbolic", title: "Counter", subtitle: `Value: ${state.count}` }),
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

### Rust

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
```

### Go

```go
type counterState struct {
	Count int
}

type counterApplet struct {
	sdk.BaseApplet[counterState]
}

func (a *counterApplet) Status(_ context.Context, state *counterState) ([]sdk.StatusItem, error) {
	return []sdk.StatusItem{{
		ID:    "counter",
		Icon:  "view-refresh-symbolic",
		Label: fmt.Sprintf("%d", state.Count),
	}}, nil
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
					a.SetState(func(state *counterState) { state.Count++ })
					return nil
				},
			},
		},
	}, nil
}
```

## IPC Client

SDK applets can listen to shell events and dispatch shell commands through IPC.
Use `"shell"` as the service name for panel events and commands.

| Operation | Meaning |
|---|---|
| `listen(channel)` | Subscribe by exact name, prefix pattern such as `"audio.*"`, or wildcard `"*"`. |
| `dispatch(action, params)` | Send a command and wait for acknowledgment. |

Python:

```python
from glimpse_sdk import ipc

async with app.background():
    sub = ipc("shell")
    async for event in await sub.listen("audio.*"):
        await self.set_state(volume=int(event.fields.get("volume") or 0))
```

TypeScript:

```ts
import { ipc } from "glimpse-sdk";

const sub = ipc("shell");
for await (const event of sub.listen("audio.*")) {
  await this.setState({ volume: Number(event.fields.volume ?? 0) });
}

await sub.dispatch("set_volume", { level: "50" });
```

## Best Practices

| Practice | Why |
|---|---|
| Start from `glimpse-shell applets new` | Generated projects track the current SDK and package shape. |
| Use `glimpse-shell applets dev` while editing | It watches source files and shows the applet through `__dev__`. |
| Use `glimpse-shell applets link` for local use | It keeps the applet entry tied to the project `applet.toml` while you iterate. |
| Keep panel labels short | Long labels crowd the panel. |
| Put detail in the popover | The panel is for glanceable state. |
| Use stable ids | Events are easier to route when ids do not change. |
| Send complete updates | Treat each `status` or `popover` line as the current truth. |
| Keep `env_forward = false` by default | Forward only the environment your applet actually needs. |
| Write diagnostics to stderr | Stdout is reserved for protocol lines. |

## See Also

| Page | Covers |
|---|---|
| [Getting Started](../custom-applets/getting-started.md) | First applet using the tooling. |
| [Applet Tooling](../custom-applets/tooling.md) | Project commands, dev mode, local linking, distribution shape, diagnostics. |
| [Exec Applet](../custom-applets/exec.md) | Exec host config and options. |
| [Line Protocol](../custom-applets/exec-protocol.md) | Raw protocol messages and event payloads. |
| [Components](../custom-applets/exec-components.md) | Component fields and implemented component types. |
| [Exec SDK](../applets/exec-sdk.md) | SDK reference by language. |
