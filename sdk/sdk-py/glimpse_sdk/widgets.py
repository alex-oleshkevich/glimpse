from __future__ import annotations

from collections.abc import Iterable, Mapping
from dataclasses import dataclass, field
from enum import StrEnum
from typing import TypeAlias



class Align(StrEnum):
    FILL = "fill"
    START = "start"
    END = "end"
    CENTER = "center"
    BASELINE = "baseline"


class Orientation(StrEnum):
    HORIZONTAL = "horizontal"
    VERTICAL = "vertical"


class Variant(StrEnum):
    NORMAL = "normal"
    MUTED = "muted"
    ACCENT = "accent"
    SUCCESS = "success"
    WARNING = "warning"
    DANGER = "danger"


class ButtonVariant(StrEnum):
    PRIMARY = "primary"
    SECONDARY = "secondary"
    COMPACT = "compact"
    FLAT = "flat"
    DANGER = "danger"


class PagerAppearance(StrEnum):
    DOTS = "dots"
    NUMBERS = "numbers"


class ContentFit(StrEnum):
    FILL = "fill"
    CONTAIN = "contain"
    COVER = "cover"
    SCALE_DOWN = "scale_down"


class LevelBarMode(StrEnum):
    CONTINUOUS = "continuous"
    DISCRETE = "discrete"


@dataclass(slots=True)
class CommonProps:
    visible: bool | None = None
    hexpand: bool | None = None
    vexpand: bool | None = None
    halign: Align | None = None
    valign: Align | None = None
    tooltip: str | None = None
    css_classes: list[str] = field(default_factory=list)

    def apply_common(self, payload: dict[str, object]) -> dict[str, object]:
        if self.visible is not None:
            payload["visible"] = self.visible
        if self.hexpand is not None:
            payload["hexpand"] = self.hexpand
        if self.vexpand is not None:
            payload["vexpand"] = self.vexpand
        if self.halign is not None:
            payload["halign"] = self.halign.value
        if self.valign is not None:
            payload["valign"] = self.valign.value
        if self.tooltip is not None:
            payload["tooltip"] = self.tooltip
        if self.css_classes:
            payload["css_classes"] = self.css_classes
        return payload


class Widget(CommonProps):
    widget_type: str = ""

    def to_protocol(self) -> dict[str, object]:
        raise NotImplementedError


