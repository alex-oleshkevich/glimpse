#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "dbus-next>=0.2.3",
#     "Pillow>=10",
# ]
# ///
"""Spawn three fake MPRIS players on the session bus for popover testing.

Each player advertises a different identity, long artist/title text (to
exercise label ellipsization), an artwork PNG generated to /tmp, and a
different playback status. Run, open the glimpse mpris popover, then
Ctrl-C to tear everything down.

Implements just enough of org.mpris.MediaPlayer2 + .Player for
glimpse-core's parser: Identity / CanRaise / PlaybackStatus / Metadata /
Position / CanGo{Previous,Next} / CanPlay / CanPause / CanSeek. Transport
methods (PlayPause, Next, Previous, Seek, SetPosition) update local
state and emit the corresponding signals so the popover sees the change
land — but no audio is produced.
"""

import asyncio
import signal as _signal
import sys
from dataclasses import dataclass
from pathlib import Path

try:
    from dbus_next.aio import MessageBus
    from dbus_next.constants import PropertyAccess
    from dbus_next.service import (
        ServiceInterface,
        dbus_property,
        method,
        signal,
    )
    from dbus_next.signature import Variant
except ImportError as exc:  # pragma: no cover
    sys.stderr.write(
        "dbus-next is required. Re-run with `uv run` or install it:\n"
        "    uv run scripts/mpris-fake-players.py\n"
    )
    raise SystemExit(1) from exc

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError as exc:  # pragma: no cover
    sys.stderr.write("Pillow is required. Re-run via `uv run`.\n")
    raise SystemExit(1) from exc


@dataclass
class FakePlayer:
    bus_suffix: str          # appended after "org.mpris.MediaPlayer2."
    identity: str
    artist: str
    title: str
    album: str
    art_color: tuple[int, int, int]
    length_us: int
    position_us: int
    status: str              # "Playing" | "Paused"


PLAYERS: list[FakePlayer] = [
    FakePlayer(
        bus_suffix="glimpsefake.spotify",
        identity="Spotify (Fake)",
        artist="Florence + The Machine feat. The Mountain Goats",
        title="Dog Days Are Over (Extended Live Remix)",
        album="Lungs — 15th Anniversary Deluxe Edition",
        art_color=(36, 178, 89),
        length_us=246_000_000,
        position_us=72_000_000,
        status="Playing",
    ),
    FakePlayer(
        bus_suffix="glimpsefake.firefox",
        identity="Firefox (Fake)",
        artist="Khruangbin & Leon Bridges",
        title="Texas Sun",
        album="Texas Sun EP",
        art_color=(220, 90, 30),
        length_us=312_000_000,
        position_us=180_000_000,
        status="Paused",
    ),
    FakePlayer(
        bus_suffix="glimpsefake.mpv",
        identity="mpv (Fake)",
        artist="Jinjer",
        title="Pisces",
        album="King of Everything",
        art_color=(70, 90, 200),
        length_us=435_000_000,
        position_us=10_000_000,
        status="Paused",
    ),
]


# ---------- artwork --------------------------------------------------------- #


def make_artwork(path: Path, color: tuple[int, int, int], label_text: str) -> None:
    """Write a 256x256 solid-color PNG with bold initials centered."""
    img = Image.new("RGB", (256, 256), color)
    draw = ImageDraw.Draw(img)
    initials = "".join(word[0].upper() for word in label_text.split()[:2]) or "?"
    font = _load_font(96)
    bbox = draw.textbbox((0, 0), initials, font=font)
    w, h = bbox[2] - bbox[0], bbox[3] - bbox[1]
    draw.text(
        ((256 - w) / 2 - bbox[0], (256 - h) / 2 - bbox[1]),
        initials,
        fill="white",
        font=font,
    )
    img.save(path, "PNG")


def _load_font(size: int) -> ImageFont.ImageFont:
    candidates = [
        "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/noto/NotoSans-Bold.ttf",
    ]
    for path in candidates:
        try:
            return ImageFont.truetype(path, size)
        except OSError:
            continue
    return ImageFont.load_default()


# ---------- D-Bus interfaces ----------------------------------------------- #


class RootInterface(ServiceInterface):
    def __init__(self, identity):
        super().__init__("org.mpris.MediaPlayer2")
        self._identity = identity

    @method()
    def Raise(self):
        print(f"[{self._identity}] Raise()")

    @method()
    def Quit(self):
        pass

    @dbus_property(access=PropertyAccess.READ)
    def CanQuit(self) -> "b":
        return False

    @dbus_property(access=PropertyAccess.READ)
    def CanRaise(self) -> "b":
        return True

    @dbus_property(access=PropertyAccess.READ)
    def HasTrackList(self) -> "b":
        return False

    @dbus_property(access=PropertyAccess.READ)
    def Identity(self) -> "s":
        return self._identity

    @dbus_property(access=PropertyAccess.READ)
    def DesktopEntry(self) -> "s":
        return ""

    @dbus_property(access=PropertyAccess.READ)
    def SupportedUriSchemes(self) -> "as":
        return []

    @dbus_property(access=PropertyAccess.READ)
    def SupportedMimeTypes(self) -> "as":
        return []


