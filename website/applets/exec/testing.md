# Test an applet

Each SDK can run your applet without a panel: you feed it the panel's messages and check what it
draws and asks for. `just test` runs the tests of a generated project. Run `just dev` as well: only
the panel checks [which element may hold which](/applets/exec/reference/elements#parent-table).

## Check what the applet draws

::: code-group

```tsx [TypeScript]
import assert from "node:assert/strict";
import { test } from "node:test";
import type { ReactNode } from "react";
import { Button, Indicator, Popover, usePopover } from "glimpse-sdk";
import { render } from "glimpse-sdk/testing";

function Todo() {
  const popover = usePopover();
  return (
    <>
      <Indicator icon="view-list-symbolic" />
      <Popover title="Todo">
        <Button text="Close" onClick={popover.close} />
      </Popover>
    </>
  );
}

test("closes the popover when the button is clicked", async () => {
  const app = await render((<Todo />) as ReactNode);
  try {
    const tree = app.tree() as {
      children: Array<{ type: string; children: Array<{
        id: number; type: string; props: Record<string, unknown>;
      }> }>;
    };
    const popover = tree.children.find((node) => node.type === "popover");
    const button = popover?.children.find((node) => node.type === "button");
    assert.ok(button);
    assert.equal(button.props.text, "Close");
    await app.click(button.id);
    assert.deepEqual(app.requests(), [{ t: "close-popover" }]);
  } finally {
    await app.close();
  }
});
```

```python [Python]
from glimpse_sdk import Session

from applet import App


def test_build_shows_the_indicator() -> None:
    app = App()
    session = Session(app, lambda line: None)
    widgets = app.build(session.ctx)
    assert widgets[0].kind == "indicator"
    assert widgets[0].props["text"] == "Glimpse Todo"
```

```go [Go]
// main_test.go
package main

import (
	"testing"

	sdk "github.com/alex-oleshkevich/glimpse/sdk/sdk-go/sdk"
)

func TestMounts(t *testing.T) {
	var lines []string
	session := sdk.NewSession(&app{}, func(line string) { lines = append(lines, line) })
	session.Handle(map[string]any{"t": "hello", "v": float64(1), "name": "test",
		"state": t.TempDir(), "options": map[string]any{}})
	session.Flush()
	if len(lines) == 0 {
		t.Fatal("no commit")
	}
}
```

```rust [Rust]
// at the end of src/main.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mounts() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut session = Session::new(App, tx);
        session.handle(&json!({"t": "hello", "v": 1, "name": "test", "state": "", "options": {}}));
        session.flush().expect("a valid tree");
        assert!(rx.try_recv().is_ok());
    }
}
```

:::

Save a TypeScript test as `<name>.test.tsx` in the project folder. `just test` bundles each one into
`build/tests/` and runs it with `node --test`. The Python test is the one `glimpsectl applet new` generates. In Go and Rust, each line the session
writes is a protocol message; a `commit` holds the operations that build the tree, described in
[Applet protocol](/developers/protocol#commit).

## Drive a TypeScript applet

```ts
function render(
  element: ReactNode,
  host?: {
    options?: Record<string, unknown>;
    placement?: Record<string, unknown>;
    state?: string;
  },
): Promise<Rendered>;

interface Rendered {
  tree(): unknown;                              // id, type, props, children; narrow it first
  requests(): Array<Record<string, unknown>>;  // e.g. { t: "close-popover" }
  click(id: number): Promise<void>;             // clicks a Button or activates an ActionRow
  event(id: number, name: string, args?: unknown[]): Promise<void>; // e.g. "onToggle", [true]
  setOptions(options: Record<string, unknown>): Promise<void>;
  setPopoverOpen(open: boolean): Promise<void>; // what usePopover().open sees
  close(): Promise<void>;                       // unmounts; await it before the next render
}
```

Without `state`, the test gets a temporary directory that `close()` removes.

The in-memory host checks the size limits but not nesting or text cleaning.
