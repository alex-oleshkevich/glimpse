import abc
import collections.abc
import contextlib
import dataclasses
import json
import pathlib
import typing

from glimpse_sdk.context import Context
from glimpse_sdk.runtime import run
from glimpse_sdk.widget import W


@dataclasses.dataclass(frozen=True)
class State:
    pass


class BaseApplet[S: State](abc.ABC):
    state: S
    persist: typing.ClassVar[tuple[str, ...]] = ()

    _wake: collections.abc.Callable[[], None] | None = None
    _state_file: pathlib.Path | None = None

    @abc.abstractmethod
    def build(self, ctx: Context) -> collections.abc.Sequence[W]: ...

    async def start(self, ctx: Context) -> None:
        return

    def mount(self, ctx: Context) -> None:
        self._wake = ctx.wake

        if ctx.state_dir:
            self._state_file = pathlib.Path(ctx.state_dir) / "state.json"

            with contextlib.suppress(OSError, ValueError, TypeError):
                saved = json.loads(self._state_file.read_text())
                self.state = dataclasses.replace(
                    self.state, **{k: v for k, v in saved.items() if k in self.persist}
                )

    def set_state(self, **changes: typing.Any) -> None:
        new = dataclasses.replace(self.state, **changes)

        if new == self.state:
            return

        self.state = new

        if self._state_file is not None and any(name in self.persist for name in changes):
            self._state_file.write_text(
                json.dumps({name: getattr(new, name) for name in self.persist})
            )

        if self._wake is not None:
            self._wake()

    @classmethod
    def main(cls) -> None:
        run(cls())
