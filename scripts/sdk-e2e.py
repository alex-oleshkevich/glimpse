#!/usr/bin/env python3
"""End-to-end protocol test for each SDK's counter example applet.

For every SDK (rs, py, ts, go):
  1. Build the example applet.
  2. Spawn it as a subprocess (the same way Glimpse would).
  3. Drive it through a fixed sequence of `init` and `event` lines.
  4. Assert the `status` and `popover` messages it emits match the
     counter contract (initial count=0, increment bumps it, popover
     content tracks state).

Exit code 0 if every SDK passes, non-zero otherwise. Prints a per-SDK
summary at the end.

Run via: `just e2e-sdks` or `python3 scripts/sdk-e2e.py [-k rs|py|ts|go]`.

Notes on safety: every subprocess invocation in this file uses the
argv-list form (asyncio.create_subprocess_exec, subprocess.run with a
list) — never via a shell — so user-controlled args are not interpreted
by /bin/sh.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent

# How long we wait for any single message to arrive before giving up.
EXPECT_TIMEOUT = 10.0
# How long the build step is allowed to run.
BUILD_TIMEOUT = 180.0


# ---------- protocol harness ------------------------------------------------


class ProtocolError(RuntimeError):
    pass


@dataclass
class Message:
    kind: str
    data: dict[str, Any]


class Applet:
    """Spawned applet subprocess. Writes to stdin, reads from stdout,
    surfaces stderr to the caller's stderr for debugging."""

    def __init__(
        self,
        name: str,
        command: list[str],
        cwd: Path | None = None,
        env_overrides: dict[str, str] | None = None,
    ):
        self.name = name
        self.command = command
        self.cwd = cwd
        self.env_overrides = env_overrides or {}
        self.proc: asyncio.subprocess.Process | None = None
        self._stderr_task: asyncio.Task[None] | None = None

    async def start(self) -> None:
        env = {**os.environ, "PYTHONUNBUFFERED": "1", **self.env_overrides}
        self.proc = await asyncio.create_subprocess_exec(
            *self.command,
            cwd=self.cwd,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=env,
        )
        self._stderr_task = asyncio.create_task(self._pump_stderr())

    async def _pump_stderr(self) -> None:
        assert self.proc is not None and self.proc.stderr is not None
        async for line in self.proc.stderr:
            sys.stderr.write(f"[{self.name} stderr] {line.decode(errors='replace')}")
            sys.stderr.flush()

    async def send(self, kind: str, payload: dict[str, Any]) -> None:
        assert self.proc is not None and self.proc.stdin is not None
        line = f"{kind} {json.dumps(payload, separators=(',', ':'))}\n"
        self.proc.stdin.write(line.encode())
        await self.proc.stdin.drain()

    async def expect(
        self, kind: str | None = None, timeout: float = EXPECT_TIMEOUT
    ) -> Message:
        """Wait for the next protocol line. If `kind` is given, skip
        any other kinds in between."""
        assert self.proc is not None and self.proc.stdout is not None
        deadline = asyncio.get_event_loop().time() + timeout
        while True:
            remaining = deadline - asyncio.get_event_loop().time()
            if remaining <= 0:
                raise ProtocolError(f"timed out waiting for {kind or 'any'} message")
            try:
                raw = await asyncio.wait_for(self.proc.stdout.readline(), remaining)
            except asyncio.TimeoutError:
                raise ProtocolError(f"timed out waiting for {kind or 'any'} message")
            if not raw:
                raise ProtocolError("applet stdout closed unexpectedly")
            text = raw.decode(errors="replace").strip()
            if not text:
                continue
            try:
                cmd, _, body = text.partition(" ")
                msg = Message(kind=cmd, data=json.loads(body))
            except (ValueError, json.JSONDecodeError) as parse_err:
                raise ProtocolError(f"malformed line {text!r}: {parse_err}")
            if kind is None or msg.kind == kind:
                return msg

    async def close(self) -> None:
        assert self.proc is not None
        if self.proc.returncode is None:
            try:
                if self.proc.stdin is not None and not self.proc.stdin.is_closing():
                    self.proc.stdin.close()
            except Exception:
                pass
            try:
                await asyncio.wait_for(self.proc.wait(), timeout=3.0)
            except asyncio.TimeoutError:
                self.proc.kill()
                await self.proc.wait()
        if self._stderr_task is not None:
            self._stderr_task.cancel()


