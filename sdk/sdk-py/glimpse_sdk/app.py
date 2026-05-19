from __future__ import annotations

import asyncio
import fnmatch
import inspect
import json
import sys
from dataclasses import dataclass, is_dataclass
from typing import Any, Generic, TypeVar

from .events import CallbackEvent, InitEvent, OptionsT, PopoverEvent, parse_callback_event, parse_init_event
from .protocol import StatusItem
from .widgets import InlineHandler, TreeNode

StateT = TypeVar("StateT", bound="AppletState")


@dataclass(slots=True)
class AppletState:
    pass


@dataclass(slots=True)
class CommandResult:
    stdout: str
    stderr: str
    rc: int


class _InlineHandlerRegistry:
    def __init__(self) -> None:
        self.handlers: dict[tuple[str, str], InlineHandler] = {}

    def generated_id(self, event: str, path: tuple[int, ...]) -> str:
        suffix = ".".join(str(part) for part in path) if path else "root"
        return f"__glimpse:{event}:{suffix}"

    def add(self, event: str, target_id: str, handler: InlineHandler) -> None:
        self.handlers[(event, target_id)] = handler


class Applet(Generic[StateT, OptionsT]):
    def __init__(self) -> None:
        self.state: StateT = self.initial_state()
        self._incoming: asyncio.Queue[InitEvent[OptionsT] | CallbackEvent] = asyncio.Queue()
        self._outgoing: asyncio.Queue[tuple[str, dict[str, Any]]] = asyncio.Queue()
        self._handler_map, self._pattern_handlers = self._collect_handlers()
        self._inline_handler_map: dict[tuple[str, str], InlineHandler] = {}
        self._render_task: asyncio.Task[None] | None = None
        self._render_requested = False
        self._last_status: list[dict[str, Any]] | None = None
        self._last_tree: dict[str, Any] | None = None
        self._popover_open = False

    def initial_state(self) -> StateT:
        raise NotImplementedError

    def css_class(self) -> str | None:
        return None

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

    def log(self, *args: object) -> None:
        print(*args, file=sys.stderr, flush=True)

    async def run_command(self, command: list[str]) -> CommandResult:
        return await run_command(command)

    async def close_popover(self) -> None:
        sys.stdout.write("close_popover {}\n")
        sys.stdout.flush()

    async def copy_to_clipboard(self, text: str) -> None:
        await _run_desktop_command("wl-copy", [], text)

    async def open_uri(self, uri: str) -> None:
        await _run_desktop_command("xdg-open", [uri])

    async def show_notification(self, summary: str, body: str | None = None) -> None:
        args = [summary]
        if body is not None:
            args.append(body)
        await _run_desktop_command("notify-send", args)

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
            if widget is not None:
                registry = _InlineHandlerRegistry()
                widget.bind_handlers(registry, ())
                self._inline_handler_map = registry.handlers
            else:
                self._inline_handler_map = {}
            content = None if widget is None else widget.to_protocol()
            tree = {"root": content}
            if tree != self._last_tree:
                self._last_tree = tree
                await self._outgoing.put(("popover", tree))

    def _collect_handlers(
        self,
    ) -> tuple[dict[tuple[str, str], Any], list[tuple[tuple[str, str], Any]]]:
        exact: dict[tuple[str, str], Any] = {}
        patterns: list[tuple[tuple[str, str], Any]] = []
        for name in dir(self):
            value = getattr(self, name)
            handler_meta = getattr(value, "__glimpse_handler__", None)
            if handler_meta is None:
                continue
            ev_type, target_id = handler_meta
            if any(c in target_id for c in ("*", "?", "[")):
                patterns.append(((ev_type, target_id), value))
            else:
                exact[(ev_type, target_id)] = value
        return exact, patterns

    async def _dispatch_callback(self, event: CallbackEvent) -> None:
        inline_handler = self._inline_handler_map.get((event.event, event.id))
        if inline_handler is not None:
            result = inline_handler(self.state, event)
            if inspect.isawaitable(result):
                await result
            return

        handler = self._handler_map.get((event.event, event.id))
        if handler is None:
            for (ev_type, pat), h in self._pattern_handlers:
                if ev_type == event.event and fnmatch.fnmatch(event.id, pat):
                    handler = h
                    break
        if handler is not None:
            result = handler(event)
            if inspect.isawaitable(result):
                await result
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
        if (cls := self.css_class()) is not None:
            sys.stdout.write(f"class {cls}\n")
            sys.stdout.flush()
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


async def _run_desktop_command(
    command: str,
    args: list[str],
    stdin: str | None = None,
) -> None:
    result = await _run_command([command, *args], stdin)
    if result.rc != 0:
        raise RuntimeError(f"{command} exited with status {result.rc}")


async def run_command(command: list[str]) -> CommandResult:
    return await _run_command(command)


async def _run_command(command: list[str], stdin: str | None = None) -> CommandResult:
    if not command:
        raise ValueError("command must not be empty")
    process = await asyncio.create_subprocess_exec(
        command[0],
        *command[1:],
        stdin=asyncio.subprocess.PIPE if stdin is not None else asyncio.subprocess.DEVNULL,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    input_bytes = stdin.encode() if stdin is not None else None
    stdout, stderr = await process.communicate(input_bytes)
    return CommandResult(
        stdout=stdout.decode(errors="replace"),
        stderr=stderr.decode(errors="replace"),
        rc=process.returncode if process.returncode is not None else -1,
    )


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
