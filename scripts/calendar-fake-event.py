#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "dbus-next>=0.2.3",
# ]
# ///
"""Fake `org.gnome.Shell.CalendarServer` on the session bus to inject a
single test event that glimpse's calendar-events service will pick up.

Glimpse reads calendar events through the gnome-shell-calendar-server
D-Bus interface (see `glimpse-core/src/dbus/calendar.rs`). The earlier
file-write approach (mutating `~/.local/share/evolution/calendar/system/
calendar.ics`) doesn't help because EDS's in-memory state doesn't refresh.

This script registers itself as `org.gnome.Shell.CalendarServer` and
emits one `EventsAddedOrUpdated` signal containing a test event N
minutes in the future. It also tries to take
`org.gnome.evolution.dataserver.Sources5` so the event's source name
shows up properly; if EDS is running and owns that name, we fall back
without a source (the event still renders, source label is empty).

Usage:
    scripts/calendar-fake-event.py
    scripts/calendar-fake-event.py --in 2 --duration 15 --title "Lunch"
    scripts/calendar-fake-event.py --location "Room A"

Prerequisite: the real `gnome-shell-calendar-server` must NOT own the
bus name. Stop it with:
    systemctl --user stop org.gnome.Shell.CalendarServer.service
or:
    pkill -f gnome-shell-calendar-server
The script will print a clear error if it can't take the bus name.
"""

import argparse
import asyncio
import signal as _signal
import sys
import time
from datetime import datetime, timedelta

try:
    from dbus_next.aio import MessageBus
    from dbus_next.constants import (
        NameFlag,
        PropertyAccess,
        RequestNameReply,
    )
    from dbus_next.service import (
        ServiceInterface,
        dbus_property,
        method,
        signal,
    )
    from dbus_next.signature import Variant
except ImportError as exc:  # pragma: no cover
    sys.stderr.write("dbus-next is required. Re-run via `uv run`.\n")
    raise SystemExit(1) from exc

CALENDAR_SERVER_NAME = "org.gnome.Shell.CalendarServer"
CALENDAR_SERVER_PATH = "/org/gnome/Shell/CalendarServer"
SOURCE_MANAGER_NAME = "org.gnome.evolution.dataserver.Sources5"
SOURCE_MANAGER_PATH = "/org/gnome/evolution/dataserver/SourceManager"
SOURCE_PATH = "/org/gnome/evolution/dataserver/SourceManager/test_source"


class CalendarServerInterface(ServiceInterface):
    """Implements `org.gnome.Shell.CalendarServer` — `SetTimeRange` plus
    `EventsAddedOrUpdated` / `EventsRemoved` signals."""

    def __init__(self, event):
        super().__init__("org.gnome.Shell.CalendarServer")
        self._event = event  # (id, summary, start_epoch, end_epoch, meta_dict)

    @method()
    def SetTimeRange(self, since: "x", until: "x", force_reload: "b"):
        print(
            f"[CalendarServer] SetTimeRange(since={since}, until={until}, "
            f"force={force_reload}) → re-emitting event"
        )
        self.emit_event_now()

    def emit_event_now(self):
        # Signature `a(ssxxa{sv})` — array of (id, summary, start, end, meta).
        self.EventsAddedOrUpdated([self._event])

    @signal()
    def EventsAddedOrUpdated(self, events: "a(ssxxa{sv})") -> "a(ssxxa{sv})":
        return events

    @signal()
    def EventsRemoved(self, ids: "as") -> "as":
        return ids


class SourceInterface(ServiceInterface):
    """Implements `org.gnome.evolution.dataserver.Source` — minimal."""

    def __init__(self, display_name: str, color: str):
        super().__init__("org.gnome.evolution.dataserver.Source")
        # Glimpse parses the `Data` property as INI-style text for
        # display_name + color (see provider.rs `value_to_string` /
        # source parsing). The minimal INI we emit gives it what it needs.
        self._data = (
            "[Data Source]\n"
            f"DisplayName={display_name}\n"
            "[Calendar]\n"
            f"Color={color}\n"
        )

    @dbus_property(access=PropertyAccess.READ)
    def UID(self) -> "s":
        return "glimpsefake-source"

    @dbus_property(access=PropertyAccess.READ)
    def Data(self) -> "s":
        return self._data


class SourceManagerInterface(ServiceInterface):
    """Implements `org.freedesktop.DBus.ObjectManager.GetManagedObjects` so
    glimpse's `read_sources` finds one source named 'Test'."""

    def __init__(self, display_name: str, color: str):
        super().__init__("org.freedesktop.DBus.ObjectManager")
        self._display_name = display_name
        self._color = color

    @method()
    def GetManagedObjects(self) -> "a{oa{sa{sv}}}":
        # path → interface_name → property_name → Variant
        source_props = {
            "UID": Variant("s", "glimpsefake-source"),
            "Data": Variant(
                "s",
                "[Data Source]\n"
                f"DisplayName={self._display_name}\n"
                "[Calendar]\n"
                f"Color={self._color}\n",
            ),
        }
        return {
            SOURCE_PATH: {
                "org.gnome.evolution.dataserver.Source": source_props,
            }
        }


