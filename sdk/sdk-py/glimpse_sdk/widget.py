import collections.abc
import typing

from glimpse_sdk.protocol import TEXT_KINDS, _camel, _clean, _flatten

type Key = str | int | None
type Child = W | str | int | float | bool | collections.abc.Sequence[Child] | None


class W:
    __slots__ = ("children", "handlers", "key", "kind", "props")

    def __init__(
        self, kind: str, children: Child = (), key: Key = None, **props: typing.Any
    ) -> None:
        flat = _flatten(children)

        self.kind = kind
        self.key = key
        self.props: dict[str, typing.Any] = {}
        self.handlers: dict[str, collections.abc.Callable[..., typing.Any]] = {}

        for name, value in props.items():
            wire = _camel(name)

            if callable(value):
                self.handlers[wire] = value
                self.props[wire] = True
            elif value is not None:
                cap = 8 if wire == "badge" else 4096 if kind == "entry" and wire == "value" else 256
                self.props[wire] = value[:cap] if isinstance(value, str) else _clean(value)

        if (
            kind in TEXT_KINDS
            and flat
            and all(isinstance(c, (str, int, float)) and not isinstance(c, bool) for c in flat)
        ):
            self.props["text"] = "".join(str(c) for c in flat)[:256]
            flat = []

        self.children = [c if isinstance(c, W) else W("label", text=str(c)[:256]) for c in flat]
