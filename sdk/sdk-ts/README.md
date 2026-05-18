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
glimpse-applet new counter --lang typescript
cd counter
glimpse-applet dev
```

Read `docs/custom-applets/tooling.md` for project layout, `applet.toml`, dev applets, linking, and diagnostics.

## Goals

- typed protocol models
- typed widget builders
- async runtime
- explicit typed handler registration
- separate `status(state)` and `popover(state)` methods; state mutation via `await this.setState(...)`

## Example

```ts
import {
  Applet,
  Box,
  Button,
  Hero,
  Icon,
  StatusItem,
  Text,
  type TreeNode,
} from "glimpse-sdk";

interface DeployState {
  version: string;
  status: string;
}

class DeployApplet extends Applet<DeployState> {
  protected initialState(): DeployState {
    return { version: "2026.04.07", status: "Ready" };
  }

  constructor() {
    super();
    this.onClick("deploy_now", async () => {
      await this.setState({ status: "Deploying" });
    });
  }

  protected async status(state: DeployState): Promise<StatusItem[]> {
    return [
      new StatusItem({
        id: "deploy",
        icon: Icon.name("software-update-available-symbolic"),
        label: state.status,
      }),
    ];
  }

  protected async popover(state: DeployState): Promise<TreeNode | null> {
    return Box.vertical([
      new Hero({
        icon: Icon.name("software-update-available-symbolic"),
        title: "Deploy",
        subtitle: state.version,
      }),
      new Text("Version"),
      new Button({
        id: "deploy_now",
        label: "Deploy now",
        icon: "media-playback-start-symbolic",
        variant: "primary",
      }),
    ]);
  }
}

await new DeployApplet().run();
```

## Handler Registration

Use explicit registration helpers instead of decorators:

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
