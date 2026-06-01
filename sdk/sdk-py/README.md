# Glimpse Applet Python SDK

Small async framework for building Glimpse `exec` applets without touching stdio or raw JSON.

Requires Python 3.14+.

## Install

```sh
pip install glimpse-applet-sdk
# or with uv:
uv add glimpse-applet-sdk
```

The distribution is named `glimpse-applet-sdk` on PyPI; the import name is `glimpse_sdk`.

## Develop

Create and live-run a Python applet project with the Glimpse tooling:

```sh
glimpse-applet new counter --lang python
cd counter
glimpse-applet dev
```

Read `docs/custom-applets/tooling.md` for project layout, `applet.toml`, dev applets, local linking, distribution, and diagnostics.

## Goals

- typed protocol models
- typed widget builders
- async runtime
- widget callbacks such as `Button(on_click=...)`, plus decorator-based callbacks for explicit ids
- separate `status(state)` and `popover(state)` methods; state mutation via `await self.set_state(...)`

## Example

```python
from dataclasses import dataclass

from glimpse_sdk import (
    Applet,
    AppletState,
    Box,
    Button,
    ButtonVariant,
    Hero,
    Icon,
    StatusItem,
    Label,
)


@dataclass
class DeployState(AppletState):
    version: str = "2026.04.07"
    status: str = "Ready"


class DeployApplet(Applet[DeployState]):
    def initial_state(self) -> DeployState:
        return DeployState()

    async def status(self, state: DeployState):
        return [
            StatusItem(
                id="deploy",
                icon=Icon.name("software-update-available-symbolic"),
                label=state.status,
            )
        ]

    async def popover(self, state: DeployState):
        return Box.vertical(
            [
                Hero(
                    icon=Icon.name("software-update-available-symbolic"),
                    title="Deploy",
                    subtitle=state.version,
                ),
                Label(label="Version"),
                Button(
                    label="Deploy now",
                    on_click=self.on_deploy,
                    icon="media-playback-start-symbolic",
                    variant=ButtonVariant.PRIMARY,
                ),
            ]
        )

    async def on_deploy(self, state: DeployState, _event) -> None:
        await self.set_state(status="Deploying")


if __name__ == "__main__":
    DeployApplet().run()
```

## IPC client

Talk to a running Glimpse daemon: subscribe to event channels and dispatch
actions. `ipc(service)` only resolves the socket path — the connection is
opened lazily.

```python
import glimpse_sdk

sub = glimpse_sdk.ipc(service="shell")  # "shell" | "wallpaper" | "lock"

# Fire an action; awaits the ack, raises IpcError if the server rejects it.
ack = await sub.dispatch("open_uri", {"uri": "https://example.com"})

# Stream events until the socket closes.
async for ev in sub.listen("audio.*"):
    print(ev.name, ev.fields)
```