@dataclass(slots=True)
class Hero(Widget):
    title: str = ""
    subtitle: str = ""
    icon: str | None = None
    id: str | None = None
    switch: bool | None = None
    widget_type: str = "hero"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({
            "title": self.title,
            "subtitle": self.subtitle,
        })
        if self.icon is not None:
            payload["icon"] = self.icon
        if self.id is not None:
            payload["id"] = self.id
        if self.switch is not None:
            payload["switch"] = self.switch
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Progress(Widget):
    value: float = 0.0
    max: float = 1.0
    show_text: bool = False
    text: str | None = None
    widget_type: str = "progress"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"value": self.value, "max": self.max})
        if self.show_text:
            payload["show_text"] = self.show_text
        if self.text is not None:
            payload["text"] = self.text
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Label(Widget):
    text: str = ""
    wrap: bool = False
    xalign: float | None = None
    selectable: bool = False
    variant: Variant | None = None
    widget_type: str = "label"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"text": self.text})
        if self.wrap:
            payload["wrap"] = self.wrap
        if self.xalign is not None:
            payload["xalign"] = self.xalign
        if self.selectable:
            payload["selectable"] = self.selectable
        if self.variant is not None:
            payload["variant"] = self.variant.value
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class LevelBar(Widget):
    value: float = 0.0
    min: float = 0.0
    max: float = 1.0
    mode: LevelBarMode | str = LevelBarMode.CONTINUOUS
    widget_type: str = "level_bar"

    def to_protocol(self) -> dict[str, object]:
        mode = self.mode.value if isinstance(self.mode, LevelBarMode) else self.mode
        payload = self.apply_common(
            {"value": self.value, "min": self.min, "max": self.max, "mode": mode}
        )
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Icon(Widget):
    icon: str | None = None
    pixel_size: int | None = None
    widget_type: str = "icon"

    def to_protocol(self) -> dict[str, object]:
        if self.icon is None:
            raise ValueError("Icon requires an icon")
        payload = self.apply_common({"icon": self.icon})
        if self.pixel_size is not None:
            payload["pixel_size"] = self.pixel_size
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Picture(Widget):
    path: str = ""
    content_fit: ContentFit | str | None = None
    widget_type: str = "picture"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"path": self.path})
        if self.content_fit is not None:
            payload["content_fit"] = (
                self.content_fit.value
                if isinstance(self.content_fit, ContentFit)
                else self.content_fit
            )
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Button(Widget):
    id: str = ""
    label: str | None = None
    icon: str | None = None
    enabled: bool | None = None
    variant: ButtonVariant | None = None
    widget_type: str = "button"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"id": self.id})
        if self.label is not None:
            payload["label"] = self.label
        if self.icon is not None:
            payload["icon"] = self.icon
        if self.enabled is not None:
            payload["enabled"] = self.enabled
        if self.variant is not None:
            payload["variant"] = self.variant.value
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class LinkButton(Widget):
    uri: str = ""
    label: str | None = None
    widget_type: str = "link_button"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"uri": self.uri})
        if self.label is not None:
            payload["label"] = self.label
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Expander(Widget):
    label: str = ""
    child: "TreeNode | None" = None
    expanded: bool = False
    widget_type: str = "expander"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"label": self.label, "expanded": self.expanded})
        if self.child is not None:
            payload["child"] = self.child.to_protocol()
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class TreeExpander(Widget):
    child: "TreeNode | None" = None
    hide_expander: bool = False
    indent_for_depth: bool = False
    indent_for_icon: bool = False
    widget_type: str = "tree_expander"

    def to_protocol(self) -> dict[str, object]:
        if self.child is None:
            raise ValueError("TreeExpander requires a child")
        payload = self.apply_common(
            {
                "child": self.child.to_protocol(),
                "hide_expander": self.hide_expander,
                "indent_for_depth": self.indent_for_depth,
                "indent_for_icon": self.indent_for_icon,
            }
        )
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class MenuButton(Widget):
    label: str | None = None
    icon: str | None = None
    popover: "TreeNode | None" = None
    widget_type: str = "menu_button"

    def to_protocol(self) -> dict[str, object]:
        if self.popover is None:
            raise ValueError("MenuButton requires a popover")
        payload = self.apply_common({"popover": self.popover.to_protocol()})
        if self.label is not None:
            payload["label"] = self.label
        if self.icon is not None:
            payload["icon"] = self.icon
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Switch(Widget):
    id: str = ""
    label: str | None = None
    active: bool = False
    widget_type: str = "switch"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"id": self.id, "active": self.active})
        if self.label is not None:
            payload["label"] = self.label
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class ToggleButton(Widget):
    id: str = ""
    label: str | None = None
    active: bool = False
    widget_type: str = "toggle_button"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"id": self.id, "active": self.active})
        if self.label is not None:
            payload["label"] = self.label
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Slider(Widget):
    id: str = ""
    min: float = 0.0
    max: float = 1.0
    step: float = 0.1
    value: float = 0.0
    orientation: Orientation | None = None
    draw_value: bool = False
    widget_type: str = "slider"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common(
            {
                "id": self.id,
                "min": self.min,
                "max": self.max,
                "step": self.step,
                "value": self.value,
            }
        )
        if self.orientation is not None:
            payload["orientation"] = self.orientation.value
        if self.draw_value:
            payload["draw_value"] = self.draw_value
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Checkbox(Widget):
    id: str = ""
    label: str | None = None
    active: bool = False
    widget_type: str = "checkbox"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"id": self.id, "active": self.active})
        if self.label is not None:
            payload["label"] = self.label
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Select(Widget):
    id: str = ""
    items: list[dict[str, str]] = field(default_factory=list)
    selected: int | None = None
    widget_type: str = "select"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"id": self.id, "items": self.items})
        if self.selected is not None:
            payload["selected"] = self.selected
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Separator(Widget):
    orientation: Orientation | None = None
    widget_type: str = "separator"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({})
        if self.orientation is not None:
            payload["orientation"] = self.orientation.value
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Scroll(Widget):
    child: "TreeNode | None" = None
    widget_type: str = "scroll"

    def to_protocol(self) -> dict[str, object]:
        if self.child is None:
            raise ValueError("Scroll requires a child")
        payload = self.apply_common({"child": self.child.to_protocol()})
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Overlay(Widget):
    child: "TreeNode | None" = None
    overlays: list["TreeNode"] = field(default_factory=list)
    widget_type: str = "overlay"

    def to_protocol(self) -> dict[str, object]:
        if self.child is None:
            raise ValueError("Overlay requires a child")
        payload = self.apply_common(
            {
                "child": self.child.to_protocol(),
                "overlays": [overlay.to_protocol() for overlay in self.overlays],
            }
        )
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class ListBox(Widget):
    children: list["TreeNode"] = field(default_factory=list)
    widget_type: str = "list_box"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common(
            {"children": [child.to_protocol() for child in self.children]}
        )
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class GridChild:
    row: int
    column: int
    child: "TreeNode"
    width: int = 1
    height: int = 1

    def to_protocol(self) -> dict[str, object]:
        return {
            "row": self.row,
            "column": self.column,
            "width": self.width,
            "height": self.height,
            "child": self.child.to_protocol(),
        }


