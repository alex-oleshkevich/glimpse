from __future__ import annotations

from dataclasses import dataclass, field
from enum import StrEnum
from typing import Any, Awaitable, Callable, TypeAlias

InlineHandler: TypeAlias = Callable[[Any, Any], Awaitable[None] | None]
HandlerRegistry: TypeAlias = Any


class Space(StrEnum):
    S1 = "s1"
    S2 = "s2"
    S3 = "s3"
    S4 = "s4"
    S5 = "s5"
    S6 = "s6"
    S7 = "s7"
    S8 = "s8"
    S9 = "s9"
    S10 = "s10"


class Radius(StrEnum):
    NONE = "none"
    SM = "sm"
    MD = "md"
    LG = "lg"
    PILL = "pill"


class ContainerBg(StrEnum):
    NONE = "none"
    SURFACE = "surface"
    RAISED = "raised"


class FontSize(StrEnum):
    XS = "xs"
    SM = "sm"
    BASE = "base"
    LG = "lg"
    XL = "xl"


class FontWeight(StrEnum):
    NORMAL = "normal"
    MEDIUM = "medium"
    SEMIBOLD = "semibold"
    BOLD = "bold"


class TextColor(StrEnum):
    NORMAL = "normal"
    MUTED = "muted"
    ACCENT = "accent"
    SUCCESS = "success"
    WARNING = "warning"
    ERROR = "error"


class BadgeKind(StrEnum):
    DEFAULT = "default"
    SUCCESS = "success"
    WARNING = "warning"
    ERROR = "error"
    ACCENT = "accent"


class StatusDotStatus(StrEnum):
    NEUTRAL = "neutral"
    SUCCESS = "success"
    WARNING = "warning"
    ERROR = "error"
    ACCENT = "accent"


class PagerAppearance(StrEnum):
    DOTS = "dots"
    NUMBERS = "numbers"


class PopoverSize(StrEnum):
    SMALL = "small"
    MEDIUM = "medium"
    LARGE = "large"
    WIDE = "wide"


@dataclass
class Widget:
    widget_type: str = field(init=False, default="")
    visible: bool | None = None
    tooltip: str | None = None
    css_classes: list[str] = field(default_factory=list)
    styles: dict[str, str] = field(default_factory=dict)

    def data(self) -> dict[str, Any]:
        payload: dict[str, Any] = {}
        if self.visible is not None:
            payload["visible"] = self.visible
        if self.tooltip is not None:
            payload["tooltip"] = self.tooltip
        if self.css_classes:
            payload["css_classes"] = self.css_classes
        if self.styles:
            payload["styles"] = self.styles
        return payload

    def to_protocol(self) -> dict[str, Any]:
        return {"type": self.widget_type, "data": self.data()}

    def bind_handlers(self, registry: HandlerRegistry, path: tuple[int, ...]) -> None:
        return None


def _node(value: TreeNode | None) -> dict[str, Any] | None:
    return None if value is None else value.to_protocol()


def _children(values: list[TreeNode]) -> list[dict[str, Any]]:
    return [child.to_protocol() for child in values]


def _target_id(registry: HandlerRegistry, event: str, widget_id: str | None, path: tuple[int, ...]) -> str:
    return widget_id if widget_id else registry.generated_id(event, path)


@dataclass
class Text(Widget):
    text: str = ""
    size: FontSize | None = None
    weight: FontWeight | None = None
    color: TextColor | None = None
    xalign: float | None = None
    wrap: bool | None = None
    widget_type: str = field(init=False, default="text")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload["text"] = self.text
        if self.size is not None:
            payload["size"] = self.size.value
        if self.weight is not None:
            payload["weight"] = self.weight.value
        if self.color is not None:
            payload["color"] = self.color.value
        if self.xalign is not None:
            payload["xalign"] = self.xalign
        if self.wrap is not None:
            payload["wrap"] = self.wrap
        return payload


@dataclass
class Header(Widget):
    label: str = ""
    widget_type: str = field(init=False, default="header")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload["label"] = self.label
        return payload


