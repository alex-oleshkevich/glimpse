from dataclasses import dataclass

from glimpse_sdk import (
    Applet,
    AppletState,
    Button,
    Column,
    Hero,
    Icon,
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
                icon=Icon.name("view-refresh-symbolic"),
                label=str(state.count),
            )
        ]

    async def popover(self, state: CounterState):
        return Column(
            spacing=8,
            children=[
                Hero(title="__NAME__", subtitle=f"Value: {state.count}"),
                Button(id="increment", label="Increment"),
            ],
        )

    @click("increment")
    async def on_increment(self, _event) -> None:
        await self.set_state(count=self.state.count + 1)


if __name__ == "__main__":
    try:
        CounterApplet().run()
    except KeyboardInterrupt:
        pass
