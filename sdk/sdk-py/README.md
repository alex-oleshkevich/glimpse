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

Read `docs/custom-applets/tooling.md` for project layout, `applet.toml`, dev applets, linking, and diagnostics.

## Goals

- typed protocol models
- typed widget builders
- async runtime
- decorator-based callbacks (`@click`, `@scroll`, `@input`, `@change`, `@toggle`)
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
    Label,
    StatusItem,
    click,
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
                Label(text="Version"),
                Button(
                    id="deploy_now",
                    label="Deploy now",
                    icon="media-playback-start-symbolic",
                    variant=ButtonVariant.PRIMARY,
                ),
            ]
        )

    @click("deploy_now")
    async def on_deploy(self, _event) -> None:
        await self.set_state(status="Deploying")


if __name__ == "__main__":
    DeployApplet().run()
```
