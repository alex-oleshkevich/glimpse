# Start an applet in Python, Go or Rust

Create a project, run it in your panel, and change it. The [todo tutorial](/applets/exec/) walks
through the same steps in TypeScript.

## Install the toolchain

You need a running glimpse panel and [`just`](https://github.com/casey/just), plus:

| Language | Needs |
| --- | --- |
| Python | Python 3.14 and [`uv`](https://docs.astral.sh/uv/), which fetches the SDK on first run |
| Go | Go 1.24 or newer |
| Rust | Rust with the 2024 edition |

## Create the project

::: code-group

```sh [Python]
glimpsectl applet new --language py --id me.aresa.GlimpseTodo --yes
```

```sh [Go]
glimpsectl applet new --language go --id me.aresa.GlimpseTodo --yes
```

```sh [Rust]
glimpsectl applet new --language rs --id me.aresa.GlimpseTodo --yes
```

:::

This creates `glimpse-todo/` with the name "Glimpse Todo". Leave out `--yes` to be asked for the
name, description, icon and directory.

| File | What it is |
| --- | --- |
| `applet.py`, `main.go` or `src/main.rs` | Your applet |
| `me.aresa.GlimpseTodo.desktop` | The desktop entry: name, icon and the command that runs it |
| `devconfig.toml` | `options` and `env` for development runs |
| `justfile` | `dev`, `build`, `test`, `lint`, `package`, `install` |
| `pyproject.toml`, `go.mod` or `Cargo.toml` | The project and its SDK dependency |
| `test_applet.py` (Python) | A test that builds the applet and checks its indicator |
| `README.md`, `.gitignore` | How to run and package the project, and the files git ignores |

## Run it in the panel

```sh
cd glimpse-todo
just dev
```

A "Glimpse Todo" chip appears at the left of your bar. Click it to see an "It works" row. Keep the
terminal open: your applet's errors print there, and Ctrl C removes it from the bar. `just dev right`
puts it in the right zone instead.

## Change it

The generated applet:

::: code-group

```python [Python]
#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.14"
# dependencies = ["glimpse-applet-sdk>=0.13"]
# ///
from glimpse_sdk import BaseApplet, Context, Indicator, Popover, Row, Section, State, W


class App(BaseApplet[State]):
    state = State()

    def build(self, ctx: Context) -> list[W]:
        return [
            Indicator(
                icon="application-x-addon-symbolic",
                tooltip="Glimpse Todo",
                children=["Glimpse Todo"],
            ),
            Popover(
                icon="application-x-addon-symbolic",
                title="Glimpse Todo",
                children=[
                    Section(
                        title="Glimpse Todo",
                        children=[Row(title="It works")],
                    )
                ],
            ),
        ]


if __name__ == "__main__":
    App.main()
```

```go [Go]
package main

import sdk "github.com/alex-oleshkevich/glimpse/sdk/sdk-go/sdk"

type state struct{}

type app struct {
	sdk.Applet[state]
}

func (a *app) Build(ctx *sdk.Context) []sdk.Widget {
	return []sdk.Widget{
		sdk.Indicator(
			sdk.IndicatorProps{
				Icon: "application-x-addon-symbolic",
				Tooltip: "Glimpse Todo",
			},
			"Glimpse Todo",
		),
		sdk.Popover(
			sdk.PopoverProps{
				Icon: "application-x-addon-symbolic",
				Title: "Glimpse Todo",
			},
			sdk.Section(
				sdk.SectionProps{
					Title: "Glimpse Todo",
				},
				sdk.Row(sdk.RowProps{
					Title: "It works",
				}),
			),
		),
	}
}

func main() {
	sdk.Main(&app{})
}
```

```rust [Rust]
use glimpse_sdk::*;
use std::process::ExitCode;

struct App;

enum Msg {}

impl glimpse_sdk::App for App {
    type Msg = Msg;

    fn update(&mut self, _ctx: &Ctx, msg: Msg) {
        match msg {}
    }

    fn build(&self, _ctx: &Ctx) -> Vec<Widget<Msg>> {
        vec![
            indicator()
                .icon("application-x-addon-symbolic")
                .tooltip("Glimpse Todo")
                .text("Glimpse Todo")
                .into(),
            popover()
                .icon("application-x-addon-symbolic")
                .title("Glimpse Todo")
                .child(section().title("Glimpse Todo").child(row().title("It works")))
                .into(),
        ]
    }
}

fn main() -> ExitCode {
    run(App)
}
```

:::

Rename the row. Saving rebuilds the applet and restarts it in the bar.

::: code-group

```diff [Python]
                     Section(
                         title="Glimpse Todo",
-                        children=[Row(title="It works")],
+                        children=[Row(title="Buy groceries")],
                     )
```

```diff [Go]
 				sdk.Row(sdk.RowProps{
-					Title: "It works",
+					Title: "Buy groceries",
 				}),
```

```diff [Rust]
-                .child(section().title("Glimpse Todo").child(row().title("It works")))
+                .child(section().title("Glimpse Todo").child(row().title("Buy groceries")))
```

:::

## Next

- The SDK pages: [Python](/applets/exec/reference/python), [Go](/applets/exec/reference/go),
  [Rust](/applets/exec/reference/rust). Every element is in [Elements](/applets/exec/reference/elements).
- [Keep data between restarts](/applets/exec/data), [test it](/applets/exec/testing), then
  [install and share it](/applets/exec/share).
