from __future__ import annotations

import asyncio
import json
import sys
from dataclasses import dataclass, is_dataclass
from typing import Any, Generic, TypeVar

from .events import CallbackEvent, InitEvent, OptionsT, PopoverEvent, parse_callback_event, parse_init_event
from .protocol import StatusItem
from .widgets import TreeNode

StateT = TypeVar("StateT", bound="AppletState")


@dataclass(slots=True)
class AppletState:
    pass


class Applet(Generic[StateT, OptionsT]):
    def __init__(self) -> None:
        self.state: StateT = self.initial_state()
        self._incoming: asyncio.Queue[InitEvent[OptionsT] | CallbackEvent] = asyncio.Queue()
        self._outgoing: asyncio.Queue[tuple[str, dict[str, Any]]] = asyncio.Queue()
        self._handler_map = self._collect_handlers()
        self._render_task: asyncio.Task[None] | None = None
        self._render_requested = False
        self._last_status: list[dict[str, Any]] | None = None
        self._last_tree: dict[str, Any] | None = None
        self._popover_open = False

    def initial_state(self) -> StateT:
        raise NotImplementedError

    async def on_start(self) -> None:
        return None

    def parse_options(self, raw: dict[str, Any]) -> OptionsT:
        return raw  # type: ignore[return-value]

    async def on_init(self, _event: InitEvent[OptionsT]) -> None:
        return None

    async def on_callback(self, _event: CallbackEvent) -> None:
        return None

    async def status(self, state: StateT) -> list[StatusItem]:
        return []

    async def popover(self, state: StateT) -> TreeNode | None:
        return None

    async def set_state(self, **kwargs: Any) -> None:
        for key, value in kwargs.items():
            if not hasattr(self.state, key):
                raise AttributeError(f"Unknown state field: {key}")
            setattr(self.state, key, value)
        self._schedule_render()
        await asyncio.sleep(0)

    def is_popover_open(self) -> bool:
        return self._popover_open

    def _schedule_render(self) -> None:
        self._render_requested = True
        if self._render_task is None or self._render_task.done():
            self._render_task = asyncio.create_task(self._flush_render())
            self._render_task.add_done_callback(_log_render_exception)

    async def _flush_render(self) -> None:
        await asyncio.sleep(0)
        while self._render_requested:
            self._render_requested = False
            status_items = await self.status(self.state)
            status = [item.to_protocol() for item in status_items]
            if status != self._last_status:
                self._last_status = status
                await self._outgoing.put(("status", {"items": status}))

            widget = await self.popover(self.state)
            content = None if widget is None else widget.to_protocol()
            tree = {"root": content}
            # Emit only when popover open, or first render, or clearing.
            should_emit = (
                self._popover_open or self._last_tree is None or content is None
            )
            if should_emit and tree != self._last_tree:
                self._last_tree = tree
                await self._outgoing.put(("popover", tree))

    def _collect_handlers(self) -> dict[tuple[str, str], Any]:
        handlers: dict[tuple[str, str], Any] = {}
        for name in dir(self):
            value = getattr(self, name)
            handler_meta = getattr(value, "__glimpse_handler__", None)
            if handler_meta is not None:
                handlers[handler_meta] = value
        return handlers

    async def _dispatch_callback(self, event: CallbackEvent) -> None:
        handler = self._handler_map.get((event.event, event.id))
        if handler is not None:
            await handler(event)
        else:
            await self.on_callback(event)

    async def _reader_loop(self, eof: asyncio.Event) -> None:
        transport: asyncio.BaseTransport | None = None
        try:
            try:
                reader = asyncio.StreamReader()
                protocol = asyncio.StreamReaderProtocol(reader)
                transport, _ = await asyncio.get_running_loop().connect_read_pipe(
                    lambda: protocol,
                    sys.stdin,
                )
            except (NotImplementedError, OSError, ValueError):
                await self._reader_loop_threaded()
                return

            while True:
                raw = await reader.readline()
                if raw == b"":
                    break
                line = raw.decode(errors="replace")
                await self._handle_input_line(line)
        finally:
            if transport is not None:
                transport.close()
            eof.set()

    async def _reader_loop_threaded(self) -> None:
        while True:
            line = await asyncio.to_thread(sys.stdin.readline)
            if line == "":
                break
            await self._handle_input_line(line)

    async def _handle_input_line(self, line: str) -> None:
        try:
            parsed = _parse_line(line)
        except (ValueError, json.JSONDecodeError) as exc:
            print(f"glimpse-sdk: ignoring malformed input: {exc}", file=sys.stderr)
            return
        if parsed is None:
            return
        message_type, data = parsed
        try:
            if message_type == "init":
                raw = parse_init_event(data)
                typed: InitEvent[OptionsT] = InitEvent(instance=raw.instance, options=self.parse_options(raw.options))
                await self._incoming.put(typed)
            elif message_type == "event":
                await self._incoming.put(parse_callback_event(data))
        except Exception as exc:
            print(f"glimpse-sdk: ignoring malformed event: {exc}", file=sys.stderr)
            return

    async def _writer_loop(self) -> None:
        while True:
            command, payload = await self._outgoing.get()
            try:
                sys.stdout.write(f"{command} {json.dumps(payload, separators=(',', ':'))}\n")
                sys.stdout.flush()
            except (BrokenPipeError, OSError):
                return

    async def _event_loop(self, eof: asyncio.Event) -> None:
        await self.on_start()
        self._schedule_render()
        get_task: asyncio.Task[InitEvent | CallbackEvent] | None = None
        eof_task = asyncio.create_task(eof.wait())
        try:
            while True:
                if get_task is None:
                    get_task = asyncio.create_task(self._incoming.get())
                done, _ = await asyncio.wait(
                    {get_task, eof_task}, return_when=asyncio.FIRST_COMPLETED
                )
                if eof_task in done and get_task not in done:
                    get_task.cancel()
                    return
                event = get_task.result()
                get_task = None
                if isinstance(event, InitEvent):
                    await self.on_init(event)
                    self._schedule_render()
                else:
                    if isinstance(event, PopoverEvent):
                        self._popover_open = event.open
                    await self._dispatch_callback(event)
                    self._schedule_render()
                if self._render_task is not None and self._render_task.done():
                    exc = self._render_task.exception()
                    if exc is not None:
                        raise exc
                await asyncio.sleep(0)
        finally:
            if get_task is not None:
                get_task.cancel()
            eof_task.cancel()

    async def _run(self) -> None:
        if not is_dataclass(self.state):
            raise TypeError("Applet state must be a dataclass instance")
        eof = asyncio.Event()
        writer = asyncio.create_task(self._writer_loop())
        reader = asyncio.create_task(self._reader_loop(eof))
        try:
            await self._event_loop(eof)
        finally:
            reader.cancel()
            writer.cancel()
            if self._render_task is not None and not self._render_task.done():
                self._render_task.cancel()

    def run(self) -> None:
        try:
            asyncio.run(self._run())
        except KeyboardInterrupt:
            pass


def _log_render_exception(task: "asyncio.Task[None]") -> None:
    if task.cancelled():
        return
    exc = task.exception()
    if exc is None:
        return
    import traceback

    print("glimpse-sdk: render error:", file=sys.stderr)
    traceback.print_exception(type(exc), exc, exc.__traceback__, file=sys.stderr)


def _parse_line(line: str) -> tuple[str, dict[str, Any]] | None:
    stripped = line.strip()
    if not stripped:
        return None
    command, _, payload = stripped.partition(" ")
    if not payload:
        raise ValueError("missing command payload")
    data = json.loads(payload)
    if not isinstance(data, dict):
        raise ValueError("command payload must be an object")
    return command, data
