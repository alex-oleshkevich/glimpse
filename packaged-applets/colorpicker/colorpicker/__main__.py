# /// script
# requires-python = ">=3.14"
# dependencies = ["glimpse-applet-sdk>=0.2,<1"]
# ///

import asyncio
import shutil
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
)
from glimpse_sdk.widgets import Align, Button


@dataclass
class ColorPickerState(AppletState):
    items: list[str]


class ColorPickerApplet(Applet[ColorPickerState]):
    def initial_state(self) -> ColorPickerState:
        return ColorPickerState(items=[])

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

        recent_colors = Text(text="No recent colors.")
        if state.items:
            recent_colors = Column(
                spacing=0,
                children=[
                    Container(
                        halign=Align.START,
                        child=Text(text="Recent colors", css_classes=["header"]),
                    ),
                    *[
                        ActionItem(
                            id=f"pick_{color}",
                            label=color,
                            on_click=lambda _, _2, c=color: self.copy_to_clipboard(c),
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
                        label="Pick color",
                        on_click=self.pick_color,
                        variant=ButtonVariant.PRIMARY,
                        hexpand=True,
                    ),
                    recent_colors,
                ],
            ),
        )

    async def pick_color(self, state: ColorPickerState, _event: object):
        import os

        await self.close_popover()
        await asyncio.sleep(0.3)

        process = await asyncio.create_subprocess_exec(  # noqa: S603
            "hyprpicker",
            "--render-inactive",
            stdin=asyncio.subprocess.DEVNULL,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=os.environ,
        )
        stdout, stderr = await process.communicate()
        self.log(stderr.decode())
        if process.returncode == 0:
            color = stdout.decode().strip()
            await self.set_state(items=[color, *state.items])


if __name__ == "__main__":
    try:
        ColorPickerApplet().run()
    except KeyboardInterrupt:
        pass
