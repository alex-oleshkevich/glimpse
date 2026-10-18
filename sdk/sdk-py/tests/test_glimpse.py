import asyncio
import collections.abc
import contextlib
import dataclasses
import functools
import io
import json
import os
import socket
import tempfile
import threading
import time
import typing
import unittest
from unittest import mock

import glimpse_sdk as glimpse
from glimpse_sdk import (
    AppletEntry,
    BaseApplet,
    Context,
    Entry,
    Indicator,
    Popover,
    Section,
    Session,
    State,
    ToggleRow,
    W,
)
from glimpse_sdk.context import _spawn, notify_argv
from glimpse_sdk.session import App as AppProtocol

Node = dict[str, typing.Any]


def discard(line: str) -> None:
    pass


class EmptyApp(BaseApplet[State]):
    def build(self, ctx: Context) -> list[W]:
        return []


@dataclasses.dataclass
class Item:
    id: str
    title: str
    done: bool


class App(BaseApplet[State]):
    def __init__(self) -> None:
        self.items = [Item("a", "Alpha", False), Item("b", "Bravo", False)]
        self.draft = ""

    def build(self, ctx: Context) -> list[W]:
        return [
            Indicator(icon="view-list-symbolic", badge="2", children=["Todo"]),
            Popover(
                title="Todo",
                children=[
                    Section(
                        title="Todo",
                        children=[
                            ToggleRow(
                                key=i.id,
                                title=i.title,
                                control="check",
                                active=i.done,
                                on_toggle=functools.partial(setattr, i, "done"),
                            )
                            for i in self.items
                        ],
                    ),
                    Entry(
                        placeholder="Add",
                        value=self.draft,
                        on_change=lambda v: setattr(self, "draft", v),
                    ),
                ],
            ),
        ]


HELLO = {
    "t": "hello",
    "v": 1,
    "name": "test",
    "state": "/tmp/x",
    "options": {},
    "placement": {
        "output": None,
        "position": "bottom",
        "orientation": "horizontal",
        "zone": "left",
        "size": 40,
    },
}


class Host:
    def __init__(self, app: App) -> None:
        self.lines: list[str] = []
        self.session = Session(app, self.lines.append)
        self.nodes: dict[int, Node] = {0: {"id": 0, "type": "root", "props": {}, "children": []}}
        self.session.send({"t": "hello", "v": 1})

    def feed(self, msg: Node) -> list[Node]:
        self.session.handle(msg)
        self.session.flush()
        commits = [json.loads(line) for line in self.lines if '"commit"' in line]
        self.lines[:] = [line for line in self.lines if '"commit"' not in line]

        for commit in commits:
            self.apply(commit["ops"])

        return commits

    def apply(self, ops: list[Node]) -> None:
        for op in ops:
            if op["op"] == "insert":
                parent = self.nodes[op["parent"]]
                node = op["node"]
                stack = [node]

                while stack:
                    current = stack.pop()
                    self.nodes[current["id"]] = current
                    stack.extend(current["children"])

                index = (
                    len(parent["children"])
                    if op["before"] is None
                    else next(
                        i for i, c in enumerate(parent["children"]) if c["id"] == op["before"]
                    )
                )
                parent["children"].insert(index, node)
            elif op["op"] == "remove":
                parent = self.nodes[op["parent"]]
                parent["children"] = [c for c in parent["children"] if c["id"] != op["id"]]
                stack = [self.nodes.pop(op["id"])]

                while stack:
                    stack.extend(
                        self.nodes.pop(c["id"], {"children": []}) for c in stack.pop()["children"]
                    )
            elif op["op"] == "move":
                node = self.nodes[op["id"]]

                for parent in self.nodes.values():
                    parent["children"] = [c for c in parent["children"] if c["id"] != op["id"]]

                parent = self.nodes[op["parent"]]
                index = (
                    len(parent["children"])
                    if op["before"] is None
                    else next(
                        i for i, c in enumerate(parent["children"]) if c["id"] == op["before"]
                    )
                )
                parent["children"].insert(index, node)
            elif op["op"] == "set":
                props = self.nodes[op["id"]]["props"]

                for key, value in op["props"].items():
                    if value is None:
                        props.pop(key, None)
                    else:
                        props[key] = value

    def titles(self) -> list[str]:
        return [n["props"].get("title") for n in self.nodes.values() if n["type"] == "togglerow"]

    def find(self, kind: str, title: str | None = None) -> Node:
        for node in self.nodes.values():
            if node["type"] == kind and (title is None or node["props"].get("title") == title):
                return node

        raise AssertionError(f"no {kind} {title}")