@dataclass(slots=True)
class Grid(Widget):
    children: list[GridChild] = field(default_factory=list)
    row_spacing: int = 0
    column_spacing: int = 0
    widget_type: str = "grid"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common(
            {
                "row_spacing": self.row_spacing,
                "column_spacing": self.column_spacing,
                "children": [child.to_protocol() for child in self.children],
            }
        )
        return {"type": self.widget_type, "data": payload}



@dataclass(slots=True)
class Card(Widget):
    child: "TreeNode | None" = None
    widget_type: str = "card"

    def to_protocol(self) -> dict[str, object]:
        data: dict[str, object] = {}
        if self.child is not None:
            data["child"] = self.child.to_protocol()
        payload = self.apply_common(data)
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Section(Widget):
    title: str = ""
    subtitle: str = ""
    child: "TreeNode | None" = None
    widget_type: str = "section"

    def to_protocol(self) -> dict[str, object]:
        data: dict[str, object] = {"title": self.title}
        if self.child is not None:
            data["child"] = self.child.to_protocol()
        payload = self.apply_common(data)
        if self.subtitle:
            payload["subtitle"] = self.subtitle
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Meter(Widget):
    id: str | None = None
    icon: str | None = None
    label: str = ""
    value: float = 0.0
    min: float = 0.0
    max: float = 1.0
    step: float = 0.01
    text: str | None = None
    interactive: bool = False
    widget_type: str = "meter"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common(
            {
                "label": self.label,
                "value": self.value,
                "min": self.min,
                "max": self.max,
                "step": self.step,
                "interactive": self.interactive,
            }
        )
        if self.id is not None:
            payload["id"] = self.id
        if self.icon is not None:
            payload["icon"] = self.icon
        if self.text is not None:
            payload["text"] = self.text
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Copyable(Widget):
    label: str = ""
    value: str = ""
    widget_type: str = "copyable"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"label": self.label, "value": self.value})
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Row(Widget):
    spacing: int = 0
    children: list["TreeNode"] = field(default_factory=list)
    widget_type: str = "row"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common(
            {
                "spacing": self.spacing,
                "children": [child.to_protocol() for child in self.children],
            }
        )
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Column(Widget):
    spacing: int = 0
    children: list["TreeNode"] = field(default_factory=list)
    widget_type: str = "column"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common(
            {
                "spacing": self.spacing,
                "children": [child.to_protocol() for child in self.children],
            }
        )
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Spinner(Widget):
    spinning: bool = True
    widget_type: str = "spinner"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"spinning": self.spinning})
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True, init=False)
class PropertyList(Widget):
    rows: Mapping[str, str] | Iterable[tuple[str, str]] = field(default_factory=dict)
    title: str = ""
    widget_type: str = "property_list"

    def __init__(
        self,
        rows: Mapping[str, str] | Iterable[tuple[str, str]] | None = None,
        title: str = "",
        **common: object,
    ) -> None:
        unknown = sorted(set(common) - set(CommonProps.__dataclass_fields__))
        if unknown:
            raise TypeError(f"unexpected keyword argument: {unknown[0]}")
        for name in CommonProps.__dataclass_fields__:
            setattr(self, name, common.get(name))
        self.rows = {} if rows is None else rows
        self.title = title
        self.widget_type = "property_list"

    def to_protocol(self) -> dict[str, object]:
        rows = self.rows.items() if isinstance(self.rows, Mapping) else self.rows
        payload = self.apply_common(
            {
                "rows": [{"key": key, "value": value} for key, value in rows],
            }
        )
        if self.title:
            payload["title"] = self.title
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Item(Widget):
    label: str = ""
    sublabel: str = ""
    icon: str = ""
    left: "TreeNode | None" = None
    right: "TreeNode | None" = None
    widget_type: str = "item"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"label": self.label})
        left = self.left or (Icon(icon=self.icon, pixel_size=16) if self.icon else None)
        if left is not None:
            payload["left"] = left.to_protocol()
        if self.sublabel:
            payload["sublabel"] = self.sublabel
        if self.right is not None:
            payload["right"] = self.right.to_protocol()
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class ActionItem(Widget):
    id: str = ""
    label: str = ""
    sublabel: str = ""
    icon: str = ""
    left: "TreeNode | None" = None
    right: "TreeNode | None" = None
    enabled: bool | None = None
    widget_type: str = "action_item"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"id": self.id, "label": self.label})
        left = self.left or (Icon(icon=self.icon, pixel_size=16) if self.icon else None)
        if left is not None:
            payload["left"] = left.to_protocol()
        if self.sublabel:
            payload["sublabel"] = self.sublabel
        if self.right is not None:
            payload["right"] = self.right.to_protocol()
        if self.enabled is not None:
            payload["enabled"] = self.enabled
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class EmptyState(Widget):
    title: str = ""
    subtitle: str = ""
    widget_type: str = "empty_state"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"title": self.title, "subtitle": self.subtitle})
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Badge(Widget):
    label: str = ""
    variant: Variant | None = None
    widget_type: str = "badge"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"label": self.label})
        if self.variant is not None:
            payload["variant"] = self.variant.value
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class StatusDot(Widget):
    variant: Variant | None = None
    widget_type: str = "status"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({})
        if self.variant is not None:
            payload["variant"] = self.variant.value
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class PagerItem(Widget):
    id: str | None = None
    appearance: PagerAppearance = PagerAppearance.DOTS
    label: str = ""
    active: bool = False
    inactive: bool = False
    occupied: bool = False
    urgent: bool = False
    widget_type: str = "pager_item"

    def to_data(self) -> dict[str, object]:
        payload = self.apply_common(
            {
                "appearance": self.appearance.value,
                "label": self.label,
                "active": self.active,
                "inactive": self.inactive,
                "occupied": self.occupied,
                "urgent": self.urgent,
            }
        )
        if self.id is not None:
            payload["id"] = self.id
        return payload

    def to_protocol(self) -> dict[str, object]:
        return {"type": self.widget_type, "data": self.to_data()}


@dataclass(slots=True)
class PagerStrip(Widget):
    id: str | None = None
    items: list[PagerItem] = field(default_factory=list)
    widget_type: str = "pager_strip"

    def to_protocol(self) -> dict[str, object]:
        data: dict[str, object] = {}
        if self.id:
            data["id"] = self.id
        data["items"] = [item.to_data() for item in self.items]
        payload = self.apply_common(data)
        return {"type": self.widget_type, "data": payload}


TreeNode: TypeAlias = (
    Hero
    | Card
    | Section
    | Meter
    | Copyable
    | PropertyList
    | Item
    | ActionItem
    | EmptyState
    | Badge
    | StatusDot
    | PagerItem
    | PagerStrip
    | Row
    | Column
    | Grid
    | Scroll
    | Overlay
    | ListBox
    | LevelBar
    | TreeExpander
    | MenuButton
    | Progress
    | Separator
    | Spinner
    | Label
    | Icon
    | Picture
    | Button
    | LinkButton
    | Expander
    | Switch
    | ToggleButton
    | Slider
    | Select
    | Checkbox
)
