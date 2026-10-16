# Test an applet

Use `glimpse-sdk/testing` to render an applet in memory, inspect its tree, and exercise handlers without starting a panel.

## Write a test

Add `copy.test.tsx` to a generated applet project:

```tsx
import assert from "node:assert/strict";
import { test } from "node:test";
import type { ReactNode } from "react";
import { Button, copy, Indicator, Popover } from "glimpse-sdk";
import { render } from "glimpse-sdk/testing";

test("copies a task when the button is clicked", async () => {
  const app = await render(
    <>
      <Indicator icon="view-list-symbolic" />
      <Popover title="Todo">
        <Button text="Copy task" onClick={() => copy("Buy groceries")} />
      </Popover>
    </> as ReactNode,
  );
  try {
    const tree = app.tree() as {
      children: Array<{ type: string; children: Array<{
        id: number; type: string; props: Record<string, unknown>;
      }> }>;
    };
    const popover = tree.children.find((node) => node.type === "popover");
    const button = popover?.children.find((node) => node.type === "button");
    assert.ok(button);
    assert.equal(button.props.text, "Copy task");
    await app.click(button.id);
    assert.deepEqual(app.requests(), [{ t: "copy", text: "Buy groceries" }]);
  } finally {
    await app.close();
  }
});
```

Bundle the test with the project's existing esbuild dependency, then run Node's test runner:

```sh
npx esbuild copy.test.tsx --bundle --platform=node --format=esm --outfile=build/copy.test.mjs
node --test build/copy.test.mjs
```

The test checks both the rendered button and its clipboard request. It does not change your real clipboard. The `ReactNode` assertion bridges the SDK's JSX return type and the test host's React input type.

Run `glimpsectl applet check .` to validate the desktop entry, local configuration, and `Exec` expansion without a panel. `glimpsectl applet check --run .` also starts the applet headlessly and validates its first tree, reporting errors such as `section cannot hold section`.

## Supply host state

`render(element, host)` accepts optional `options`, `placement`, and `state` fields. `state` is a directory path; omitting it gives the test a temporary directory that `close()` removes.

The returned session provides:

| Method | Use |
| --- | --- |
| `tree()` | Get a cloned root with `id`, `type`, `props`, and `children`. Its TypeScript return type is `unknown`; narrow it before inspecting fields. |
| `requests()` | Get the host requests sent so far as `Array<Record<string, unknown>>`. |
| `click(id)` | Click a `Button` or activate an `ActionRow`, returning `Promise<void>`. |
| `event(id, name, args?)` | Send another subscribed event, such as `onToggle` with `[true]`. |
| `setOptions(options)` | Replace options and update `useOptions()`. |
| `setPopoverOpen(open)` | Update `usePopoverOpen()`. |
| `setService(name, value)` | Supply a service snapshot: `setService("mpris", { players: [player] })` or `setService("compositor", { name, workspaces, windows, outputs })`. |
| `close()` | Unmount and release the session. Await it before starting another render. |

All event and update methods return `Promise<void>`. The in-memory host checks commit shape and the SDK's limits on nodes, children, operations and array lengths, but not the panel's nesting rules or text cleaning. Use `glimpsectl applet check --run .` to check the first tree's nesting. Run `glimpsectl applet run --dev .` to exercise later updates and inspect the displayed text. If the panel rejects a tree, it stops the applet and logs the reason; the development terminal prints nothing about that rejection.