class TestSession(unittest.TestCase):
    def setUp(self) -> None:
        self.app = App()
        self.host = Host(self.app)
        self.host.feed(HELLO)

    def test_mount_builds_tree(self) -> None:
        self.assertEqual(sorted(self.host.titles()), ["Alpha", "Bravo"])
        self.assertEqual(self.host.find("indicator")["props"]["text"], "Todo")
        self.assertTrue(self.host.find("togglerow", "Alpha")["props"]["onToggle"])

    def test_toggle_event_sets_only_changed_prop(self) -> None:
        node = self.host.find("togglerow", "Alpha")
        commits = self.host.feed(
            {"t": "event", "id": node["id"], "name": "onToggle", "args": [True], "seq": 7}
        )
        ops = [op for commit in commits for op in commit["ops"]]
        self.assertEqual(len(ops), 1)
        self.assertEqual(ops[0]["op"], "set")
        self.assertEqual(ops[0]["props"], {"active": True})
        self.assertEqual(ops[0]["seq"], 7)
        self.assertTrue(self.app.items[0].done)

    def test_stale_seq_is_dropped(self) -> None:
        node = self.host.find("togglerow", "Alpha")
        self.host.feed(
            {"t": "event", "id": node["id"], "name": "onToggle", "args": [True], "seq": 7}
        )
        self.host.feed(
            {"t": "event", "id": node["id"], "name": "onToggle", "args": [False], "seq": 3}
        )
        self.assertTrue(self.app.items[0].done)

    def test_keyed_reorder_moves(self) -> None:
        before = {t: self.host.find("togglerow", t)["id"] for t in ("Alpha", "Bravo")}
        self.app.items.reverse()
        self.host.session.dirty = True
        commits = self.host.feed({"t": "popover", "open": True})
        ops = [op for commit in commits for op in commit["ops"]]
        self.assertTrue(any(op["op"] == "move" for op in ops))
        self.assertEqual({t: self.host.find("togglerow", t)["id"] for t in before}, before)
        section = self.host.find("section")
        self.assertEqual([c["props"]["title"] for c in section["children"]], ["Bravo", "Alpha"])

    def test_entry_change_confirms_seq(self) -> None:
        node = self.host.find("entry")
        commits = self.host.feed(
            {"t": "event", "id": node["id"], "name": "onChange", "args": ["hi"], "seq": 4}
        )
        ops = [op for commit in commits for op in commit["ops"]]
        self.assertEqual(ops[0]["props"], {"value": "hi"})
        self.assertEqual(ops[0]["seq"], 4)

    def test_rejected_edit_still_confirms(self) -> None:
        node = self.host.find("entry")
        self.app.draft = "fixed"
        commits = self.host.feed(
            {"t": "event", "id": node["id"], "name": "onChange", "args": ["fixed"], "seq": 9}
        )
        ops = [op for commit in commits for op in commit["ops"]]
        self.assertTrue(any(op.get("seq") == 9 for op in ops))

    def test_remaining_host_requests_reach_the_wire(self) -> None:
        ctx = self.host.session.ctx
        ctx.open_uri("https://example.org")
        ctx.launch("org.example.App")
        ctx.close_popover()
        sent = [json.loads(line) for line in self.host.lines if '"hello"' not in line]
        self.assertEqual(
            sent,
            [
                {"t": "open-uri", "uri": "https://example.org"},
                {"t": "launch", "id": "org.example.App"},
                {"t": "close-popover"},
            ],
        )

    def test_non_finite_numbers_become_null(self) -> None:
        widget = glimpse.Scale(value=float("nan"), min=0, max=float("inf"))
        self.assertEqual(widget.props, {"value": None, "min": 0, "max": None})

    def test_string_caps_match_the_host(self) -> None:
        widget = glimpse.Indicator(badge="123456789", tooltip="x" * 300, children=["y" * 300])
        self.assertEqual(len(widget.props["badge"]), 8)
        self.assertEqual(len(widget.props["tooltip"]), 256)
        self.assertEqual(len(widget.props["text"]), 256)
        entry = glimpse.Entry(value="z" * 5000)
        self.assertEqual(len(entry.props["value"]), 4096)

    def test_render_throw_is_a_render_error(self) -> None:
        class Boom(BaseApplet[State]):
            def build(self, ctx: Context) -> typing.Never:
                raise ValueError("boom")

        session = Session(Boom(), discard)
        session.handle(HELLO)

        with self.assertRaises(glimpse.RenderError):
            session.flush()

    def test_oversized_array_prop_is_a_limit_error(self) -> None:
        class Wide(BaseApplet[State]):
            def build(self, ctx: Context) -> list[W]:
                return [glimpse.DropDown(options=["x"] * 65)]

        session = Session(Wide(), discard)
        session.handle(HELLO)

        with self.assertRaises(glimpse.LimitError):
            session.flush()

    def test_fresh_subtree_over_children_cap_is_a_limit_error(self) -> None:
        class Wide(BaseApplet[State]):
            def build(self, ctx: Context) -> list[W]:
                return [
                    Section(title="x", children=[glimpse.Label(text=str(i)) for i in range(250)])
                ]

        session = Session(Wide(), discard)
        session.handle(HELLO)

        with self.assertRaises(glimpse.LimitError):
            session.flush()

    def test_unkeyed_type_swap_remounts_instead_of_moving(self) -> None:
        class Swap(BaseApplet[State]):
            def __init__(self) -> None:
                self.flipped = False

            def build(self, ctx: Context) -> list[W]:
                pair = [
                    glimpse.Label(text="one"),
                    glimpse.Button(text="two", on_click=lambda: None),
                ]
                return [Popover(title="t", children=pair[::-1] if self.flipped else pair)]

        app = Swap()
        lines: list[str] = []
        session = Session(app, lines.append)
        session.handle(HELLO)
        session.flush()
        app.flipped = True
        session.dirty = True
        session.flush()
        commits = [json.loads(line) for line in lines if '"commit"' in line]
        ops = [op for commit in commits[1:] for op in commit["ops"]]
        self.assertTrue(any(op["op"] == "remove" for op in ops))
        self.assertFalse(any(op["op"] == "move" for op in ops))

    def test_no_build_before_hello(self) -> None:
        lines: list[str] = []
        session = Session(App(), lines.append)
        session.handle({"t": "options", "options": {}})
        session.flush()
        self.assertEqual(lines, [])