class PlayerInterface(ServiceInterface):
    def __init__(self, fake, art_uri):
        super().__init__("org.mpris.MediaPlayer2.Player")
        self._fake = fake
        self._art_uri = art_uri
        self._track_id = (
            "/org/mpris/MediaPlayer2/Track/"
            + fake.bus_suffix.replace(".", "_")
        )

    # --- methods --- #
    @method()
    def PlayPause(self):
        self._fake.status = (
            "Paused" if self._fake.status == "Playing" else "Playing"
        )
        print(f"[{self._fake.identity}] PlayPause → {self._fake.status}")
        self.emit_properties_changed({"PlaybackStatus": self._fake.status})

    @method()
    def Play(self):
        if self._fake.status != "Playing":
            self._fake.status = "Playing"
            self.emit_properties_changed({"PlaybackStatus": self._fake.status})

    @method()
    def Pause(self):
        if self._fake.status != "Paused":
            self._fake.status = "Paused"
            self.emit_properties_changed({"PlaybackStatus": self._fake.status})

    @method()
    def Stop(self):
        self._fake.status = "Stopped"
        self.emit_properties_changed({"PlaybackStatus": self._fake.status})

    @method()
    def Next(self):
        print(f"[{self._fake.identity}] Next()")

    @method()
    def Previous(self):
        print(f"[{self._fake.identity}] Previous()")

    @method()
    def Seek(self, offset: "x"):
        new_pos = self._fake.position_us + offset
        self._fake.position_us = max(0, min(self._fake.length_us, new_pos))
        print(
            f"[{self._fake.identity}] Seek({offset:+}us) "
            f"→ position={self._fake.position_us}us"
        )
        self.Seeked(self._fake.position_us)

    @method()
    def SetPosition(self, track_id: "o", position: "x"):
        self._fake.position_us = max(0, min(self._fake.length_us, position))
        self.Seeked(self._fake.position_us)

    @signal()
    def Seeked(self, position: "x") -> "x":
        return position

    # --- properties --- #
    @dbus_property(access=PropertyAccess.READ)
    def PlaybackStatus(self) -> "s":
        return self._fake.status

    @dbus_property(access=PropertyAccess.READ)
    def Rate(self) -> "d":
        return 1.0

    @dbus_property(access=PropertyAccess.READ)
    def MinimumRate(self) -> "d":
        return 1.0

    @dbus_property(access=PropertyAccess.READ)
    def MaximumRate(self) -> "d":
        return 1.0

    @dbus_property(access=PropertyAccess.READ)
    def Metadata(self) -> "a{sv}":
        return {
            "mpris:trackid": Variant("o", self._track_id),
            "mpris:length": Variant("x", self._fake.length_us),
            "mpris:artUrl": Variant("s", self._art_uri),
            "xesam:title": Variant("s", self._fake.title),
            "xesam:artist": Variant("as", [self._fake.artist]),
            "xesam:album": Variant("s", self._fake.album),
        }

    @dbus_property(access=PropertyAccess.READ)
    def Position(self) -> "x":
        return self._fake.position_us

    @dbus_property(access=PropertyAccess.READ)
    def CanGoNext(self) -> "b":
        return True

    @dbus_property(access=PropertyAccess.READ)
    def CanGoPrevious(self) -> "b":
        return True

    @dbus_property(access=PropertyAccess.READ)
    def CanPlay(self) -> "b":
        return True

    @dbus_property(access=PropertyAccess.READ)
    def CanPause(self) -> "b":
        return True

    @dbus_property(access=PropertyAccess.READ)
    def CanSeek(self) -> "b":
        return True

    @dbus_property(access=PropertyAccess.READ)
    def CanControl(self) -> "b":
        return True

    # Optional properties — exposed so playerctld and similar daemons
    # don't spam UNKNOWN_PROPERTY errors when they probe.
    @dbus_property(access=PropertyAccess.READ)
    def LoopStatus(self) -> "s":
        return "None"

    @dbus_property(access=PropertyAccess.READ)
    def Shuffle(self) -> "b":
        return False

    @dbus_property(access=PropertyAccess.READ)
    def Volume(self) -> "d":
        return 1.0


# ---------- main loop ------------------------------------------------------- #


async def run():
    sys.stdout.reconfigure(line_buffering=True)
    art_dir = Path("/tmp/glimpse-mpris-fake")
    art_dir.mkdir(exist_ok=True)

    # MPRIS players must each own a distinct bus name AND export the
    # MPRIS interfaces on /org/mpris/MediaPlayer2. dbus-next disallows
    # double-exporting the same interface per connection, so each fake
    # player gets its own MessageBus.
    buses = []
    for fake in PLAYERS:
        art_path = art_dir / f"{fake.bus_suffix}.png"
        make_artwork(art_path, fake.art_color, fake.artist)
        art_uri = art_path.as_uri()

        bus = await MessageBus().connect()
        bus.export("/org/mpris/MediaPlayer2", RootInterface(fake.identity))
        bus.export("/org/mpris/MediaPlayer2", PlayerInterface(fake, art_uri))

        well_known = f"org.mpris.MediaPlayer2.{fake.bus_suffix}"
        await bus.request_name(well_known)
        buses.append((bus, well_known))
        print(
            f"registered {well_known}\n"
            f"  identity: {fake.identity}\n"
            f"  track:    {fake.artist} — {fake.title}\n"
            f"  album:    {fake.album}\n"
            f"  artwork:  {art_path}\n"
            f"  status:   {fake.status}  "
            f"({fake.position_us // 1_000_000}s / {fake.length_us // 1_000_000}s)\n"
        )

    print("Ready. Open the glimpse mpris popover. Ctrl-C to quit.\n")

    stop = asyncio.Event()
    loop = asyncio.get_running_loop()
    for sig in (_signal.SIGINT, _signal.SIGTERM):
        loop.add_signal_handler(sig, stop.set)
    await stop.wait()

    print("\nshutting down...")
    for bus, name in buses:
        try:
            await bus.release_name(name)
        except Exception as exc:  # noqa: BLE001
            print(f"  release {name}: {exc}", file=sys.stderr)
        bus.disconnect()


if __name__ == "__main__":
    try:
        asyncio.run(run())
    except KeyboardInterrupt:
        pass
