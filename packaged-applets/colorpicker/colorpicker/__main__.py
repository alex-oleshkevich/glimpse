# /// script
# requires-python = ">=3.14"
# dependencies = ["glimpse-applet-sdk>=0.2,<1"]
# ///

import asyncio
import shutil
import subprocess
from dataclasses import dataclass

from glimpse_sdk import (
    ActionItem,
    Applet,
    AppletState,
    ButtonVariant,
    Color,
    Column,
    Container,
    EmptyState,
    Hero,
    PopoverScaffold,
    Radius,
    StatusItem,
    Text,
    click,
)
from glimpse_sdk.widgets import Align, Button


@dataclass
class ColorPickerState(AppletState):
    items: list[str]


class ColorPickerApplet(Applet[ColorPickerState]):
    def initial_state(self) -> ColorPickerState:
        return ColorPickerState(
            items=[
                "#ff0000",
                "#00ff00",
                "#0000ff",
            ]
        )

    @property
    def is_hyprpicker_installed(self) -> bool:
        return shutil.which("hyprpicker") is not None

    async def status(self, state: ColorPickerState):
        return [
            StatusItem(
                id="counter",
                icon="color-select-symbolic",
            )
        ]

    async def popover(self, state: ColorPickerState):
        if not self.is_hyprpicker_installed:
            return EmptyState(
                title="Not installed", subtitle="hyprpicker is not installed"
            )

        return PopoverScaffold(
            hero=Hero(
                title="Color picker",
                subtitle="Pick a color",
                icon="color-select-symbolic",
            ),
            body=Column(
                spacing=16,
                halign=Align.FILL,
                children=[
                    Button(
                        id="pick",
                        label="Pick color",
                        variant=ButtonVariant.PRIMARY,
                        hexpand=True,
                    ),
                    Column(
                        spacing=4,
                        children=[
                            Text(text="Recent colors", css_classes=["header"]),
                            *[
                                ActionItem(
                                    id=f"pick_{color}",
                                    label=color,
                                    left=Container(
                                        min_width=20,
                                        min_height=20,
                                        border_color=Color.MUTED_FG,
                                        hexpand=True,
                                        vexpand=True,
                                        border_radius=Radius.PILL,
                                        styles={"background-color": color},
                                    ),
                                )
                                for color in state.items
                            ],
                        ],
                    ),
                ],
            ),
        )

    @click("pick")
    async def on_pick(self, state: ColorPickerState):
        process = await asyncio.create_subprocess_exec(
            "hyprpicker", stdout=subprocess.PIPE
        )
        if process.returncode == 0:
            color = process.stdout.read().strip()
            self.log(f"color picked {color}")
            await self.set_state(items=[color, *state.items])


if __name__ == "__main__":
    try:
        ColorPickerApplet().run()
    except KeyboardInterrupt:
        pass
