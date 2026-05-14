from __future__ import annotations

import asyncio
import contextlib
import unittest
from dataclasses import dataclass

from glimpse_sdk import (
    Applet,
    AppletState,
    Box,
    Button,
    ChangeEvent,
    Column,
    Icon,
    InitEvent,
    Label,
    PopoverEvent,
    Row,
    Select,
    SelectOption,
    Spinner,
    StatusDot,
    StatusItem,
    Variant,
    click,
)
from glimpse_sdk.events import parse_callback_event


@dataclass
class DemoState(AppletState):
    version: str = "v1"
    clicks: int = 0


class DemoApplet(Applet[DemoState]):
    def initial_state(self) -> DemoState:
        return DemoState()

    async def status(self, state: DemoState):
        return [StatusItem(id="demo", icon=Icon.name("demo-symbolic"), label=state.version)]

    async def popover(self, state: DemoState):
        return Box.vertical([Label(text=state.version), Button(id="submit", label="Submit")])

    @click("submit")
    async def handle_submit(self, _event) -> None:
        await self.set_state(clicks=self.state.clicks + 1, version="v2")


class InitApplet(DemoApplet):
    async def on_init(self, event: InitEvent) -> None:
        self.state.version = event.instance


class GlimpseAppletTests(unittest.IsolatedAsyncioTestCase):
    async def test_set_state_updates_dataclass_fields(self) -> None:
        applet = DemoApplet()
        await applet.set_state(version="v2")
        self.assertEqual(applet.state.version, "v2")

    async def test_render_flush_emits_protocol_messages(self) -> None:
        applet = DemoApplet()
        await applet.set_state(version="v2")
        await applet._flush_render()
        status = await applet._outgoing.get()
        tree = await applet._outgoing.get()
        self.assertEqual(status[0], "status")
        self.assertEqual(status[1]["items"][0]["label"], "v2")
        self.assertEqual(tree[0], "popover")
        self.assertIn("root", tree[1])

    def test_parse_callback_event_returns_typed_variant(self) -> None:
        event = parse_callback_event({"id": "submit", "type": "click", "button": "left"})
        self.assertEqual(event.event, "click")
        self.assertEqual(getattr(event, "button"), "left")

    def test_parse_callback_event_returns_typed_popover_variant(self) -> None:
        event = parse_callback_event({"id": "popover", "type": "open", "source": "popover"})
        self.assertIsInstance(event, PopoverEvent)
        self.assertTrue(getattr(event, "open"))

    def test_select_serializes_items(self) -> None:
        node = Select(id="env", items=[SelectOption(id="prod", label="Production")], selected=0)
        payload = node.to_protocol()
        self.assertEqual(payload["type"], "select")
        self.assertEqual(payload["data"]["items"][0]["id"], "prod")

    def test_row_and_column_serialize_as_layout_protocol_types(self) -> None:
        self.assertEqual(Row().to_protocol()["type"], "row")
        self.assertEqual(Column().to_protocol()["type"], "column")

    def test_status_dot_serializes_as_status_protocol_type(self) -> None:
        self.assertEqual(StatusDot().to_protocol()["type"], "status")

    def test_spinner_serializes_with_default_spinning(self) -> None:
        payload = Spinner().to_protocol()
        self.assertEqual(payload["type"], "spinner")
        self.assertEqual(payload["data"]["spinning"], True)

    def test_variant_serializes_as_semantic_protocol_value(self) -> None:
        payload = Label(text="Warning", variant=Variant.WARNING).to_protocol()
        self.assertEqual(payload["data"]["variant"], "warning")

    async def test_init_event_rerenders_changed_state(self) -> None:
        applet = InitApplet()
        eof = asyncio.Event()
        loop_task = asyncio.create_task(applet._event_loop(eof))
        try:
            status = await applet._outgoing.get()
            await applet._outgoing.get()
            self.assertEqual(status[1]["items"][0]["label"], "v1")

            await applet._incoming.put(InitEvent(instance="v3", options={}))
            status = await applet._outgoing.get()
            self.assertEqual(status[1]["items"][0]["label"], "v3")
        finally:
            loop_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await loop_task

    async def test_popover_updates_are_emitted_when_state_changes(self) -> None:
        applet = DemoApplet()
        applet._render_requested = True
        await applet._flush_render()
        await applet._outgoing.get()
        await applet._outgoing.get()

        await applet.set_state(version="v2")
        status = await applet._outgoing.get()
        self.assertEqual(status[0], "status")
        command, payload = await applet._outgoing.get()
        self.assertEqual(command, "popover")
        self.assertEqual(payload["root"]["data"]["children"][0]["data"]["text"], "v2")


if __name__ == "__main__":
    unittest.main()
