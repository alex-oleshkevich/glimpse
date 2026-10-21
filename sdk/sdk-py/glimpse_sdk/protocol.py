import math
import typing

TEXT_KINDS = {"label", "indicator", "button"}
CONTROLLED = {
    "entry": ("onChange", "value"),
    "entryrow": ("onChange", "value"),
    "scale": ("onChange", "value"),
    "switch": ("onToggle", "active"),
    "togglerow": ("onToggle", "active"),
    "togglebutton": ("onToggle", "active"),
    "checkbutton": ("onToggle", "active"),
    "expanderrow": ("onToggle", "open"),
    "sliderrow": ("onChange", "value"),
    "dropdown": ("onSelect", "selected"),
    "choicelist": ("onSelect", "selected"),
}
MAX_LINE = 1_048_576
MAX_NODES = 300
MAX_CHILDREN = 200
MAX_OPS = 1200
MAX_ITEMS = 64
MAX_SERIES = 366


class LimitError(Exception):
    pass


class RenderError(Exception):
    pass


def _camel(name: str) -> str:
    head, *rest = name.split("_")
    return head + "".join(part.title() for part in rest)


def _flatten(children: typing.Any) -> list[typing.Any]:
    out: list[typing.Any] = []

    for child in children:
        match child:
            case None | bool():
                continue
            case list() | tuple():
                out.extend(_flatten(child))
            case _:
                out.append(child)

    return out


def _clean(value: typing.Any) -> typing.Any:
    match value:
        case float() if not math.isfinite(value):
            return None
        case list():
            return [_clean(item) for item in value]
        case dict():
            return {key: _clean(item) for key, item in value.items()}
        case _:
            return value


def _oversized(value: typing.Any, limit: int = MAX_ITEMS) -> bool:
    match value:
        case list():
            return len(value) > limit or any(_oversized(item) for item in value)
        case dict():
            return any(_oversized(item) for item in value.values())
        case _:
            return False