@dataclass
class Hero(Widget):
    title: str = ""
    subtitle: str = ""
    id: str | None = None
    icon: str | None = None
    icon_size: int | None = None
    toggle: bool | None = None
    toggle_sensitive: bool | None = None
    separator: bool | None = None
    trailing: TreeNode | None = None
    on_toggle: InlineHandler | None = None
    widget_type: str = field(init=False, default="hero")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        if self.id is not None:
            payload["id"] = self.id
        payload["title"] = self.title
        payload["subtitle"] = self.subtitle
        if self.icon is not None:
            payload["icon"] = self.icon
        if self.icon_size is not None:
            payload["icon_size"] = self.icon_size
        if self.toggle is not None:
            payload["toggle"] = self.toggle
        if self.toggle_sensitive is not None:
            payload["toggle_sensitive"] = self.toggle_sensitive
        if self.separator is not None:
            payload["separator"] = self.separator
        if self.trailing is not None:
            payload["trailing"] = self.trailing.to_protocol()
        return payload

    def bind_handlers(self, registry: HandlerRegistry, path: tuple[int, ...]) -> None:
        if self.id and self.on_toggle:
            registry.add("toggle", self.id, self.on_toggle)
        if self.trailing:
            self.trailing.bind_handlers(registry, (*path, 0))


@dataclass
class Badge(Widget):
    label: str = ""
    kind: BadgeKind = BadgeKind.DEFAULT
    widget_type: str = field(init=False, default="badge")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload["label"] = self.label
        payload["kind"] = self.kind.value
        return payload


@dataclass
class StatusDot(Widget):
    status: StatusDotStatus = StatusDotStatus.NEUTRAL
    widget_type: str = field(init=False, default="status_dot")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload["status"] = self.status.value
        return payload


@dataclass
class PanelIndicator(Widget):
    id: str | None = None
    icon: str | None = None
    label: str | None = None
    active: bool = False
    checked: bool = False
    needs_attention: bool = False
    extra: TreeNode | None = None
    on_click: InlineHandler | None = None
    widget_type: str = field(init=False, default="panel_indicator")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        if self.id is not None:
            payload["id"] = self.id
        if self.icon is not None:
            payload["icon"] = self.icon
        if self.label is not None:
            payload["label"] = self.label
        payload["active"] = self.active
        payload["checked"] = self.checked
        payload["needs_attention"] = self.needs_attention
        if self.extra is not None:
            payload["extra"] = self.extra.to_protocol()
        return payload

    def bind_handlers(self, registry: HandlerRegistry, path: tuple[int, ...]) -> None:
        if self.id and self.on_click:
            registry.add("click", self.id, self.on_click)
        if self.extra:
            self.extra.bind_handlers(registry, (*path, 0))


@dataclass
class EmptyState(Widget):
    title: str = ""
    subtitle: str | None = None
    widget_type: str = field(init=False, default="empty_state")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload["title"] = self.title
        if self.subtitle is not None:
            payload["subtitle"] = self.subtitle
        return payload


@dataclass
class Spinner(Widget):
    spinning: bool = True
    widget_type: str = field(init=False, default="spinner")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload["spinning"] = self.spinning
        return payload


@dataclass
class Meter(Widget):
    label: str = ""
    value: float = 0.0
    min: float = 0.0
    max: float = 1.0
    step: float = 0.01
    id: str | None = None
    icon: str | None = None
    text: str | None = None
    interactive: bool = False
    on_change: InlineHandler | None = None
    widget_type: str = field(init=False, default="meter")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        if self.id is not None:
            payload["id"] = self.id
        if self.icon is not None:
            payload["icon"] = self.icon
        payload["label"] = self.label
        payload["value"] = self.value
        payload["min"] = self.min
        payload["max"] = self.max
        payload["step"] = self.step
        if self.text is not None:
            payload["text"] = self.text
        payload["interactive"] = self.interactive or self.on_change is not None
        return payload

    def bind_handlers(self, registry: HandlerRegistry, path: tuple[int, ...]) -> None:
        if self.on_change:
            self.id = _target_id(registry, "change", self.id, path)
            registry.add("change", self.id, self.on_change)


@dataclass
class Separator(Widget):
    widget_type: str = field(init=False, default="separator")


