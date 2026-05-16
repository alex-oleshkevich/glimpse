"""Minimal client for the Glimpse IPC socket.

``ipc(service)`` resolves a :class:`Subscriber` (no I/O — the connection is
opened lazily). :meth:`Subscriber.listen` subscribes to an event channel and
yields decoded :class:`Event` objects; :meth:`Subscriber.dispatch` sends an
action and awaits the server ack on a one-shot connection. The wire protocol
matches the ``glimpse-shell watch`` / ``dispatch`` CLIs.
"""

from __future__ import annotations

import asyncio
import contextlib
import os
from collections.abc import AsyncIterator, Mapping
from dataclasses import dataclass, field
from pathlib import Path

__all__ = ["Event", "IpcError", "Subscriber", "ipc"]


class IpcError(RuntimeError):
    """Raised on connection failure or a rejected dispatch (``ok=false``)."""


@dataclass(frozen=True, slots=True)
class Event:
    """One decoded event line; ``fields`` values are unescaped."""

    name: str
    ts: int
    fields: dict[str, str] = field(default_factory=dict)


@dataclass(frozen=True, slots=True)
class Subscriber:
    """A resolved IPC endpoint. Cheap to create; holds only the path."""

    _socket: Path

    async def listen(self, channel: str) -> AsyncIterator[Event]:
        """Subscribe to ``channel`` (an exact name, ``prefix.*``, or ``*``)
        and yield events until the server closes the connection."""
        reader, writer = await self._connect()
        try:
            writer.write(f"subscribe {channel}\n".encode())
            await writer.drain()
            while True:
                raw = await reader.readline()
                if not raw:
                    return
                line = raw.decode(errors="replace").strip()
                if line:
                    yield _parse_event(line)
        finally:
            await _close(writer)

    async def dispatch(
        self, action: str, params: Mapping[str, str] | None = None
    ) -> dict[str, str]:
        """Dispatch ``action`` with ``params`` on a fresh connection and
        await the ack. Returns the extra ack fields; raises :class:`IpcError`
        if the server replies ``ok=false``."""
        _validate_token("action", action, forbid_eq=False)
        items = list((params or {}).items())
        for key, _ in items:
            _validate_token("param key", key, forbid_eq=True)
        reader, writer = await self._connect()
        try:
            line = action
            for key, value in items:
                line += f" {key}={_escape(value)}"
            writer.write((line + "\n").encode())
            await writer.drain()
            raw = await reader.readline()
            if not raw:
                raise IpcError("IPC server closed connection without ack")
            return _parse_ack(raw.decode(errors="replace").strip())
        finally:
            await _close(writer)

    async def _connect(
        self,
    ) -> tuple[asyncio.StreamReader, asyncio.StreamWriter]:
        try:
            reader, writer = await asyncio.open_unix_connection(self._socket)
        except OSError as exc:
            raise IpcError(
                f"cannot connect to IPC socket at {self._socket}: {exc}"
            ) from exc
        hello = await reader.readline()
        if not hello:
            await _close(writer)
            raise IpcError("IPC server closed connection before hello")
        if not hello.decode(errors="replace").startswith("hello"):
            await _close(writer)
            raise IpcError(f"unexpected IPC greeting: {hello!r}")
        return reader, writer


def ipc(service: str = "shell") -> Subscriber:
    """Resolve the :class:`Subscriber` for ``service``.

    The socket is ``<dir>/<service>.sock`` (``shell`` maps to ``ipc.sock``)
    where ``<dir>`` is ``$GLIMPSE_IPC_DIR``, else ``$XDG_RUNTIME_DIR/glimpse``.
    No connection is made here.
    """
    return Subscriber(_socket_path(service))


def _socket_path(service: str) -> Path:
    override = os.environ.get("GLIMPSE_IPC_DIR")
    if override:
        directory = Path(override)
    else:
        runtime = os.environ.get("XDG_RUNTIME_DIR")
        if not runtime:
            raise IpcError(
                "neither GLIMPSE_IPC_DIR nor XDG_RUNTIME_DIR is set; "
                "cannot locate the Glimpse IPC socket"
            )
        directory = Path(runtime) / "glimpse"
    name = "ipc.sock" if service == "shell" else f"{service}.sock"
    return directory / name


async def _close(writer: asyncio.StreamWriter) -> None:
    writer.close()
    with contextlib.suppress(Exception):
        await writer.wait_closed()


def _validate_token(label: str, token: str, *, forbid_eq: bool) -> None:
    """The wire protocol splits client lines on whitespace and never
    unescapes the command name or a field key, so an ``action``/key with
    whitespace would forge extra tokens or whole client lines. Values are
    safe (escaped); reject the unsafe shapes loudly."""
    if not token:
        raise IpcError(f"IPC {label} must not be empty")
    if any(ch.isspace() for ch in token):
        raise IpcError(f"IPC {label} {token!r} must not contain whitespace")
    if forbid_eq and "=" in token:
        raise IpcError(f"IPC param key {token!r} must not contain '='")


def _escape(value: str) -> str:
    return (
        value.replace("\\", "\\\\")
        .replace("\n", "\\n")
        .replace("\t", "\\t")
        .replace(" ", "\\s")
    )


def _unescape(value: str) -> str:
    out: list[str] = []
    chars = iter(value)
    for char in chars:
        if char != "\\":
            out.append(char)
            continue
        nxt = next(chars, None)
        if nxt == "s":
            out.append(" ")
        elif nxt == "n":
            out.append("\n")
        elif nxt == "t":
            out.append("\t")
        elif nxt == "\\":
            out.append("\\")
        elif nxt is None:
            out.append("\\")
        else:
            out.append("\\")
            out.append(nxt)
    return "".join(out)


def _parse_event(line: str) -> Event:
    tokens = line.split()
    name = tokens[0] if tokens else ""
    ts = 0
    fields: dict[str, str] = {}
    for token in tokens[1:]:
        key, sep, raw = token.partition("=")
        if not sep:
            continue
        value = _unescape(raw)
        if key == "ts":
            try:
                ts = int(value)
                continue
            except ValueError:
                pass
        fields[key] = value
    return Event(name=name, ts=ts, fields=fields)


def _parse_ack(line: str) -> dict[str, str]:
    tokens = line.split()
    if not tokens or tokens[0] != "ack":
        raise IpcError(f"expected an ack, got: {line}")
    ok = False
    fields: dict[str, str] = {}
    for token in tokens[1:]:
        key, sep, raw = token.partition("=")
        if not sep:
            continue
        value = _unescape(raw)
        if key == "ok":
            ok = value == "true"
        else:
            fields[key] = value
    if not ok:
        raise IpcError(fields.get("error", "command failed"))
    return fields
