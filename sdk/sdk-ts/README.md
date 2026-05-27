# Glimpse Applet TypeScript SDK

Small async framework for building Glimpse `exec` applets without touching stdio or raw JSON.

Requires Node.js 20+.

## Install

```sh
npm install glimpse-sdk
```

## Develop

Create and live-run a TypeScript applet project with the Glimpse tooling:

```sh
glimpse-shell applets new counter --lang typescript
cd counter
glimpse-shell applets dev
```

Read `docs/custom-applets/tooling.md` for project layout, `applet.toml`, dev applets, local linking, distribution, and diagnostics.

## Goals

- typed protocol models
- typed widget builders
- async runtime
- widget-local callbacks, with explicit typed handler registration available when needed
- separate `status(state)` and `popover(state)` methods; state mutation via `await this.setState(...)`

## Example

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

interface DeployState {
  version: string;
  status: string;
}

class DeployApplet extends Applet<DeployState> {
  constructor() {
    super();
  }

  protected initialState(): DeployState {
    return { version: "2026.04.07", status: "Ready" };
  }

  protected async status(state: DeployState): Promise<StatusItem[]> {
    return [
      new StatusItem({
        id: "deploy",
        icon: "software-update-available-symbolic",
        label: state.status,
      }),
    ];
  }

  protected async popover(state: DeployState): Promise<TreeNode | null> {
    return new Column({
      children: [
        new Hero({
          icon: "software-update-available-symbolic",
          title: "Deploy",
          subtitle: state.version,
        }),
        new Label("Version"),
        new Tile({
          id: "deploy_now",
          primary: "Deploy now",
          left_icon: "media-playback-start-symbolic",
          on_click: async () => {
            await this.setState({ status: "Deploying" });
          },
        }),
      ],
    });
  }
}

await new DeployApplet().run();
```

## Handler Registration

Prefer widget-local callbacks for controls rendered in `popover(state)`.
Explicit registration helpers are still available when the handler should live
outside the widget tree:

- `this.onClick(id, handler)`
- `this.onScroll(id, handler)`
- `this.onInput(id, handler)`
- `this.onChange(id, handler)`
- `this.onToggle(id, handler)`

The SDK owns the line transport. `status(state)` produces the panel items;
`popover(state)` produces the popover tree; both are pure functions of state.

## IPC client

Talk to a running Glimpse daemon: subscribe to event channels and dispatch
actions. `ipc(service)` only resolves the socket path — the connection is
opened lazily.

```ts
import { ipc } from "glimpse-sdk";

const sub = ipc("shell"); // "shell" | "wallpaper" | "idle" | "lock"

// Fire an action; awaits the ack, throws IpcError if the server rejects it.
const ack = await sub.dispatch("open_uri", { uri: "https://example.com" });

// Stream events until the socket closes.
for await (const ev of sub.listen("audio.*")) {
  console.log(ev.name, ev.fields);
}
```