@dataclass
class Scroll(Widget):
    child: TreeNode | None = None
    widget_type: str = field(init=False, default="scroll")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        if self.child is None:
            raise ValueError("Scroll requires a child")
        payload["child"] = self.child.to_protocol()
        return payload

    def bind_handlers(self, registry: HandlerRegistry, path: tuple[int, ...]) -> None:
        if self.child:
            self.child.bind_handlers(registry, (*path, 0))


@dataclass
class ChildrenWidget(Widget):
    children: list[TreeNode] = field(default_factory=list)

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload["children"] = _children(self.children)
        return payload

    def bind_handlers(self, registry: HandlerRegistry, path: tuple[int, ...]) -> None:
        for index, child in enumerate(self.children):
            child.bind_handlers(registry, (*path, index))


@dataclass
class Row(ChildrenWidget):
    widget_type: str = field(init=False, default="row")


@dataclass
class Column(ChildrenWidget):
    widget_type: str = field(init=False, default="column")


@dataclass
class BoxedList(ChildrenWidget):
    widget_type: str = field(init=False, default="boxed_list")


@dataclass
class ButtonRow(ChildrenWidget):
    widget_type: str = field(init=False, default="button_row")


@dataclass
class Container(ChildrenWidget):
    padding: Space | None = None
    padding_x: Space | None = None
    padding_y: Space | None = None
    margin: Space | None = None
    margin_x: Space | None = None
    margin_y: Space | None = None
    radius: Radius = Radius.NONE
    bg: ContainerBg = ContainerBg.NONE
    border_width: int = 0
    min_width: Space | None = None
    min_height: Space | None = None
    widget_type: str = field(init=False, default="container")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        for name in ("padding", "padding_x", "padding_y", "margin", "margin_x", "margin_y", "min_width", "min_height"):
            value = getattr(self, name)
            if value is not None:
                payload[name] = value.value
        payload["radius"] = self.radius.value
        payload["bg"] = self.bg.value
        payload["border_width"] = self.border_width
        return payload


@dataclass
class PopoverShell(ChildrenWidget):
    size: PopoverSize = PopoverSize.MEDIUM
    footer: list[TreeNode] = field(default_factory=list)
    footer_visible: bool = False
    widget_type: str = field(init=False, default="popover_shell")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload["size"] = self.size.value
        if self.footer:
            payload["footer"] = _children(self.footer)
        if self.footer_visible:
            payload["footer_visible"] = self.footer_visible
        return payload

    def bind_handlers(self, registry: HandlerRegistry, path: tuple[int, ...]) -> None:
        super().bind_handlers(registry, path)
        for index, child in enumerate(self.footer):
            child.bind_handlers(registry, (*path, len(self.children) + index))


@dataclass
class Tile(Widget):
    primary: str = ""
    id: str | None = None
    secondary: str | None = None
    left_icon: str | None = None
    left: TreeNode | None = None
    right: TreeNode | None = None
    activatable: bool = False
    on_click: InlineHandler | None = None
    widget_type: str = field(init=False, default="tile")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        if self.id is not None:
            payload["id"] = self.id
        payload["primary"] = self.primary
        if self.secondary is not None:
            payload["secondary"] = self.secondary
        if self.left_icon is not None:
            payload["left_icon"] = self.left_icon
        if self.left is not None:
            payload["left"] = self.left.to_protocol()
        if self.right is not None:
            payload["right"] = self.right.to_protocol()
        payload["activatable"] = self.activatable
        return payload

    def bind_handlers(self, registry: HandlerRegistry, path: tuple[int, ...]) -> None:
        if self.on_click:
            self.id = _target_id(registry, "click", self.id, path)
            registry.add("click", self.id, self.on_click)
        for index, child in enumerate(c for c in (self.left, self.right) if c is not None):
            child.bind_handlers(registry, (*path, index))


