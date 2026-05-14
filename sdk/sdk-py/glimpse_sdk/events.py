from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Generic, TypeVar

OptionsT = TypeVar("OptionsT", default=dict[str, Any])


@dataclass(slots=True)
class InitEvent(Generic[OptionsT]):
    instance: str
    options: OptionsT


@dataclass(slots=True)
class CallbackEvent:
    id: str
    event: str


@dataclass(slots=True)
class ClickEvent(CallbackEvent):
    button: str | None = None


@dataclass(slots=True)
class ScrollEvent(CallbackEvent):
    delta_y: float | None = None


@dataclass(slots=True)
class InputEvent(CallbackEvent):
    text: str = ""


@dataclass(slots=True)
class ChangeEvent(CallbackEvent):
    value: Any = None


@dataclass(slots=True)
class ToggleEvent(CallbackEvent):
    value: bool = False


@dataclass(slots=True)
class PopoverEvent(CallbackEvent):
    open: bool = False


def parse_init_event(payload: dict[str, Any]) -> InitEvent[dict[str, Any]]:
    return InitEvent(
        instance=str(payload.get("instance", "")),
        options=payload.get("options") or {},
    )


def parse_callback_event(payload: dict[str, Any]) -> CallbackEvent:
    event_type = str(payload.get("type", payload.get("event", "")))
    callback_id = str(payload.get("id", ""))
    if event_type == "click":
        return ClickEvent(id=callback_id, event=event_type, button=payload.get("button"))
    if event_type == "scroll":
        return ScrollEvent(id=callback_id, event=event_type, delta_y=payload.get("delta_y"))
    if event_type == "input":
        return InputEvent(id=callback_id, event=event_type, text=str(payload.get("text", "")))
    if event_type == "toggle":
        active = payload.get("active")
        value = payload.get("value")
        if isinstance(active, bool):
            toggled = active
        elif isinstance(value, bool):
            toggled = value
        else:
            toggled = False
        return ToggleEvent(id=callback_id, event=event_type, value=toggled)
    if event_type in {"open", "close"}:
        return PopoverEvent(id=callback_id, event=event_type, open=event_type == "open")
    return ChangeEvent(id=callback_id, event=event_type, value=payload.get("value"))
