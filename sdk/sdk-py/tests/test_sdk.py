from __future__ import annotations

import asyncio
import contextlib
import unittest
from dataclasses import dataclass

from glimpse_sdk import (
    Applet,
    AppletState,
    Button,
    ChangeEvent,
    Column,
    InitEvent,
    Label,
    PopoverEvent,
    Row,
    Select,
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
        return [StatusItem(id="demo", icon="demo-symbolic", label=state.version)]

    async def popover(self, state: DemoState):
        return Column(children=[Label(text=state.version), Button(id="submit", label="Submit")])

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
        node = Select(id="env", items=[{"id": "prod", "label": "Production"}], selected=0)
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


    async def test_wildcard_handler_matches_by_pattern(self) -> None:
        hits: list[str] = []

        @dataclass
        class S(AppletState):
            pass

        class WildApplet(Applet[S]):
            def initial_state(self) -> S:
                return S()

            @click("item_*")
            async def on_item(self, event) -> None:
                hits.append(event.id)

        applet = WildApplet()
        ev = parse_callback_event({"id": "item_42", "type": "click", "source": "popover"})
        await applet._dispatch_callback(ev)
        self.assertEqual(hits, ["item_42"])

    async def test_exact_handler_takes_priority_over_pattern(self) -> None:
        hits: list[str] = []

        @dataclass
        class S(AppletState):
            pass

        class PriorityApplet(Applet[S]):
            def initial_state(self) -> S:
                return S()

            @click("item_*")
            async def on_item_any(self, event) -> None:
                hits.append(f"pattern:{event.id}")

            @click("item_special")
            async def on_item_special(self, event) -> None:
                hits.append(f"exact:{event.id}")

        applet = PriorityApplet()
        ev_special = parse_callback_event({"id": "item_special", "type": "click", "source": "popover"})
        ev_other = parse_callback_event({"id": "item_42", "type": "click", "source": "popover"})
        await applet._dispatch_callback(ev_special)
        await applet._dispatch_callback(ev_other)
        self.assertEqual(hits, ["exact:item_special", "pattern:item_42"])

    async def test_unmatched_event_falls_through_to_on_callback(self) -> None:
        hits: list[str] = []

        @dataclass
        class S(AppletState):
            pass

        class FallbackApplet(Applet[S]):
            def initial_state(self) -> S:
                return S()

            async def on_callback(self, event) -> None:
                hits.append(event.id)

        applet = FallbackApplet()
        ev = parse_callback_event({"id": "unknown", "type": "click", "source": "popover"})
        await applet._dispatch_callback(ev)
        self.assertEqual(hits, ["unknown"])


if __name__ == "__main__":
    unittest.main()