@dataclass
class SegmentedTile(Tile):
    child: TreeNode | None = None
    expanded: bool = False
    on_toggle: InlineHandler | None = None
    widget_type: str = field(init=False, default="segmented_tile")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        if self.child is not None:
            payload["child"] = self.child.to_protocol()
        payload["expanded"] = self.expanded
        return payload

    def bind_handlers(self, registry: HandlerRegistry, path: tuple[int, ...]) -> None:
        super().bind_handlers(registry, path)
        if self.on_toggle:
            self.id = _target_id(registry, "toggle", self.id, path)
            registry.add("toggle", self.id, self.on_toggle)
        if self.child:
            self.child.bind_handlers(registry, (*path, 2))


@dataclass
class SwitchTile(Widget):
    id: str = ""
    primary: str = ""
    secondary: str | None = None
    left_icon: str | None = None
    left: TreeNode | None = None
    active: bool = False
    on_toggle: InlineHandler | None = None
    widget_type: str = field(init=False, default="switch_tile")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload["id"] = self.id
        payload["primary"] = self.primary
        if self.secondary is not None:
            payload["secondary"] = self.secondary
        if self.left_icon is not None:
            payload["left_icon"] = self.left_icon
        if self.left is not None:
            payload["left"] = self.left.to_protocol()
        payload["active"] = self.active
        return payload

    def bind_handlers(self, registry: HandlerRegistry, path: tuple[int, ...]) -> None:
        if self.on_toggle:
            registry.add("toggle", self.id, self.on_toggle)
        if self.left:
            self.left.bind_handlers(registry, (*path, 0))


@dataclass
class ExpanderTile(Widget):
    primary: str = ""
    id: str | None = None
    secondary: str | None = None
    left_icon: str | None = None
    left: TreeNode | None = None
    child: TreeNode | None = None
    expanded: bool = False
    on_toggle: InlineHandler | None = None
    widget_type: str = field(init=False, default="expander_tile")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        if self.id is not None:
            payload["id"] = self.id
        payload["primary"] = self.primary
        if self.secondary is not None:
            payload["secondary"] = self.secondary
        if self.left_icon is not None:
            payload["left_icon"] = self.left_icon
        if self.left is not None:
            payload["left"] = self.left.to_protocol()
        if self.child is not None:
            payload["child"] = self.child.to_protocol()
        payload["expanded"] = self.expanded
        return payload

    def bind_handlers(self, registry: HandlerRegistry, path: tuple[int, ...]) -> None:
        if self.on_toggle:
            self.id = _target_id(registry, "toggle", self.id, path)
            registry.add("toggle", self.id, self.on_toggle)
        for index, child in enumerate(c for c in (self.left, self.child) if c is not None):
            child.bind_handlers(registry, (*path, index))


@dataclass
class SliderTile(Widget):
    id: str = ""
    label: str | None = None
    left_icon: str | None = None
    left: TreeNode | None = None
    value: float = 0.0
    min: float = 0.0
    max: float = 1.0
    step: float = 0.01
    page: float = 0.1
    digits: int = 0
    snap_step: float | None = None
    on_change: InlineHandler | None = None
    widget_type: str = field(init=False, default="slider_tile")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload.update({"id": self.id, "value": self.value, "min": self.min, "max": self.max, "step": self.step, "page": self.page, "digits": self.digits})
        if self.label is not None:
            payload["label"] = self.label
        if self.left_icon is not None:
            payload["left_icon"] = self.left_icon
        if self.left is not None:
            payload["left"] = self.left.to_protocol()
        if self.snap_step is not None:
            payload["snap_step"] = self.snap_step
        return payload

    def bind_handlers(self, registry: HandlerRegistry, path: tuple[int, ...]) -> None:
        if self.on_change:
            registry.add("change", self.id, self.on_change)
        if self.left:
            self.left.bind_handlers(registry, (*path, 0))


@dataclass
class ChoiceTile(Widget):
    primary: str = ""
    id: str | None = None
    secondary: str | None = None
    left_icon: str | None = None
    left: TreeNode | None = None
    selected: bool = False
    on_click: InlineHandler | None = None
    widget_type: str = field(init=False, default="choice_tile")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        if self.id is not None:
            payload["id"] = self.id
        payload["primary"] = self.primary
        if self.secondary is not None:
            payload["secondary"] = self.secondary
        if self.left_icon is not None:
            payload["left_icon"] = self.left_icon
        if self.left is not None:
            payload["left"] = self.left.to_protocol()
        payload["selected"] = self.selected
        return payload

    def bind_handlers(self, registry: HandlerRegistry, path: tuple[int, ...]) -> None:
        if self.on_click:
            self.id = _target_id(registry, "click", self.id, path)
            registry.add("click", self.id, self.on_click)
        if self.left:
            self.left.bind_handlers(registry, (*path, 0))