class ClientSideHelpersTest(unittest.TestCase):
    def test_notify_argv_uses_entry_name_icon_and_hint(self) -> None:
        entry = AppletEntry(id="me.example.App.desktop", name="My App", icon="app-icon")
        self.assertEqual(
            notify_argv("slot-key", entry, "Hi", "Body", None, "low"),
            [
                "notify-send",
                "--app-name=My App",
                "--urgency=low",
                "--icon=app-icon",
                "--hint=string:desktop-entry:me.example.App",
                "--",
                "Hi",
                "Body",
            ],
        )
        self.assertIn(
            "--icon=explicit", notify_argv("slot-key", entry, "Hi", None, "explicit", None)
        )

    def test_notify_argv_without_entry_uses_name_and_no_hint(self) -> None:
        self.assertEqual(
            notify_argv("slot-key", None, "Hi", None, None, None),
            ["notify-send", "--app-name=slot-key", "--urgency=normal", "--", "Hi"],
        )

    def test_notify_argv_bogus_urgency_becomes_normal(self) -> None:
        self.assertIn("--urgency=normal", notify_argv("slot-key", None, "Hi", None, None, "bogus"))

    def test_notify_argv_summary_after_separator_and_empty_body_omitted(self) -> None:
        argv = notify_argv("slot-key", None, "-u critical", "", None, None)
        self.assertEqual(argv[-2:], ["--", "-u critical"])

    def test_notify_and_copy_send_no_socket_line(self) -> None:
        lines: list[str] = []
        session = Session(EmptyApp(), lines.append)
        session.handle(HELLO)
        lines.clear()

        with mock.patch("glimpse_sdk.context._spawn") as spawn:
            session.ctx.notify("Hi", body="b", icon="i", urgency="low")
            session.ctx.copy("x")

        self.assertEqual(lines, [])
        self.assertEqual(spawn.call_count, 2)
        self.assertEqual(spawn.call_args_list[0].args[0], "notify-send")
        self.assertEqual(
            spawn.call_args_list[0].args[1],
            notify_argv("test", None, "Hi", "b", "i", "low")[1:],
        )
        self.assertEqual(spawn.call_args_list[1].args, ("wl-copy", []))
        self.assertEqual(spawn.call_args_list[1].kwargs, {"stdin": "x"})

    def test_hello_without_entry_and_service_message_still_renders(self) -> None:
        lines: list[str] = []
        session = Session(App(), lines.append)
        session.handle(HELLO)
        session.handle({"t": "service", "name": "mpris", "value": {"players": []}})
        session.flush()
        self.assertIsNone(session.ctx.entry)
        self.assertIn('"commit"', "".join(lines))

    def test_hello_entry_is_exposed_and_malformed_entry_is_none(self) -> None:
        session = Session(EmptyApp(), discard)
        session.handle(
            {
                **HELLO,
                "entry": {"id": "me.example.App", "name": "My App", "icon": None},
            }
        )
        self.assertEqual(
            session.ctx.entry, AppletEntry(id="me.example.App", name="My App", icon=None)
        )
        malformed = Session(EmptyApp(), discard)
        malformed.handle({**HELLO, "entry": {"id": 3, "name": "My App", "icon": "i"}})
        self.assertIsNone(malformed.ctx.entry)

    def test_spawn_of_a_missing_program_does_not_raise(self) -> None:
        with contextlib.redirect_stderr(io.StringIO()):
            _spawn("glimpse-no-such-tool", [])

    def test_spawn_swallows_a_broken_pipe_on_stdin(self) -> None:
        proc = mock.Mock()
        handed_off = threading.Event()
        proc.communicate.side_effect = lambda data: handed_off.set()

        with (
            mock.patch("glimpse_sdk.context.subprocess.Popen", return_value=proc),
            contextlib.redirect_stderr(io.StringIO()),
        ):
            _spawn("wl-copy", [], stdin="x")

        self.assertTrue(handed_off.wait(timeout=5))
        proc.communicate.assert_called_once_with(b"x")

    def test_removed_surface_raises_attribute_error(self) -> None:
        session = Session(EmptyApp(), discard)

        for attr in ("mpris", "compositor", "services", "mpris_control"):
            with self.subTest(attr=attr), self.assertRaises(AttributeError):
                getattr(session.ctx, attr)


