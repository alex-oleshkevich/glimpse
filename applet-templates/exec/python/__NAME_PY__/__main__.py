from dataclasses import dataclass

from glimpse_sdk import (
    Applet,
    AppletState,
    Column,
    Hero,
    Label,
    StatusItem,
    Tile,
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
                    title="__NAME__",
                    subtitle=f"Value: {state.count}",
                ),
                Label(label=f"Count = {state.count}"),
                Tile(
                    primary="Increment",
                    left_icon="list-add-symbolic",
                    on_click=self.on_increment,
                ),
            ],
        )

    async def on_increment(self, state: CounterState, _event) -> None:
        await self.set_state(count=state.count + 1)


if __name__ == "__main__":
    try:
        CounterApplet().run()
    except KeyboardInterrupt:
        pass
