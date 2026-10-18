import asyncio
import collections.abc
import dataclasses
import subprocess
import sys
import threading
import typing

from glimpse_sdk.elements import Urgency

if typing.TYPE_CHECKING:
    from glimpse_sdk.session import Session


@dataclasses.dataclass(frozen=True)
class AppletEntry:
    id: str
    name: str
    icon: str | None


def notify_argv(
    name: str,
    entry: AppletEntry | None,
    summary: str,
    body: str | None,
    icon: str | None,
    urgency: str | None,
) -> list[str]:
    app = entry.name if entry is not None and entry.name else name
    level = urgency if urgency in ("low", "normal", "critical") else "normal"
    argv = ["notify-send", f"--app-name={app}", f"--urgency={level}"]
    resolved_icon = icon or (entry.icon if entry is not None else None)

    if resolved_icon:
        argv.append(f"--icon={resolved_icon}")

    if entry is not None and entry.id:
        argv.append(f"--hint=string:desktop-entry:{entry.id.removesuffix('.desktop')}")

    argv.append("--")
    argv.append(summary)

    if body:
        argv.append(body)

    return argv


def _spawn(program: str, args: list[str], stdin: str | None = None) -> None:
    try:
        proc = subprocess.Popen(
            [program, *args],
            stdin=subprocess.PIPE if stdin is not None else subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

        if stdin is not None:
            threading.Thread(target=proc.communicate, args=(stdin.encode(),), daemon=True).start()
    except OSError as error:
        print(f"glimpse: failed to run {program}: {error}", file=sys.stderr)


class Context:
    def __init__(self, session: Session) -> None:
        self._session = session
        self.name = ""
        self.state_dir = ""
        self.options: dict[str, typing.Any] = {}
        self.placement: dict[str, typing.Any] | None = None
        self.popover_open = False
        self.entry: AppletEntry | None = None

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
        argv = notify_argv(self.name, self.entry, summary, body, icon, urgency)
        _spawn(argv[0], argv[1:])

    def copy(self, text: str) -> None:
        _spawn("wl-copy", [], stdin=text)

    def open_uri(self, uri: str) -> None:
        self._session.send({"t": "open-uri", "uri": uri})

    def launch(self, desktop_id: str) -> None:
        self._session.send({"t": "launch", "id": desktop_id})

    def close_popover(self) -> None:
        self._session.send({"t": "close-popover"})
