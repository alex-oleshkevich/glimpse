from __future__ import annotations

import asyncio
import contextlib
import tempfile
import unittest
from pathlib import Path

from glimpse_sdk import Event, IpcError, Subscriber, ipc
from glimpse_sdk.ipc import (
    _escape,
    _parse_ack,
    _parse_event,
    _unescape,
    _validate_token,
)


class IpcCodecTests(unittest.TestCase):
    def test_escape_roundtrip(self) -> None:
        s = "a b\tc\nd\\e"
        self.assertEqual(_unescape(_escape(s)), s)

    def test_parse_event_unescapes_and_extracts_ts(self) -> None:
        ev = _parse_event("notification.received body=l1\\nl2\\sword ts=42")
        self.assertEqual(ev.name, "notification.received")
        self.assertEqual(ev.ts, 42)
        self.assertEqual(ev.fields["body"], "l1\nl2 word")

    def test_parse_ack_failure_raises(self) -> None:
        with self.assertRaises(IpcError):
            _parse_ack("ack ok=false error=nope")
        self.assertEqual(_parse_ack("ack ok=true echo=hi"), {"echo": "hi"})

    def test_validate_token_rejects_injection(self) -> None:
        _validate_token("action", "open_uri", forbid_eq=False)
        for bad in ("a\nsubscribe *", "a b", ""):
            with self.assertRaises(IpcError):
                _validate_token("action", bad, forbid_eq=False)
        with self.assertRaises(IpcError):
            _validate_token("param key", "k=v", forbid_eq=True)

    async def _dispatch_rejects_unsafe(self) -> None:
        sub = Subscriber(Path("/nonexistent/glimpse-x.sock"))
        with self.assertRaises(IpcError):
            await sub.dispatch("evil\naction", {"k": "v"})
        with self.assertRaises(IpcError):
            await sub.dispatch("ok", {"bad key": "v"})

    def test_dispatch_rejects_unsafe_action_before_connect(self) -> None:
        asyncio.run(self._dispatch_rejects_unsafe())

    def test_ipc_default_service_is_shell(self) -> None:
        import os

        with tempfile.TemporaryDirectory() as d:
            os.environ["GLIMPSE_IPC_DIR"] = d
            try:
                self.assertEqual(ipc()._socket, Path(d) / "ipc.sock")
                self.assertEqual(ipc("idle")._socket, Path(d) / "idle.sock")
            finally:
                del os.environ["GLIMPSE_IPC_DIR"]


class IpcServerTests(unittest.IsolatedAsyncioTestCase):
    async def test_dispatch_and_listen_against_fake_server(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            socket = Path(d) / "ipc.sock"

            async def handle(
                reader: asyncio.StreamReader, writer: asyncio.StreamWriter
            ) -> None:
                writer.write(b"hello version=test\n")
                await writer.drain()
                first = (await reader.readline()).decode().strip()
                if first.startswith("subscribe "):
                    self.assertEqual(first, "subscribe audio.*")
                    writer.write(b"audio.volume_changed volume=42 ts=7\n")
                else:
                    self.assertEqual(first, "open_uri uri=https://example.com")
                    writer.write(b"ack ok=true echo=done\n")
                await writer.drain()
                writer.close()
                with contextlib.suppress(Exception):
                    await writer.wait_closed()

            server = await asyncio.start_unix_server(handle, path=str(socket))
            async with server:
                sub = Subscriber(socket)

                ack = await sub.dispatch(
                    "open_uri", {"uri": "https://example.com"}
                )
                self.assertEqual(ack, {"echo": "done"})

                events: list[Event] = []
                async with contextlib.aclosing(sub.listen("audio.*")) as stream:
                    async for ev in stream:
                        events.append(ev)
                        break

            self.assertEqual(events[0].name, "audio.volume_changed")
            self.assertEqual(events[0].ts, 7)
            self.assertEqual(events[0].fields["volume"], "42")

    async def test_connect_failure_raises_ipc_error(self) -> None:
        sub = Subscriber(Path("/nonexistent/glimpse-missing.sock"))
        with self.assertRaises(IpcError):
            await sub.dispatch("noop")


if __name__ == "__main__":
    unittest.main()