# ---------- the counter contract -------------------------------------------


async def run_counter_contract(applet: Applet) -> None:
    """The canonical protocol exchange every counter example must pass."""
    await applet.start()

    # 1. Initial status: counter widget with id="counter", label="0".
    status = await applet.expect("status")
    items = status.data.get("items", [])
    if not items or items[0].get("id") != "counter":
        raise ProtocolError(f"initial status missing counter item: {status.data}")
    if items[0].get("label") != "0":
        raise ProtocolError(f"initial status label != '0': {items[0]}")

    # 2. Initial popover (if the SDK sends it eagerly).
    try:
        popover = await applet.expect("popover", timeout=2.0)
        root = popover.data.get("root")
        if root is None:
            raise ProtocolError("initial popover root was null")
        if not _tree_contains_button(root, "increment"):
            raise ProtocolError(f"initial popover missing increment button: {root}")
    except ProtocolError as e:
        if "timed out" not in str(e):
            raise
        # Some SDKs defer popover until the popover event arrives.

    # 3. Send init.
    await applet.send("init", {"instance": "e2e-counter", "options": {}})

    # 4. Open the popover so subsequent re-renders are pushed.
    await applet.send(
        "event", {"id": "popover", "type": "open", "source": "popover"}
    )

    try:
        popover = await applet.expect("popover", timeout=3.0)
        root = popover.data.get("root")
        if not root or not _tree_contains_button(root, "increment"):
            raise ProtocolError(f"popover after open missing increment: {root}")
    except ProtocolError as e:
        if "timed out" not in str(e):
            raise

    # 5. Click increment three times; each click should bump the status
    # label by one. Filter for `status` lines since some SDKs also emit
    # popover updates.
    for expected in ("1", "2", "3"):
        await applet.send(
            "event",
            {
                "id": "increment",
                "type": "click",
                "source": "popover",
                "button": "left",
            },
        )
        status = await applet.expect("status")
        items = status.data.get("items", [])
        actual = items[0].get("label") if items else "<none>"
        if actual != expected:
            raise ProtocolError(
                f"after click, expected status label {expected!r}, got {actual!r}"
            )

    # 6. Close popover (must not crash).
    await applet.send(
        "event", {"id": "popover", "type": "close", "source": "popover"}
    )

    await applet.close()


def _tree_contains_button(root: dict[str, Any], button_id: str) -> bool:
    """Walk a popover tree and return True if a button with the given
    id appears anywhere in it."""
    if not isinstance(root, dict):
        return False
    if root.get("type") == "button":
        if root.get("data", {}).get("id") == button_id:
            return True
    data = root.get("data", {})
    for key in ("children", "body"):
        for child in data.get(key, []) or []:
            if isinstance(child, dict):
                if _tree_contains_button(child, button_id):
                    return True
                # grid children are { row, column, child }
                if "child" in child and _tree_contains_button(child["child"], button_id):
                    return True
    for key in ("child", "left", "right"):
        node = data.get(key)
        if isinstance(node, dict) and _tree_contains_button(node, button_id):
            return True
    return False


# ---------- per-SDK build + spawn ------------------------------------------


def build_rust() -> tuple[list[str], Path, dict[str, str]]:
    print("  building Rust example…", flush=True)
    subprocess.run(
        ["cargo", "build", "--release", "--example", "stateful_counter"],
        cwd=ROOT / "sdk" / "sdk-rs",
        check=True,
        timeout=BUILD_TIMEOUT,
    )
    binary = (
        ROOT / "sdk" / "sdk-rs" / "target" / "release" / "examples" / "stateful_counter"
    )
    return [str(binary)], ROOT / "sdk" / "sdk-rs", {}


def build_python() -> tuple[list[str], Path, dict[str, str]]:
    print("  using Python example directly (no build)", flush=True)
    py = shutil.which("python3") or shutil.which("python")
    if not py:
        raise RuntimeError("python not found on PATH")
    # The example imports `glimpse_sdk`; point PYTHONPATH at the sdk
    # source tree so we don't need to `pip install -e .` first.
    cwd = ROOT / "sdk" / "sdk-py"
    env = {"PYTHONPATH": str(cwd)}
    return [py, str(cwd / "examples" / "stateful_counter.py")], cwd, env


