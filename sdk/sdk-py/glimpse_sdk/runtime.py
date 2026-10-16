import asyncio
import enum
import json
import os
import sys
import typing

from glimpse_sdk.protocol import MAX_LINE, LimitError, RenderError
from glimpse_sdk.session import App, Session


class _Closed(Exception):
    pass


class _Signal(enum.Enum):
    WAKE = enum.auto()
    EOF = enum.auto()


def run(app: App) -> None:
    path = os.environ.get("GLIMPSE_APPLET_SOCKET")

    if not path:
        print("GLIMPSE_APPLET_SOCKET is not set: run the applet under the panel", file=sys.stderr)
        sys.exit(1)

    try:
        asyncio.run(_serve(app, path))
    except BaseExceptionGroup as group:
        _exit(group)
    except Exception as error:
        _exit(error)


def _exit(error: BaseException) -> typing.NoReturn:
    while isinstance(error, BaseExceptionGroup):
        error = error.exceptions[0]

    if isinstance(error, BrokenPipeError | ConnectionResetError):
        sys.exit(0)

    print(error, file=sys.stderr)
    sys.exit(70 if isinstance(error, LimitError | RenderError) else 1)


async def _serve(app: App, path: str) -> None:
    try:
        reader, writer = await asyncio.open_unix_connection(path)
    except FileNotFoundError, ConnectionRefusedError:
        return

    await _Connection(reader, writer).serve(app)


class _Connection:
    def __init__(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
        self.reader = reader
        self.writer = writer
        self.loop = asyncio.get_running_loop()
        self.inbox: asyncio.Queue[bytes | _Signal] = asyncio.Queue()
        self.buffer = b""

    def write_line(self, line: str) -> None:
        self.loop.call_soon_threadsafe(self.writer.write, line.encode() + b"\n")

    def wake(self) -> None:
        self.loop.call_soon_threadsafe(self.inbox.put_nowait, _Signal.WAKE)

    async def serve(self, app: App) -> None:
        try:
            async with asyncio.TaskGroup() as group:
                session = Session(app, self.write_line, on_wake=self.wake, tasks=group)
                group.create_task(self._pump())
                session.send({"t": "hello", "v": 1})
                await self._dispatch(session)
        except* _Closed:
            pass
        finally:
            self.writer.close()

    async def _pump(self) -> None:
        while chunk := await self.reader.read(65536):
            self.inbox.put_nowait(chunk)

        self.inbox.put_nowait(_Signal.EOF)

    async def _dispatch(self, session: Session) -> None:
        while True:
            items = [await self.inbox.get()]

            while not self.inbox.empty():
                items.append(self.inbox.get_nowait())

            batch: list[bytes] = []
            closed = False

            for item in items:
                if item is _Signal.WAKE:
                    session.dirty = True
                elif item is _Signal.EOF:
                    closed = True
                else:
                    self.buffer += item
                    *lines, self.buffer = self.buffer.split(b"\n")
                    batch.extend(line for line in lines if line.strip())

                    if len(self.buffer) > MAX_LINE:
                        raise LimitError("incoming line exceeds 1 MiB")

            for line in batch:
                try:
                    msg = json.loads(line)
                except ValueError as error:
                    print(error, file=sys.stderr)
                    continue

                if isinstance(msg, dict) and isinstance(msg.get("t"), str):
                    session.handle(msg)

            session.flush()
            await self.writer.drain()

            if closed:
                raise _Closed
