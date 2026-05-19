from __future__ import annotations

import asyncio
import contextlib
import sys
import unittest
from dataclasses import dataclass
from unittest.mock import patch

import glimpse_sdk
from glimpse_sdk import (
    Applet,
    AppletState,
    ActionItem,
    Badge,
    Button,
    ChangeEvent,
    Checkbox,
    Column,
    Hero,
    InitEvent,
    Meter,
    PopoverEvent,
    PopoverScaffold,
    Row,
    Select,
    Spinner,
    StatusDot,
    StatusItem,
    Slider,
    Switch,
    Text,
    ToggleButton,
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
        return Column(children=[Text(text=state.version), Button(id="submit", label="Submit")])

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
        row = Row().to_protocol()
        column = Column().to_protocol()
        self.assertEqual(row["type"], "row")
        self.assertEqual(row["data"]["spacing"], 4)
        self.assertEqual(column["type"], "column")
        self.assertEqual(column["data"]["spacing"], 4)

    def test_section_is_not_public_sdk_widget(self) -> None:
        self.assertFalse(hasattr(glimpse_sdk, "Section"))

    def test_status_dot_serializes_as_status_protocol_type(self) -> None:
        self.assertEqual(StatusDot().to_protocol()["type"], "status")

    def test_spinner_serializes_with_default_spinning(self) -> None:
        payload = Spinner().to_protocol()
        self.assertEqual(payload["type"], "spinner")
        self.assertEqual(payload["data"]["spinning"], True)

    def test_variant_serializes_as_semantic_protocol_value(self) -> None:
        payload = Badge(label="Warning", variant=Variant.WARNING).to_protocol()
        self.assertEqual(payload["data"]["variant"], "warning")

    async def test_desktop_helpers_run_local_commands(self) -> None:
        calls: list[tuple[str, list[str], str | None]] = []

        async def fake_run(command: str, args: list[str], stdin: str | None = None) -> None:
            calls.append((command, args, stdin))

        applet = DemoApplet()
        with patch("glimpse_sdk.app._run_desktop_command", fake_run):
            await applet.copy_to_clipboard("hello")
            await applet.open_uri("https://example.com")
            await applet.show_notification("Build complete", "Tests passed")

        self.assertEqual(
            calls,
            [
                ("wl-copy", [], "hello"),
                ("xdg-open", ["https://example.com"], None),
                ("notify-send", ["Build complete", "Tests passed"], None),
            ],
        )

    async def test_run_command_returns_stdout_stderr_and_rc(self) -> None:
        applet = DemoApplet()

        result = await applet.run_command(
            [
                sys.executable,
                "-c",
                "import sys; print('out'); print('err', file=sys.stderr); raise SystemExit(7)",
            ]
        )

        self.assertEqual(result.stdout, "out\n")
        self.assertEqual(result.stderr, "err\n")
        self.assertEqual(result.rc, 7)

    async def test_run_command_rejects_empty_command(self) -> None:
        applet = DemoApplet()

        with self.assertRaises(ValueError):
            await applet.run_command([])

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

    async def test_button_on_click_registers_generated_handler(self) -> None:
        hits: list[str] = []

        @dataclass
        class S(AppletState):
            seen: bool = False

        class InlineHandlerApplet(Applet[S]):
            def initial_state(self) -> S:
                return S()

            async def popover(self, state: S):
                return Column(children=[Button(label="Pick", on_click=self.pick)])

            def pick(self, state: S, event) -> None:
                state.seen = True
                hits.append(event.id)

        applet = InlineHandlerApplet()
        applet._render_requested = True
        await applet._flush_render()
        await applet._outgoing.get()
        _, tree = await applet._outgoing.get()

        button_id = tree["root"]["data"]["children"][0]["data"]["id"]
        self.assertTrue(button_id.startswith("__glimpse:click:"))

        ev = parse_callback_event({"id": button_id, "type": "click", "source": "popover"})
        await applet._dispatch_callback(ev)

        self.assertEqual(hits, [button_id])
        self.assertTrue(applet.state.seen)

    async def test_interactive_widgets_register_inline_handlers(self) -> None:
        hits: list[tuple[str, str, object]] = []

        @dataclass
        class S(AppletState):
            seen: int = 0

        class InlineHandlerApplet(Applet[S]):
            def initial_state(self) -> S:
                return S()

            async def popover(self, state: S):
                return PopoverScaffold(
                    hero=Hero(title="VPN", id="hero-toggle", switch=True, on_toggle=self.handle),
                    body=Column(
                        children=[
                            ActionItem(label="Open", on_click=self.handle),
                            Switch(label="VPN", on_toggle=self.handle),
                            ToggleButton(label="Wi-Fi", on_toggle=self.handle),
                            Checkbox(label="Run at login", on_toggle=self.handle),
                            Slider(on_change=self.handle),
                            Select(
                                items=[{"id": "prod", "label": "Production"}],
                                on_change=self.handle,
                            ),
                            Meter(label="Volume", on_change=self.handle),
                        ]
                    ),
                )

            async def handle(self, state: S, event) -> None:
                state.seen += 1
                value = getattr(event, "value", None)
                hits.append((event.event, event.id, value))

        applet = InlineHandlerApplet()
        applet._render_requested = True
        await applet._flush_render()
        await applet._outgoing.get()
        _, tree = await applet._outgoing.get()

        children = tree["root"]["data"]["body"]["data"]["children"]
        generated_ids = [
            children[0]["data"]["id"],
            children[1]["data"]["id"],
            children[2]["data"]["id"],
            children[3]["data"]["id"],
            children[4]["data"]["id"],
            children[5]["data"]["id"],
            children[6]["data"]["id"],
        ]
        self.assertEqual(tree["root"]["data"]["hero"]["data"]["id"], "hero-toggle")
        self.assertTrue(generated_ids[0].startswith("__glimpse:click:"))
        for target_id in generated_ids[1:4]:
            self.assertTrue(target_id.startswith("__glimpse:toggle:"))
        for target_id in generated_ids[4:]:
            self.assertTrue(target_id.startswith("__glimpse:change:"))

        events = [
            {"id": "hero-toggle", "type": "toggle", "source": "popover", "active": True},
            {"id": generated_ids[0], "type": "click", "source": "popover"},
            {"id": generated_ids[1], "type": "toggle", "source": "popover", "active": False},
            {"id": generated_ids[2], "type": "toggle", "source": "popover", "active": True},
            {"id": generated_ids[3], "type": "toggle", "source": "popover", "active": True},
            {"id": generated_ids[4], "type": "change", "source": "popover", "value": 0.72},
            {
                "id": generated_ids[5],
                "type": "change",
                "source": "popover",
                "value": {"id": "prod", "label": "Production", "index": 0},
            },
            {"id": generated_ids[6], "type": "change", "source": "popover", "value": 0.4},
        ]
        for event in events:
            await applet._dispatch_callback(parse_callback_event(event))

        self.assertEqual(applet.state.seen, 8)
        self.assertEqual(
            hits,
            [
                ("toggle", "hero-toggle", True),
                ("click", generated_ids[0], None),
                ("toggle", generated_ids[1], False),
                ("toggle", generated_ids[2], True),
                ("toggle", generated_ids[3], True),
                ("change", generated_ids[4], 0.72),
                ("change", generated_ids[5], {"id": "prod", "label": "Production", "index": 0}),
                ("change", generated_ids[6], 0.4),
            ],
        )


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