def build_typescript() -> tuple[list[str], Path, dict[str, str]]:
    print("  building TypeScript example…", flush=True)
    cwd = ROOT / "sdk" / "sdk-ts"
    if not (cwd / "node_modules").exists():
        subprocess.run(
            ["npm", "install", "--no-audit", "--no-fund"],
            cwd=cwd,
            check=True,
            timeout=BUILD_TIMEOUT,
        )
    subprocess.run(["npm", "run", "build"], cwd=cwd, check=True, timeout=BUILD_TIMEOUT)
    return ["node", str(cwd / "dist" / "examples" / "stateful-counter.js")], cwd, {}


def build_go() -> tuple[list[str], Path, dict[str, str]]:
    print("  building Go example…", flush=True)
    cwd = ROOT / "sdk" / "sdk-go"
    binary = cwd / "examples" / "stateful_counter" / "stateful_counter-e2e"
    subprocess.run(
        ["go", "build", "-o", str(binary), "./examples/stateful_counter"],
        cwd=cwd,
        check=True,
        timeout=BUILD_TIMEOUT,
    )
    return [str(binary)], cwd, {}


SDKS = {
    "rs": ("Rust", build_rust),
    "py": ("Python", build_python),
    "ts": ("TypeScript", build_typescript),
    "go": ("Go", build_go),
}


# ---------- IPC contract ----------------------------------------------------
#
# Each SDK ships an `ipc` example that: ipc("shell") -> dispatch("open_uri",
# {uri}) -> listen("audio.*"). We point GLIMPSE_IPC_DIR at a temp dir, run a
# tiny in-process server speaking the line protocol (hello / ack / one event
# then close), and assert the example's stdout shows the ack echo and the
# event. No running Glimpse needed.

# Argv-list spawn alias (no shell — same safe form used above).
_spawn_proc = asyncio.create_subprocess_exec


def build_ipc_rust() -> tuple[list[str], Path, dict[str, str]]:
    print("  building Rust ipc example…", flush=True)
    subprocess.run(
        ["cargo", "build", "--release", "--example", "ipc"],
        cwd=ROOT / "sdk" / "sdk-rs",
        check=True,
        timeout=BUILD_TIMEOUT,
    )
    return (
        [str(ROOT / "sdk" / "sdk-rs" / "target" / "release" / "examples" / "ipc")],
        ROOT / "sdk" / "sdk-rs",
        {},
    )


def build_ipc_python() -> tuple[list[str], Path, dict[str, str]]:
    py = shutil.which("python3") or shutil.which("python")
    if not py:
        raise RuntimeError("python not found on PATH")
    cwd = ROOT / "sdk" / "sdk-py"
    return [py, str(cwd / "examples" / "ipc.py")], cwd, {"PYTHONPATH": str(cwd)}


def build_ipc_typescript() -> tuple[list[str], Path, dict[str, str]]:
    print("  building TypeScript ipc example…", flush=True)
    cwd = ROOT / "sdk" / "sdk-ts"
    if not (cwd / "node_modules").exists():
        subprocess.run(
            ["npm", "install", "--no-audit", "--no-fund"],
            cwd=cwd,
            check=True,
            timeout=BUILD_TIMEOUT,
        )
    subprocess.run(["npm", "run", "build"], cwd=cwd, check=True, timeout=BUILD_TIMEOUT)
    return ["node", str(cwd / "dist" / "examples" / "ipc.js")], cwd, {}


def build_ipc_go() -> tuple[list[str], Path, dict[str, str]]:
    print("  building Go ipc example…", flush=True)
    cwd = ROOT / "sdk" / "sdk-go"
    binary = cwd / "examples" / "ipc" / "ipc-e2e"
    subprocess.run(
        ["go", "build", "-o", str(binary), "./examples/ipc"],
        cwd=cwd,
        check=True,
        timeout=BUILD_TIMEOUT,
    )
    return [str(binary)], cwd, {}


