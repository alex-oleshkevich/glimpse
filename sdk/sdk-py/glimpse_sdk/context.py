import asyncio
import collections.abc
import time
import typing

from glimpse_sdk.elements import Urgency

if typing.TYPE_CHECKING:
    from glimpse_sdk.session import Session


type Target = str | int | dict[str, typing.Any]


def position_now(player: dict[str, typing.Any], now_ms: float | None = None) -> float:
    if player.get("playback") != "playing":
        return float(player.get("positionUs", 0))

    now = time.time() * 1000 if now_ms is None else now_ms
    position = player["positionUs"] + (now - player["positionAt"]) * 1000 * player.get("rate", 1)
    length = player.get("lengthUs")
    return float(min(position, length) if length else position)


class Context:
    def __init__(self, session: Session) -> None:
        self._session = session
        self.name = ""
        self.state_dir = ""
        self.options: dict[str, typing.Any] = {}
        self.placement: dict[str, typing.Any] | None = None
        self.popover_open = False
        self.services: dict[str, typing.Any] = {}

    @property
    def mpris(self) -> list[dict[str, typing.Any]] | None:
        value = self.services.get("mpris")
        return value.get("players") if isinstance(value, dict) else None

    @property
    def compositor(self) -> dict[str, typing.Any] | None:
        return self.services.get("compositor")

    def spawn[T](
        self, coro: collections.abc.Coroutine[typing.Any, typing.Any, T]
    ) -> asyncio.Task[T]:
        return self._session.spawn(coro)

    def wake(self) -> None:
        self._session.wake()

    def notify(
        self,
        summary: str,
        body: str | None = None,
        icon: str | None = None,
        urgency: Urgency | None = None,
    ) -> None:
        msg: dict[str, typing.Any] = {"t": "notify", "summary": summary}

        for key, value in (("body", body), ("icon", icon), ("urgency", urgency)):
            if value is not None:
                msg[key] = value

        self._session.send(msg)

    def copy(self, text: str) -> None:
        self._session.send({"t": "copy", "text": text})

    def open_uri(self, uri: str) -> None:
        self._session.send({"t": "open-uri", "uri": uri})

    def launch(self, desktop_id: str) -> None:
        self._session.send({"t": "launch", "id": desktop_id})

    def close_popover(self) -> None:
        self._session.send({"t": "close-popover"})

    def mpris_control(self, player: str, action: str) -> None:
        self._session.send({"t": "mpris", "player": player, "op": "control", "action": action})

    def mpris_seek(self, player: str, offset_us: float) -> None:
        self._session.send(
            {"t": "mpris", "player": player, "op": "seek", "offset": round(offset_us)}
        )

    def mpris_set_position(self, player: str, position_us: float) -> None:
        self._session.send(
            {"t": "mpris", "player": player, "op": "set-position", "position": round(position_us)}
        )

    def focus_workspace(self, target: Target) -> None:
        self._session.send({"t": "compositor", "op": "focus-workspace", "target": target})

    def focus_window(self, target: Target) -> None:
        self._session.send({"t": "compositor", "op": "focus-window", "target": target})

    def move_window(self, window: int, target: Target) -> None:
        self._session.send(
            {"t": "compositor", "op": "move-window", "window": window, "target": target}
        )

    def rename_workspace(self, workspace_id: int, name: str | None) -> None:
        self._session.send(
            {"t": "compositor", "op": "rename-workspace", "id": workspace_id, "name": name}
        )

    def close_window(self, window_id: int) -> None:
        self._session.send({"t": "compositor", "op": "close-window", "id": window_id})