class ConcurrentSendTest(unittest.TestCase):
    def test_lines_from_many_threads_never_interleave(self) -> None:
        out: list[str] = []

        def slow_write(line: str) -> None:
            for ch in line:
                out.append(ch)
                time.sleep(0)
            out.append("\n")

        session = Session(EmptyApp(), slow_write)
        msgs = [{"t": "notify", "summary": chr(97 + n) * 200} for n in range(8)]

        def flood(msg: Node) -> None:
            for _ in range(50):
                session.send(msg)

        threads = [threading.Thread(target=flood, args=(m,)) for m in msgs]

        for t in threads:
            t.start()

        for t in threads:
            t.join()

        for line in "".join(out).splitlines():
            self.assertEqual(json.loads(line)["summary"], json.loads(line)["summary"][0] * 200)


class AsyncHandlerTest(unittest.IsolatedAsyncioTestCase):
    async def test_a_coroutine_returned_by_a_handler_is_spawned(self) -> None:
        done = asyncio.Event()

        async def on_change(value: str) -> None:
            done.set()

        class App(BaseApplet[State]):
            def build(self, ctx: Context) -> list[W]:
                return [Popover(title="t", children=[Entry(value="", on_change=on_change)])]

        woken: list[bool] = []
        async with asyncio.TaskGroup() as group:
            session = Session(App(), discard, on_wake=lambda: woken.append(True), tasks=group)
            session.handle(HELLO)
            session.flush()
            entry = next(n for n in session.nodes.values() if n.kind == "entry")
            session.handle({"t": "event", "id": entry.id, "name": "onChange", "args": ["x"]})
            await done.wait()

        self.assertEqual(woken, [True])

    def test_spawn_without_the_runtime_is_an_error(self) -> None:
        async def noop() -> None:
            pass

        session = Session(App(), discard)

        with self.assertRaises(RuntimeError):
            session.spawn(noop())