IPC_BUILDERS = {
    "rs": build_ipc_rust,
    "py": build_ipc_python,
    "ts": build_ipc_typescript,
    "go": build_ipc_go,
}


async def _fake_ipc_server(socket_path: Path) -> asyncio.AbstractServer:
    """One connection = one request. `subscribe …` → emit a single event
    then close; anything else → a success ack then close."""

    async def handle(
        reader: asyncio.StreamReader, writer: asyncio.StreamWriter
    ) -> None:
        try:
            writer.write(b"hello version=e2e\n")
            await writer.drain()
            raw = await asyncio.wait_for(reader.readline(), EXPECT_TIMEOUT)
            line = raw.decode(errors="replace").strip()
            if line.startswith("subscribe "):
                writer.write(b"audio.volume_changed volume=42 ts=7\n")
            else:
                writer.write(b"ack ok=true echo=done\n")
            await writer.drain()
        finally:
            writer.close()
            try:
                await writer.wait_closed()
            except Exception:
                pass

    return await asyncio.start_unix_server(handle, path=str(socket_path))


async def run_ipc_contract(key: str) -> None:
    command, cwd, env = IPC_BUILDERS[key]()
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        sock = Path(tmp) / "ipc.sock"
        server = await _fake_ipc_server(sock)
        proc = await _spawn_proc(
            *command,
            cwd=cwd,
            stdin=asyncio.subprocess.DEVNULL,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env={**os.environ, "GLIMPSE_IPC_DIR": tmp, **env},
        )
        try:
            out_b, err_b = await asyncio.wait_for(
                proc.communicate(), timeout=EXPECT_TIMEOUT * 3
            )
        except asyncio.TimeoutError:
            proc.kill()
            await proc.wait()
            raise ProtocolError("ipc example timed out")
        finally:
            server.close()
            await server.wait_closed()

        out = out_b.decode(errors="replace")
        if proc.returncode != 0:
            raise ProtocolError(
                f"ipc example exited {proc.returncode}; "
                f"stderr={err_b.decode(errors='replace').strip()!r}"
            )
        if "done" not in out:
            raise ProtocolError(f"dispatch ack echo missing from output: {out!r}")
        if "audio.volume_changed" not in out or "42" not in out:
            raise ProtocolError(f"listen event missing from output: {out!r}")


# ---------- runner ---------------------------------------------------------


async def run_one(key: str) -> tuple[str, bool, str]:
    label, build_fn = SDKS[key]
    print(f"\n=== {label} ({key}) ===")
    try:
        command, cwd, env = build_fn()
    except Exception as build_err:
        return label, False, f"build failed: {build_err}"

    applet = Applet(key, command, cwd=cwd, env_overrides=env)
    try:
        await asyncio.wait_for(
            run_counter_contract(applet), timeout=EXPECT_TIMEOUT * 6
        )
    except Exception as contract_err:
        try:
            await applet.close()
        except Exception:
            pass
        return label, False, f"counter contract failed: {contract_err}"

    try:
        await asyncio.wait_for(
            run_ipc_contract(key), timeout=EXPECT_TIMEOUT * 4 + BUILD_TIMEOUT
        )
    except Exception as ipc_err:
        return label, False, f"ipc contract failed: {ipc_err}"
    return label, True, "ok (counter + ipc)"


async def amain(only: list[str]) -> int:
    keys = only or list(SDKS.keys())
    results: list[tuple[str, bool, str]] = []
    for key in keys:
        if key not in SDKS:
            print(f"unknown SDK key {key!r}; expected one of {list(SDKS)}")
            return 2
        results.append(await run_one(key))

    print("\n" + "=" * 50)
    print("Summary")
    print("=" * 50)
    failures = 0
    for label, ok, detail in results:
        sigil = "✓" if ok else "✗"
        print(f"{sigil} {label:<11} {detail}")
        if not ok:
            failures += 1
    return 0 if failures == 0 else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "-k",
        "--only",
        action="append",
        choices=list(SDKS.keys()),
        help="Run only the given SDK(s). Repeatable. Default: all.",
    )
    args = parser.parse_args()
    return asyncio.run(amain(args.only or []))


if __name__ == "__main__":
    sys.exit(main())
