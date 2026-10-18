# Keep data between restarts

Each applet gets a state directory, `~/.local/state/glimpse/applets/<id>/`, mode 0700. Every SDK can
save your state to `state.json` there and load it when the applet starts.

## Save state with the SDK

This keeps a task title across restarts:

::: code-group

```tsx [TypeScript]
import { Entry, Indicator, Popover, run, usePersistentState } from "glimpse-sdk";

function Task() {
  const [title, setTitle] = usePersistentState("title", "");
  return (
    <>
      <Indicator icon="view-list-symbolic" tooltip="Saved task" />
      <Popover title="Saved task">
        <Entry placeholder="Task title" value={title} onChange={setTitle} />
      </Popover>
    </>
  );
}

await run(<Task />);
```

```python [Python]
from dataclasses import dataclass

from glimpse_sdk import BaseApplet, Context, Entry, Indicator, Popover, State, W


@dataclass(frozen=True)
class Saved(State):
    title: str = ""


class App(BaseApplet[Saved]):
    state = Saved()
    persist = ("title",)

    def build(self, ctx: Context) -> list[W]:
        return [
            Indicator(icon="view-list-symbolic", tooltip="Saved task"),
            Popover(
                title="Saved task",
                children=[
                    Entry(
                        placeholder="Task title",
                        value=self.state.title,
                        on_change=lambda title: self.set_state(title=title),
                    )
                ],
            ),
        ]


if __name__ == "__main__":
    App.main()
```

```go [Go]
type state struct {
	Title string `json:"title"`
}

type app struct {
	sdk.Applet[state]
}

func (a *app) Build(ctx *sdk.Context) []sdk.Widget {
	return []sdk.Widget{
		sdk.Indicator(sdk.IndicatorProps{Icon: "view-list-symbolic", Tooltip: "Saved task"}),
		sdk.Popover(sdk.PopoverProps{Title: "Saved task"},
			sdk.Entry(sdk.EntryProps{
				Placeholder: "Task title",
				Value:       a.State().Title,
				OnChange:    func(title string) { a.SetState(func(s *state) { s.Title = title }) },
			})),
	}
}
```

```rust [Rust]
struct Task {
    title: String,
}

enum Msg {
    Title(String),
}

impl App for Task {
    type Msg = Msg;

    fn start(&mut self, ctx: &Ctx) {
        self.title = ctx.load_state().unwrap_or_default();
    }

    fn update(&mut self, ctx: &Ctx, msg: Msg) {
        let Msg::Title(title) = msg;
        let _ = ctx.save_state(&title);
        self.title = title;
    }

    fn build(&self, _ctx: &Ctx) -> Vec<Widget<Msg>> {
        vec![
            indicator().icon("view-list-symbolic").tooltip("Saved task").into(),
            popover()
                .title("Saved task")
                .child(
                    entry()
                        .placeholder("Task title")
                        .value(self.title.as_str())
                        .on_change(Msg::Title),
                )
                .into(),
        ]
    }
}
```

:::

Run it with `just dev`, type a title, then type `r` and Enter in the terminal to restart the applet.
Open the popover again: the title is still there.

## Keep two instances apart

The state directory belongs to the applet id, so [two instances](/applets/exec/configure#run-it-twice-with-different-settings)
of one applet share it. To keep their data apart, put the instance name in the key or file name:
`useAppletName()` in TypeScript, `ctx.name` in Python and Rust, `ctx.Name` in Go.

## Write your own files

For anything bigger than a few values, write your own files in the state directory:
`useStateDir()` in TypeScript, `ctx.state_dir` in Python and Rust, `ctx.StateDir` in Go. Write to a
temporary file and rename it over the old one, so a crash mid-write never leaves half a file.
