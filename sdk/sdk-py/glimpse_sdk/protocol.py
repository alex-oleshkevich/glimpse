from __future__ import annotations

from dataclasses import dataclass, field, replace


@dataclass(slots=True)
class StatusItem:
    id: str | None = None
    icon: str | None = None
    label: str | None = None
    tooltip: str | None = None
    css_classes: list[str] = field(default_factory=list)

    def with_css_class(self, css_class: str) -> "StatusItem":
        """Return a copy with ``css_class`` appended. Immutable-update form
        mirroring the Rust SDK's ``css_class()`` builder so SDK-hopping users
        see equivalent ergonomics."""
        return replace(self, css_classes=[*self.css_classes, css_class])

    def to_protocol(self) -> dict[str, object]:
        payload: dict[str, object] = {}
        if self.id is not None:
            payload["id"] = self.id
        if self.icon is not None:
            payload["icon"] = self.icon
        if self.label is not None:
            payload["label"] = self.label
        if self.tooltip is not None:
            payload["tooltip"] = self.tooltip
        if self.css_classes:
            payload["css_classes"] = list(self.css_classes)
        return payload