@dataclass
class Choice:
    id: str
    primary: str
    secondary: str | None = None
    icon: str | None = None

    def to_protocol(self) -> dict[str, Any]:
        payload = {"id": self.id, "primary": self.primary}
        if self.secondary is not None:
            payload["secondary"] = self.secondary
        if self.icon is not None:
            payload["icon"] = self.icon
        return payload


@dataclass
class ChoiceList(Widget):
    id: str = ""
    choices: list[Choice] = field(default_factory=list)
    active: str | None = None
    on_change: InlineHandler | None = None
    widget_type: str = field(init=False, default="choice_list")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload["id"] = self.id
        if self.active is not None:
            payload["active"] = self.active
        payload["choices"] = [choice.to_protocol() for choice in self.choices]
        return payload

    def bind_handlers(self, registry: HandlerRegistry, path: tuple[int, ...]) -> None:
        if self.on_change:
            registry.add("change", self.id, self.on_change)


@dataclass
class KeyValueGrid(Widget):
    rows: list[tuple[str, str]] = field(default_factory=list)
    widget_type: str = field(init=False, default="key_value_grid")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload["rows"] = [{"key": key, "value": value} for key, value in self.rows]
        return payload


@dataclass
class PagerItem(Widget):
    id: int = 0
    label: str = ""
    appearance: PagerAppearance = PagerAppearance.DOTS
    active: bool = False
    inactive: bool = False
    occupied: bool = False
    urgent: bool = False
    on_click: InlineHandler | None = None
    widget_type: str = field(init=False, default="pager_item")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload.update({"id": self.id, "label": self.label, "appearance": self.appearance.value, "active": self.active, "inactive": self.inactive, "occupied": self.occupied, "urgent": self.urgent})
        return payload

    def bind_handlers(self, registry: HandlerRegistry, path: tuple[int, ...]) -> None:
        if self.on_click:
            registry.add("click", str(self.id), self.on_click)


@dataclass
class PagerStrip(Widget):
    id: str | None = None
    items: list[PagerItem] = field(default_factory=list)
    placeholder: bool = False
    on_change: InlineHandler | None = None
    widget_type: str = field(init=False, default="pager_strip")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        if self.id is not None:
            payload["id"] = self.id
        payload["placeholder"] = self.placeholder
        payload["items"] = [item.data() for item in self.items]
        return payload

    def bind_handlers(self, registry: HandlerRegistry, path: tuple[int, ...]) -> None:
        if self.id and self.on_change:
            registry.add("change", self.id, self.on_change)
        for index, item in enumerate(self.items):
            item.bind_handlers(registry, (*path, index))


@dataclass
class ActiveIndicator(Widget):
    active: bool = False

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload["active"] = self.active
        return payload


@dataclass
class CameraIndicator(ActiveIndicator):
    widget_type: str = field(init=False, default="camera_indicator")


@dataclass
class MicIndicator(ActiveIndicator):
    widget_type: str = field(init=False, default="mic_indicator")


@dataclass
class MutedIndicator(ActiveIndicator):
    widget_type: str = field(init=False, default="muted_indicator")


@dataclass
class LocationIndicator(ActiveIndicator):
    widget_type: str = field(init=False, default="location_indicator")


@dataclass
class ScreenCastIndicator(ActiveIndicator):
    timer_text: str | None = None
    widget_type: str = field(init=False, default="screencast_indicator")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        if self.timer_text is not None:
            payload["timer_text"] = self.timer_text
        return payload


@dataclass
class Calendar(Widget):
    selected_date: str = ""
    id: str | None = None
    event_days: list[str] = field(default_factory=list)
    on_change: InlineHandler | None = None
    widget_type: str = field(init=False, default="calendar")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        if self.id is not None:
            payload["id"] = self.id
        payload["selected_date"] = self.selected_date
        payload["event_days"] = self.event_days
        return payload

    def bind_handlers(self, registry: HandlerRegistry, path: tuple[int, ...]) -> None:
        if self.id and self.on_change:
            registry.add("change", self.id, self.on_change)