class RunLoopTest(unittest.TestCase):
    def serve(
        self,
        app: AppProtocol,
        script: collections.abc.Callable[[socket.socket], None],
        timeout: float = 5,
    ) -> tuple[int, list[Node]]:
        path = os.path.join(tempfile.mkdtemp(), "s.sock")
        server = socket.socket(socket.AF_UNIX)
        server.bind(path)
        server.listen(1)
        os.environ["GLIMPSE_APPLET_SOCKET"] = path
        received: list[Node] = []

        def host() -> None:
            conn, _ = server.accept()
            conn.settimeout(timeout)

            with contextlib.suppress(OSError):
                script(conn)
                conn.shutdown(socket.SHUT_WR)

            data = b""

            with contextlib.suppress(OSError):
                while chunk := conn.recv(65536):
                    data += chunk

            conn.close()
            received.extend(json.loads(line) for line in data.splitlines())

        thread = threading.Thread(target=host)
        thread.start()
        code = 0

        try:
            glimpse.run(app)
        except SystemExit as exit_:
            code = int(exit_.code or 0)
        finally:
            thread.join()
            del os.environ["GLIMPSE_APPLET_SOCKET"]
            server.close()

        return code, received

    def hello_then(self, tail: float = 0.3) -> collections.abc.Callable[[socket.socket], None]:
        def script(conn: socket.socket) -> None:
            conn.sendall(json.dumps(HELLO).encode() + b"\n")
            time.sleep(tail)

        return script

    def test_start_hook_runs_after_hello_and_rebuilds_on_completion(self) -> None:
        class App(BaseApplet[State]):
            def __init__(self) -> None:
                self.text = "idle"

            async def start(self, ctx: Context) -> None:
                await asyncio.sleep(0.05)
                self.text = "loaded"

            def build(self, ctx: Context) -> list[W]:
                return [Indicator(text=self.text)]

        code, lines = self.serve(App(), self.hello_then())
        texts = json.dumps(lines)
        self.assertEqual(code, 0)
        self.assertIn("idle", texts)
        self.assertIn("loaded", texts)

    def test_eof_still_flushes_the_pending_batch(self) -> None:
        def script(conn: socket.socket) -> None:
            conn.sendall(json.dumps(HELLO).encode() + b"\n")

        code, lines = self.serve(EmptyApp(), script)
        self.assertEqual(code, 0)
        self.assertEqual(lines[0], {"t": "hello", "v": 1})

    def test_a_garbage_line_is_skipped(self) -> None:
        class App(BaseApplet[State]):
            def build(self, ctx: Context) -> list[W]:
                return [Indicator(text="ok")]

        def script(conn: socket.socket) -> None:
            conn.sendall(b"not json\n" + json.dumps(HELLO).encode() + b"\n")
            time.sleep(0.2)

        code, lines = self.serve(App(), script)
        self.assertEqual(code, 0)
        self.assertIn("commit", [line["t"] for line in lines])

    def test_wake_from_another_thread_rebuilds(self) -> None:
        class App(BaseApplet[State]):
            def __init__(self) -> None:
                self.text = "before"
                self.started = False

            def build(self, ctx: Context) -> list[W]:
                if not self.started:
                    self.started = True

                    def later() -> None:
                        time.sleep(0.1)
                        self.text = "after"
                        ctx.wake()

                    threading.Thread(target=later).start()

                return [Indicator(text=self.text)]

        code, lines = self.serve(App(), self.hello_then(0.5))
        texts = json.dumps(lines)
        self.assertEqual(code, 0)
        self.assertIn("after", texts)

    def test_a_render_error_exits_70(self) -> None:
        class Boom(BaseApplet[State]):
            def build(self, ctx: Context) -> list[W]:
                raise ValueError("boom")

        code, _ = self.serve(Boom(), self.hello_then())
        self.assertEqual(code, 70)

    def test_a_limit_error_exits_70(self) -> None:
        class Wide(BaseApplet[State]):
            def build(self, ctx: Context) -> list[W]:
                return [Section(children=[glimpse.Label(text=str(i)) for i in range(250)])]

        code, _ = self.serve(Wide(), self.hello_then())
        self.assertEqual(code, 70)

    def test_a_spawned_task_that_raises_exits_1(self) -> None:
        class App(BaseApplet[State]):
            async def fail(self) -> None:
                raise ValueError("task failed")

            def build(self, ctx: Context) -> list[W]:
                ctx.spawn(self.fail())
                return []

        code, _ = self.serve(App(), self.hello_then(1))
        self.assertEqual(code, 1)

    def test_an_oversized_incoming_line_exits_70(self) -> None:
        def script(conn: socket.socket) -> None:
            conn.sendall(b"x" * (glimpse.MAX_LINE + 10))
            time.sleep(0.5)

        code, _ = self.serve(EmptyApp(), script)
        self.assertEqual(code, 70)

    def test_a_missing_socket_variable_exits_1(self) -> None:
        with mock.patch.dict(os.environ, clear=False):
            os.environ.pop("GLIMPSE_APPLET_SOCKET", None)

            with self.assertRaises(SystemExit) as raised:
                glimpse.run(EmptyApp())

        self.assertEqual(raised.exception.code, 1)

    def test_a_missing_socket_file_exits_0(self) -> None:
        missing = os.path.join(tempfile.mkdtemp(), "none.sock")

        with mock.patch.dict(os.environ, {"GLIMPSE_APPLET_SOCKET": missing}):
            glimpse.run(EmptyApp())


if __name__ == "__main__":
    unittest.main()