# ---------- main loop ------------------------------------------------------- #


async def run():
    sys.stdout.reconfigure(line_buffering=True)
    args = parse_args()

    if args.start_in < 0:
        sys.stderr.write("error: --in must be >= 0\n")
        sys.exit(2)
    if args.duration <= 0:
        sys.stderr.write("error: --duration must be > 0\n")
        sys.exit(2)

    now = datetime.now().astimezone()
    start = now + timedelta(minutes=args.start_in)
    end = start + timedelta(minutes=args.duration)
    event_id = f"glimpsefake-{int(time.time())}"
    meta = {}
    if args.location:
        meta["location"] = Variant("s", args.location)

    event_tuple = [
        event_id,
        args.title,
        int(start.timestamp()),
        int(end.timestamp()),
        meta,
    ]

    # Take the calendar-server name FIRST. This is the prerequisite for the
    # whole exercise — without it, glimpse won't see our signals.
    bus = await MessageBus().connect()
    reply = await bus.request_name(
        CALENDAR_SERVER_NAME, flags=NameFlag.DO_NOT_QUEUE
    )
    if reply != RequestNameReply.PRIMARY_OWNER:
        sys.stderr.write(
            f"error: could not take ownership of {CALENDAR_SERVER_NAME}\n"
            "Stop the real server first:\n"
            "    systemctl --user stop org.gnome.Shell.CalendarServer.service\n"
            "or:\n"
            "    pkill -f gnome-shell-calendar-server\n"
        )
        sys.exit(1)

    calendar_iface = CalendarServerInterface(event_tuple)
    bus.export(CALENDAR_SERVER_PATH, calendar_iface)
    print(f"acquired {CALENDAR_SERVER_NAME}")

    # Try to take the source manager too. If EDS owns it, we just skip —
    # the event still renders, source name will be empty.
    sources_taken = False
    source_reply = await bus.request_name(
        SOURCE_MANAGER_NAME, flags=NameFlag.DO_NOT_QUEUE
    )
    if source_reply == RequestNameReply.PRIMARY_OWNER:
        bus.export(
            SOURCE_MANAGER_PATH,
            SourceManagerInterface(args.source, args.color),
        )
        bus.export(
            SOURCE_PATH,
            SourceInterface(args.source, args.color),
        )
        sources_taken = True
        print(f"acquired {SOURCE_MANAGER_NAME}")
    else:
        print(
            f"note: {SOURCE_MANAGER_NAME} already owned (EDS running); "
            "events will render with no source label"
        )

    print()
    print(f"event:    \"{args.title}\"")
    print(f"  start:    {start.strftime('%Y-%m-%d %H:%M %Z')} (in {args.start_in}m)")
    print(f"  end:      {end.strftime('%H:%M')} ({args.duration}m duration)")
    print(f"  id:       {event_id}")
    if args.location:
        print(f"  location: {args.location}")
    if sources_taken:
        print(f"  source:   {args.source}")
    print()
    print("Open the glimpse popover that consumes calendar events.")
    print("Ctrl-C to remove the event and release the bus name.")

    # Initial emit so anyone subscribing post-startup sees it.
    calendar_iface.emit_event_now()

    stop = asyncio.Event()
    loop = asyncio.get_running_loop()
    for sig in (_signal.SIGINT, _signal.SIGTERM):
        loop.add_signal_handler(sig, stop.set)
    await stop.wait()

    print("\nremoving event and releasing bus names...")
    try:
        calendar_iface.EventsRemoved([event_id])
    except Exception as exc:  # noqa: BLE001
        print(f"  EventsRemoved emit: {exc}", file=sys.stderr)
    try:
        await bus.release_name(CALENDAR_SERVER_NAME)
    except Exception as exc:  # noqa: BLE001
        print(f"  release {CALENDAR_SERVER_NAME}: {exc}", file=sys.stderr)
    if sources_taken:
        try:
            await bus.release_name(SOURCE_MANAGER_NAME)
        except Exception as exc:  # noqa: BLE001
            print(f"  release {SOURCE_MANAGER_NAME}: {exc}", file=sys.stderr)
    bus.disconnect()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=__doc__.splitlines()[0],
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--in", dest="start_in", type=int, default=5,
        help="minutes from now when the event starts (default: 5)",
    )
    parser.add_argument(
        "--duration", type=int, default=30,
        help="event duration in minutes (default: 30)",
    )
    parser.add_argument(
        "--title", default="Test event",
        help="event summary (default: 'Test event')",
    )
    parser.add_argument(
        "--location", default=None,
        help="optional event location",
    )
    parser.add_argument(
        "--source", default="Test",
        help="calendar source display name (only used if EDS is not running)",
    )
    parser.add_argument(
        "--color", default="#3584e4",
        help="calendar source color (only used if EDS is not running)",
    )
    return parser.parse_args()


if __name__ == "__main__":
    try:
        asyncio.run(run())
    except KeyboardInterrupt:
        pass