@dataclass
class BatteryHero(Widget):
    icon: str = ""
    percentage: str = ""
    fraction: float = 0.0
    state: str = ""
    widget_type: str = field(init=False, default="battery_hero")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload.update({"icon": self.icon, "percentage": self.percentage, "fraction": self.fraction, "state": self.state})
        return payload


@dataclass
class DateHero(Widget):
    weekday: str = ""
    date: str = ""
    widget_type: str = field(init=False, default="date_hero")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload.update({"weekday": self.weekday, "date": self.date})
        return payload


@dataclass
class EventItem:
    id: str
    title: str
    start: str
    end: str = ""
    location: str | None = None
    all_day: bool = False

    def to_protocol(self) -> dict[str, Any]:
        payload = {"id": self.id, "title": self.title, "start": self.start, "end": self.end, "all_day": self.all_day}
        if self.location is not None:
            payload["location"] = self.location
        return payload


@dataclass
class Events(Widget):
    date: str = ""
    events: list[EventItem] = field(default_factory=list)
    loading: bool = False
    widget_type: str = field(init=False, default="events")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload["date"] = self.date
        payload["loading"] = self.loading
        payload["events"] = [event.to_protocol() for event in self.events]
        return payload


@dataclass
class WeatherForecastItem:
    day_name: str
    icon: str
    condition: str
    temperatures: str
    is_today: bool = False

    def to_protocol(self) -> dict[str, Any]:
        return {
            "day_name": self.day_name,
            "icon": self.icon,
            "condition": self.condition,
            "temperatures": self.temperatures,
            "is_today": self.is_today,
        }


@dataclass
class WeatherForecastList(Widget):
    items: list[WeatherForecastItem] = field(default_factory=list)
    widget_type: str = field(init=False, default="weather_forecast_list")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload["items"] = [item.to_protocol() for item in self.items]
        return payload


@dataclass
class WeatherHourlyItem:
    time: str
    icon: str
    temperature: str

    def to_protocol(self) -> dict[str, Any]:
        return {"time": self.time, "icon": self.icon, "temperature": self.temperature}


@dataclass
class WeatherHourlyStrip(Widget):
    items: list[WeatherHourlyItem] = field(default_factory=list)
    widget_type: str = field(init=False, default="weather_hourly_strip")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload["items"] = [item.to_protocol() for item in self.items]
        return payload


@dataclass
class WorldClockRow:
    name: str
    timezone: str
    time: str
    offset: str
    day_label: str | None = None

    def to_protocol(self) -> dict[str, Any]:
        payload = {"name": self.name, "timezone": self.timezone, "time": self.time, "offset": self.offset}
        if self.day_label is not None:
            payload["day_label"] = self.day_label
        return payload


@dataclass
class WorldClock(Widget):
    rows: list[WorldClockRow] = field(default_factory=list)
    widget_type: str = field(init=False, default="world_clock")

    def data(self) -> dict[str, Any]:
        payload = super().data()
        payload["rows"] = [row.to_protocol() for row in self.rows]
        return payload


TreeNode: TypeAlias = (
    Row
    | Column
    | Container
    | BoxedList
    | PopoverShell
    | Text
    | Header
    | Hero
    | Badge
    | StatusDot
    | PanelIndicator
    | EmptyState
    | Spinner
    | Meter
    | Separator
    | Scroll
    | Tile
    | SegmentedTile
    | ButtonRow
    | SwitchTile
    | ExpanderTile
    | SliderTile
    | ChoiceTile
    | ChoiceList
    | KeyValueGrid
    | PagerItem
    | PagerStrip
    | CameraIndicator
    | MicIndicator
    | MutedIndicator
    | ScreenCastIndicator
    | LocationIndicator
    | Calendar
    | BatteryHero
    | DateHero
    | Events
    | WeatherForecastList
    | WeatherHourlyStrip
    | WorldClock
)
