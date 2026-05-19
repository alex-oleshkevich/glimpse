from __future__ import annotations

from dataclasses import dataclass

from glimpse_sdk import (
    Applet,
    AppletState,
    Button,
    ButtonVariant,
    Column,
    Hero,
    StatusItem,
    Text,
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
        return Column(
            children=[
                Hero(
                    icon="view-refresh-symbolic",
                    title="Counter",
                    subtitle=f"Value: {state.count}",
                ),
                Text(text=f"Count = {state.count}"),
                Button(
                    label="Increment",
                    on_click=self.on_increment,
                    icon="list-add-symbolic",
                    variant=ButtonVariant.PRIMARY,
                ),
            ],
            spacing=8,
        )

    async def on_increment(self, state: CounterState, _event) -> None:
        await self.set_state(count=state.count + 1)


if __name__ == "__main__":
    CounterApplet().run()
