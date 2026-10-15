#!/usr/bin/env python3
"""Click a point on a niri output through a wlr virtual pointer bound to that output.

The virtual pointer cannot query the pointer position. The optional restore coordinates therefore
name the virtual-desktop position to move back to; they are not an automatic snapshot of the
position before the click.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from dataclasses import dataclass


BUTTONS = ("left", "right", "middle")
POINTER = ["cargo", "run", "-q", "-p", "glimpse-wayland", "--example", "click", "--"]


@dataclass(frozen=True)
class Geometry:
    x: int
    y: int
    width: int
    height: int

    @property
    def right(self) -> int:
        return self.x + self.width

    @property
    def bottom(self) -> int:
        return self.y + self.height


def parse_arguments(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Click a point on a niri output with a wlr virtual pointer.",
        epilog=(
            "KEY=VALUE arguments are accepted for use with `just click`; restore_x and restore_y "
            "are virtual-desktop coordinates, not an automatic pointer snapshot."
        ),
    )
    parser.add_argument("assignments", nargs="*", metavar="KEY=VALUE")
    parser.add_argument("--output")
    parser.add_argument("--x", type=int)
    parser.add_argument("--y", type=int)
    parser.add_argument("--button", choices=BUTTONS, default="left")
    parser.add_argument("--restore-x", type=int)
    parser.add_argument("--restore-y", type=int)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args(argv)

    values: dict[str, str] = {}
    for assignment in args.assignments:
        key, separator, value = assignment.partition("=")
        if not separator or not key or not value:
            parser.error(f"expected KEY=VALUE, got {assignment!r}")
        if key in values:
            parser.error(f"duplicate argument: {key}")
        values[key] = value

    def value(name: str, option: object) -> object:
        if name in values and option is not None:
            parser.error(f"{name} was provided more than once")
        return values.get(name, option)

    output = value("output", args.output)
    x = value("x", args.x)
    y = value("y", args.y)
    button = value("button", None if args.button == "left" else args.button)
    restore_x = value("restore_x", args.restore_x)
    restore_y = value("restore_y", args.restore_y)
    restore = values.get("restore")
    unknown = set(values) - {"output", "x", "y", "button", "restore", "restore_x", "restore_y"}
    if unknown:
        parser.error(f"unknown argument(s): {', '.join(sorted(unknown))}")
    if restore == "true":
        parser.error("automatic restore is unavailable; provide restore_x and restore_y explicitly")
    if restore not in (None, "false"):
        parser.error("restore must be false; use restore_x and restore_y for an explicit restore point")
    if output is None or x is None or y is None:
        parser.error("output, x, and y are required")
    if (restore_x is None) != (restore_y is None):
        parser.error("restore_x and restore_y must be provided together")

    def integer(name: str, raw: object) -> int | None:
        if raw is None:
            return None
        try:
            return int(raw)
        except (TypeError, ValueError):
            parser.error(f"{name} must be an integer")
            return None

    return argparse.Namespace(
        output=str(output),
        x=integer("x", x),
        y=integer("y", y),
        button=str(button or "left"),
        restore_x=integer("restore_x", restore_x),
        restore_y=integer("restore_y", restore_y),
        dry_run=args.dry_run,
    )


def output_geometries() -> dict[str, Geometry]:
    try:
        result = subprocess.run(
            ["niri", "msg", "-j", "outputs"],
            check=True,
            capture_output=True,
            text=True,
        )
        document = json.loads(result.stdout)
    except FileNotFoundError as error:
        raise RuntimeError("niri is not installed") from error
    except subprocess.CalledProcessError as error:
        detail = error.stderr.strip() or "niri did not return output information"
        raise RuntimeError(detail) from error
    except json.JSONDecodeError as error:
        raise RuntimeError("niri returned invalid JSON for its outputs") from error

    if not isinstance(document, dict):
        raise RuntimeError("niri returned an unexpected outputs document")
    geometries: dict[str, Geometry] = {}
    for name, record in document.items():
        if not isinstance(name, str) or not isinstance(record, dict):
            continue
        logical = record.get("logical")
        if not isinstance(logical, dict):
            continue
        try:
            geometry = Geometry(
                x=int(logical["x"]),
                y=int(logical["y"]),
                width=int(logical["width"]),
                height=int(logical["height"]),
            )
        except (KeyError, TypeError, ValueError):
            continue
        if geometry.width <= 0 or geometry.height <= 0:
            continue
        geometries[name] = geometry
    if not geometries:
        raise RuntimeError("niri reported no outputs with logical geometry")
    return geometries


def containing(x: int, y: int, geometries: dict[str, Geometry]) -> tuple[str, Geometry]:
    for name, geometry in geometries.items():
        if geometry.x <= x < geometry.right and geometry.y <= y < geometry.bottom:
            return name, geometry
    raise RuntimeError(f"point ({x}, {y}) is on no output")


def pointer(output: str, x: int, y: int, geometry: Geometry, action: str, dry_run: bool) -> None:
    command = [*POINTER, output, str(x), str(y), str(geometry.width), str(geometry.height), action]
    if dry_run:
        print(" ".join(command))
        return
    try:
        subprocess.run(command, check=True)
    except FileNotFoundError as error:
        raise RuntimeError("cargo is not installed") from error
    except subprocess.CalledProcessError as error:
        raise RuntimeError("the virtual pointer could not inject the event") from error


def run(args: argparse.Namespace) -> None:
    if shutil.which("niri") is None:
        raise RuntimeError("niri is not installed")

    geometries = output_geometries()
    try:
        output = geometries[args.output]
    except KeyError as error:
        available = ", ".join(sorted(geometries))
        raise RuntimeError(f"unknown output {args.output!r}; available outputs: {available}") from error
    if not 0 <= args.x < output.width or not 0 <= args.y < output.height:
        raise RuntimeError(
            f"point ({args.x}, {args.y}) is outside {args.output!r} ({output.width}x{output.height})"
        )

    pointer(args.output, args.x, args.y, output, args.button, args.dry_run)
    if args.restore_x is not None:
        name, geometry = containing(args.restore_x, args.restore_y, geometries)
        pointer(name, args.restore_x - geometry.x, args.restore_y - geometry.y, geometry, "move", args.dry_run)


def main() -> int:
    try:
        run(parse_arguments(sys.argv[1:]))
    except RuntimeError as error:
        print(f"click: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
