"""Subscribe to shell events and dispatch an action.

Run against a live Glimpse session:
    python3 examples/ipc.py
"""

from __future__ import annotations

import asyncio

import glimpse_sdk


async def main() -> None:
    # Cheap: resolves the socket path, no connection yet.
    sub = glimpse_sdk.ipc(service="shell")

    # One-shot connection; awaits the ack. Raises IpcError on ok=false.
    ack = await sub.dispatch("open_uri", {"uri": "https://example.com"})
    print("dispatch ack:", ack)

    # Async generator; the socket closes when the loop exits.
    async for ev in sub.listen("audio.*"):
        print(ev.name, ev.ts, ev.fields)


if __name__ == "__main__":
    asyncio.run(main())
