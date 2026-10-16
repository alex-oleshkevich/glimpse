import asyncio
import collections.abc
import dataclasses
import inspect
import json
import math
import threading
import typing

from glimpse_sdk.context import Context
from glimpse_sdk.protocol import (
    CONTROLLED,
    MAX_CHILDREN,
    MAX_ITEMS,
    MAX_LINE,
    MAX_NODES,
    MAX_OPS,
    MAX_SERIES,
    LimitError,
    RenderError,
    _flatten,
    _oversized,
)
from glimpse_sdk.widget import W


@dataclasses.dataclass(slots=True)
class El:
    id: int
    kind: str
    key: typing.Any = None
    props: dict[str, typing.Any] = dataclasses.field(default_factory=dict)
    handlers: dict[str, collections.abc.Callable[..., typing.Any]] = dataclasses.field(
        default_factory=dict
    )
    children: list[El] = dataclasses.field(default_factory=list)
    applied_seq: float | None = None
    confirm_seq: float | None = None


class App(typing.Protocol):
    def build(self, ctx: Context) -> collections.abc.Sequence[W]: ...
    def mount(self, ctx: Context) -> None: ...
    async def start(self, ctx: Context) -> None: ...


class Session:
    def __init__(
        self,
        app: App,
        write_line: collections.abc.Callable[[str], None],
        *,
        on_wake: collections.abc.Callable[[], None] | None = None,
        tasks: asyncio.TaskGroup | None = None,
    ) -> None:
        self.app = app
        self.write_line = write_line
        self.on_wake = on_wake
        self.tasks = tasks
        self._write_lock = threading.Lock()
        self.ctx = Context(self)
        self.root = El(0, "root")
        self.nodes: dict[int, El] = {}
        self.next_id = 1
        self.dirty = True
        self.mounted = False

    def send(self, msg: dict[str, typing.Any]) -> None:
        line = json.dumps(msg, separators=(",", ":"))

        if len(line.encode()) > MAX_LINE:
            raise LimitError("outgoing line exceeds 1 MiB")

        with self._write_lock:
            self.write_line(line)

    def spawn[T](
        self, coro: collections.abc.Coroutine[typing.Any, typing.Any, T]
    ) -> asyncio.Task[T]:
        if self.tasks is None:
            coro.close()
            raise RuntimeError("async handlers need the runtime loop")

        task = self.tasks.create_task(coro)
        task.add_done_callback(lambda _: self.wake())
        return task

    def wake(self) -> None:
        self.dirty = True

        if self.on_wake is not None:
            self.on_wake()

    def _new_el(self, widget: W) -> El:
        if len(widget.children) > MAX_CHILDREN:
            raise LimitError("parent exceeds 200 children")

        el = El(self.next_id, widget.kind, widget.key, dict(widget.props), dict(widget.handlers))
        self.next_id += 1

        el.children = [self._new_el(child) for child in widget.children]
        self.nodes[el.id] = el
        return el

    def _serialize(self, el: El) -> dict[str, typing.Any]:
        return {
            "id": el.id,
            "type": el.kind,
            "props": el.props,
            "children": [self._serialize(child) for child in el.children],
        }

    def _forget(self, el: El) -> None:
        self.nodes.pop(el.id, None)

        for child in el.children:
            self._forget(child)

    def _diff(self, el: El, widget: W, ops: list[typing.Any]) -> None:
        el.handlers = dict(widget.handlers)
        diff = {
            key: widget.props.get(key)
            for key in el.props.keys() | widget.props.keys()
            if el.props.get(key) != widget.props.get(key)
            or (key in el.props) != (key in widget.props)
        }

        if diff:
            op: dict[str, typing.Any] = {"op": "set", "id": el.id, "props": diff}
            controlled = CONTROLLED.get(el.kind)

            if el.confirm_seq is not None and controlled and controlled[1] in diff:
                op["seq"] = el.confirm_seq
                el.confirm_seq = None

            ops.append(op)
            el.props = dict(widget.props)

        self._reconcile(el, widget.children, ops)

    def _reconcile(self, parent: El, widgets: list[W], ops: list[typing.Any]) -> None:
        if len(widgets) > MAX_CHILDREN:
            raise LimitError("parent exceeds 200 children")

        old = parent.children
        keyed = {el.key: el for el in old if el.key is not None}
        unkeyed = [el for el in old if el.key is None]
        wanted: list[tuple[El, bool]] = []
        used: set[int] = set()
        cursor = 0

        for widget in widgets:
            match_el = None

            if widget.key is not None:
                candidate = keyed.get(widget.key)

                if (
                    candidate is not None
                    and candidate.kind == widget.kind
                    and id(candidate) not in used
                ):
                    match_el = candidate
            elif cursor < len(unkeyed):
                candidate = unkeyed[cursor]
                cursor += 1

                if candidate.kind == widget.kind:
                    match_el = candidate

            if match_el is None:
                wanted.append((self._new_el(widget), True))
            else:
                used.add(id(match_el))
                self._diff(match_el, widget, ops)
                wanted.append((match_el, False))

        for el in old:
            if id(el) not in used:
                ops.append({"op": "remove", "parent": parent.id, "id": el.id})
                self._forget(el)

        current = [el for el in old if id(el) in used]

        for index, (el, fresh) in enumerate(wanted):
            before = current[index].id if index < len(current) else None

            if fresh:
                ops.append(
                    {
                        "op": "insert",
                        "parent": parent.id,
                        "before": before,
                        "node": self._serialize(el),
                    }
                )
                current.insert(index, el)
            elif index >= len(current) or current[index] is not el:
                ops.append({"op": "move", "parent": parent.id, "id": el.id, "before": before})
                current.remove(el)
                current.insert(index, el)

        parent.children = [el for el, _ in wanted]

    def rebuild(self) -> None:
        try:
            tree = self.app.build(self.ctx)
        except Exception as error:
            raise RenderError(str(error)) from error

        ops: list[typing.Any] = []
        self._reconcile(self.root, _flatten([tree]), ops)

        if len(self.nodes) + 1 > MAX_NODES:
            raise LimitError("tree exceeds 300 nodes")

        for el in self.nodes.values():
            if any(
                _oversized(v, MAX_SERIES if k in ("values", "tooltips") else MAX_ITEMS)
                for k, v in el.props.items()
            ):
                raise LimitError("array prop exceeds 64 items")

            controlled = CONTROLLED.get(el.kind)

            if el.confirm_seq is not None and controlled:
                ops.append(
                    {
                        "op": "set",
                        "id": el.id,
                        "props": {controlled[1]: el.props.get(controlled[1])},
                        "seq": el.confirm_seq,
                    }
                )
                el.confirm_seq = None

        if len(ops) > MAX_OPS:
            raise LimitError("commit exceeds 1200 operations")

        if ops:
            self.send({"t": "commit", "ops": ops})

        self.dirty = False

    def handle(self, msg: dict[str, typing.Any]) -> None:
        match msg.get("t"):
            case "hello" if not self.mounted:
                if msg.get("v") != 1:
                    raise RuntimeError(f"unsupported host protocol version: {msg.get('v')}")

                self.ctx.name = msg.get("name") or ""
                self.ctx.state_dir = msg.get("state") or ""
                self.ctx.options = msg.get("options") or {}
                self.ctx.placement = msg.get("placement")
                self.mounted = True
                self.dirty = True

                self.app.mount(self.ctx)

            case _ if not self.mounted:
                return
            case "hello":
                return
            case "options":
                self.ctx.options = msg.get("options") or {}
                self.dirty = True
            case "placement":
                self.ctx.placement = msg.get("placement")
                self.dirty = True
            case "popover":
                self.ctx.popover_open = msg.get("open") is True
                self.dirty = True
            case "service" if isinstance(msg.get("name"), str):
                self.ctx.services[msg["name"]] = msg.get("value")
                self.dirty = True
            case "event":
                self._event(msg)

    def _event(self, msg: dict[str, typing.Any]) -> None:
        el_id = msg.get("id")
        el = (
            self.nodes.get(el_id)
            if isinstance(el_id, int) and not isinstance(el_id, bool)
            else None
        )

        if el is None:
            return

        name = msg.get("name")
        handler = el.handlers.get(name) if isinstance(name, str) else None

        if handler is None:
            return

        controlled = CONTROLLED.get(el.kind)
        seq = msg.get("seq")

        if (
            controlled
            and controlled[0] == msg.get("name")
            and isinstance(seq, (int, float))
            and not isinstance(seq, bool)
            and math.isfinite(seq)
        ):
            if el.applied_seq is not None and seq < el.applied_seq:
                return

            el.applied_seq = seq
            el.confirm_seq = seq

        args = msg.get("args")
        result = handler(*(args if isinstance(args, list) else []))

        if inspect.iscoroutine(result):
            self.spawn(result)

        self.dirty = True

    def flush(self) -> None:
        if self.mounted and self.dirty:
            self.rebuild()
