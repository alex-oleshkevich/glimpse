#!/usr/bin/env python3
"""Write or remove a local test calendar event for Glimpse.

The event is written as an iCalendar file so it exercises the same calendar
source path as configured local directories. This is useful for checking the
`next_event` applet without depending on Google, Outlook, or another remote provider.

Usage:
    scripts/calendar-fake-event.py
    scripts/calendar-fake-event.py --in 1 --duration 10 --title "Call"
    scripts/calendar-fake-event.py clear
"""

import argparse
import os
import sys
import time
from datetime import datetime, timedelta, timezone
from pathlib import Path

DEFAULT_DIR = Path.home() / ".config" / "glimpse" / "calendars" / "test-events"
DEFAULT_FILE = "next-event.ics"
SOURCE_ID = "local-test-events"


def main() -> int:
    args = parse_args()
    target_dir = args.dir.expanduser().resolve()
    target_path = target_dir / DEFAULT_FILE
    config_path = args.config.expanduser() if args.config else default_config_path()

    if args.command == "clear":
        return clear_event(target_path)

    if args.start_in < 0:
        sys.stderr.write("error: --in must be >= 0\n")
        return 2
    if args.duration <= 0:
        sys.stderr.write("error: --duration must be > 0\n")
        return 2

    now = datetime.now(timezone.utc)
    start = now + timedelta(minutes=args.start_in)
    end = start + timedelta(minutes=args.duration)
    event_id = f"glimpse-test-next-event-{int(time.time())}"

    target_dir.mkdir(parents=True, exist_ok=True)
    target_path.write_text(
        build_ics(
            event_id=event_id,
            title=args.title,
            start=start,
            end=end,
            location=args.location,
        ),
        encoding="utf-8",
    )

    print(f"wrote:    {target_path}")
    print(f"event:    {args.title}")
    print(
        f"start:    {start.astimezone().strftime('%Y-%m-%d %H:%M %Z')} "
        f"(in {args.start_in}m)"
    )
    print(
        f"end:      {end.astimezone().strftime('%H:%M %Z')} "
        f"({args.duration}m duration)"
    )
    print(f"id:       {event_id}")
    if args.location:
        print(f"location: {args.location}")
    print_config_hint(config_path, target_dir)
    print("calendar polling may take up to the configured poll interval.")
    return 0


def clear_event(target_path: Path) -> int:
    try:
        target_path.unlink()
    except FileNotFoundError:
        print(f"no test event at {target_path}")
    else:
        print(f"removed: {target_path}")
    return 0


def build_ics(
    *,
    event_id: str,
    title: str,
    start: datetime,
    end: datetime,
    location: str | None,
) -> str:
    lines = [
        "BEGIN:VCALENDAR",
        "VERSION:2.0",
        "PRODID:-//Glimpse//Next Event Test//EN",
        "BEGIN:VEVENT",
        f"UID:{escape_text(event_id)}",
        f"DTSTAMP:{format_utc(datetime.now(timezone.utc))}",
        f"SUMMARY:{escape_text(title)}",
        f"DTSTART:{format_utc(start)}",
        f"DTEND:{format_utc(end)}",
    ]
    if location:
        lines.append(f"LOCATION:{escape_text(location)}")
    lines.extend(["END:VEVENT", "END:VCALENDAR", ""])
    return "\n".join(lines)


def format_utc(value: datetime) -> str:
    return value.astimezone(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def escape_text(value: str) -> str:
    return (
        value.replace("\\", "\\\\")
        .replace("\n", "\\n")
        .replace(",", "\\,")
        .replace(";", "\\;")
    )


def default_config_path() -> Path:
    if config_path := os.environ.get("GLIMPSE_CONFIG"):
        return Path(config_path).expanduser()

    local_config = Path("config.toml")
    if local_config.exists():
        return local_config

    if xdg_config_home := os.environ.get("XDG_CONFIG_HOME"):
        return Path(xdg_config_home).expanduser() / "glimpse" / "config.toml"

    return Path.home() / ".config" / "glimpse" / "config.toml"


def print_config_hint(config_path: Path, target_dir: Path) -> None:
    uri = f"file://{target_dir}"
    if config_path.exists():
        text = config_path.read_text(encoding="utf-8")
        if SOURCE_ID in text or uri in text:
            return

    print()
    print("Add this calendar source if it is not already configured:")
    print()
    print("[[calendar.sources]]")
    print(f'id = "{SOURCE_ID}"')
    print('type = "directory"')
    print('name = "Local Test Events"')
    print(f'uri = "{uri}"')
    print('color = "#f6c343"')
    print("poll_interval = 60")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=__doc__.splitlines()[0],
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "command",
        nargs="?",
        choices=["write", "clear"],
        default="write",
        help="write a test event or clear the generated event (default: write)",
    )
    parser.add_argument(
        "--in",
        dest="start_in",
        type=int,
        default=5,
        help="minutes from now when the event starts (default: 5)",
    )
    parser.add_argument(
        "--duration",
        type=int,
        default=30,
        help="event duration in minutes (default: 30)",
    )
    parser.add_argument(
        "--title",
        default="Next Event Test",
        help="event summary (default: 'Next Event Test')",
    )
    parser.add_argument(
        "--location",
        default=None,
        help="optional event location",
    )
    parser.add_argument(
        "--dir",
        type=Path,
        default=DEFAULT_DIR,
        help=f"directory to write the .ics file into (default: {DEFAULT_DIR})",
    )
    parser.add_argument(
        "--config",
        type=Path,
        default=None,
        help="config file to inspect for source hint (default: Glimpse config discovery order)",
    )
    return parser.parse_args()


if __name__ == "__main__":
    raise SystemExit(main())
