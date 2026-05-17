from __future__ import annotations

from dataclasses import dataclass

from glimpse_sdk import (
    Applet,
    AppletState,
    Box,
    Button,
    ButtonVariant,
    Hero,
    Label,
    StatusItem,
    click,
)


@dataclass
class CounterState(AppletState):
    count: int = 0


class CounterApplet(Applet[CounterState]):
    def initial_state(self) -> CounterState:
        return CounterState()

    async def status(self, state: CounterState):
        return [
            StatusItem(
                id="counter",
                icon="view-refresh-symbolic",
                label=str(state.count),
            )
        ]

    async def popover(self, state: CounterState):
        return Box.vertical(
            [
                Hero(
                    icon="view-refresh-symbolic",
                    title="Counter",
                    subtitle=f"Value: {state.count}",
                ),
                Label(text=f"Count = {state.count}"),
                Button(
                    id="increment",
                    label="Increment",
                    icon="list-add-symbolic",
                    variant=ButtonVariant.PRIMARY,
                ),
            ],
            spacing=8,
        )

    @click("increment")
    async def on_increment(self, _event) -> None:
        await self.set_state(count=self.state.count + 1)


if __name__ == "__main__":
    CounterApplet().run()
