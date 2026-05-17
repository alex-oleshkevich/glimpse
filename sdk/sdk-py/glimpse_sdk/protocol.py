from __future__ import annotations

from dataclasses import dataclass


@dataclass(slots=True)
class StatusItem:
    id: str | None = None
    icon: str | None = None
    label: str | None = None
    tooltip: str | None = None

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
        return payload
