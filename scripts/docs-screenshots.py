#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "python-dbusmock==0.38.1",
#     "PyGObject",
#     "Pillow>=10",
# ]
# ///
"""Capture the documentation screenshots inside a sealed sandbox.

Everything shown is made up here: a nested niri, private session and system buses, a scratch home,
mocked NetworkManager, BlueZ, UPower, power-profiles, logind and UDisks2, a private PipeWire with
null devices, the repository's fake MPRIS players, fake tray and mock CUPS, and example calendar
events and notifications. Nothing from the running session can reach a capture.

Usage (through the justfile):
    just docs-screenshots
    just docs-video
    just docs-video --dark
"""

import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import wave
from datetime import datetime, timedelta
from pathlib import Path

import dbus
import dbus.bus
from PIL import Image, ImageChops, ImageDraw, ImageFilter

REPO = Path(__file__).resolve().parent.parent
BIN = REPO / "target" / "debug"
SCRIPTS = REPO / "scripts"
OUT = REPO / "website" / "public" / "screenshots"
VIDEO_OUT = REPO / "website" / "public" / "video"

APPLETS = [
    "audio", "bluetooth", "network", "display", "power", "keyboard", "printing", "removable",
    "privacy", "idle", "session", "pager", "workspace-name", "places", "clock",
    "next-event", "weather", "mpris", "notifications", "system-monitor", "clipboard",
    "color-picker", "ruler",
]
COLLAGES = {
    "collage": ["clock", "weather", "bluetooth", "network"],
    "everyday": ["audio", "display", "mpris", "notifications"],
}
COLLAGE_LAYOUT = {
    "position": "top",
    "left": ["workspace-name", "mpris"],
    "center": ["clock", "weather", "notifications"],
    "right": ["printing", "bluetooth", "network", "display", "audio", "idle",
              "power", "session"],
}
SCALE = 1.25
WALLPAPER = REPO / "data" / "themes" / "adwaita" / "default.jpg"
FUZZ = 8
FRAME_PADDING = 40
FRAME_RADIUS = 22
PANEL_HEIGHT = 36
WPA_PSK = 0x100
PADDING = 24
SCREENSHOT_WINDOW = (1800, 1300)
VIDEO_WINDOW = (1536, 864)
VIDEO_SIZE = (1920, 1080)
VIDEO_LENGTH = 30.0
VIDEO_FADE = 0.5
VIDEO_SCALE = 4 / 3
VIDEO_LEAD = 3.0
VIDEO_CUE = 3.0
VIDEO_SPEED = 1.5
VIDEO_GLIDE = 0.5
PANEL_X = {"clock": 884, "weather": 1030, "notifications": 1092, "bluetooth": 1596,
           "network": 1644, "audio": 1740, "mpris": 308}
DARK = "--dark" in sys.argv[1:]
VIDEO = "--video" in sys.argv[1:]
SUFFIX = "-dark" if DARK else ""
MODES = ("dark", "light") if DARK else ("light", "dark")
SHOT_MARKS = [
    {"kind": "blur", "x": 66, "y": 4, "width": 330, "height": 28},
    {"kind": "oval", "x": 1066, "y": 58, "width": 100, "height": 54, "color": "red", "stroke": 3},
    {"kind": "arrow", "from": [1330, 230], "to": [1180, 100], "color": "red", "stroke": 3},
    {"kind": "text", "x": 1300, "y": 240, "text": "Warm tonight", "color": "red", "stroke": 2},
    {"kind": "rect", "x": 752, "y": 236, "width": 396, "height": 42, "color": "yellow",
     "stroke": 2},
    {"kind": "number", "x": 716, "y": 257, "color": "blue", "stroke": 2},
]

children: list[subprocess.Popen] = []


def spawn(argv, env, log=None, **kwargs):
    out = open(log, "w") if log else subprocess.DEVNULL
    process = subprocess.Popen(
        [str(part) for part in argv],
        env=env,
        stdout=out,
        stderr=subprocess.STDOUT,
        stdin=subprocess.DEVNULL,
        start_new_session=True,
        **kwargs,
    )
    children.append(process)
    return process


def stop_all():
    for process in reversed(children):
        if process.poll() is None:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
    deadline = time.time() + 5
    for process in children:
        try:
            process.wait(max(0.1, deadline - time.time()))
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)


def wait_for(predicate, what, timeout=20.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            if predicate():
                return
        except Exception:
            pass
        time.sleep(0.2)
    raise SystemExit(f"timed out waiting for {what}")


def bus_daemon(address_file: Path, env) -> str:
    config = address_file.with_suffix(".conf")
    config.write_text(
        "<!DOCTYPE busconfig PUBLIC \"-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN\"\n"
        " \"http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd\">\n"
        "<busconfig>\n"
        "  <type>session</type>\n"
        f"  <listen>unix:path={address_file}</listen>\n"
        "  <auth>EXTERNAL</auth>\n"
        "  <policy context=\"default\">\n"
        "    <allow send_destination=\"*\" eavesdrop=\"true\"/>\n"
        "    <allow eavesdrop=\"true\"/>\n"
        "    <allow own=\"*\"/>\n"
        "  </policy>\n"
        "</busconfig>\n"
    )
    process = spawn(
        ["dbus-daemon", f"--config-file={config}", "--nofork", "--print-address=1"],
        env,
    )
    wait_for(address_file.exists, f"bus at {address_file}")
    del process
    return f"unix:path={address_file}"


def owned(address: str, name: str) -> bool:
    return dbus.bus.BusConnection(address).name_has_owner(name)


def mock(address: str, template: str, env, name: str, parameters=None):
    argv = [sys.executable, "-m", "dbusmock", "--system", "--template", template]
    if parameters:
        argv += ["--parameters", json.dumps(parameters)]
    log = Path(env["XDG_RUNTIME_DIR"]).parent / f"mock-{Path(template).stem}.log"
    spawn(argv, env, log)
    wait_for(lambda: owned(address, name), f"mock {name}")
    return dbus.bus.BusConnection(address)


def mock_object(connection, name, path):
    return dbus.Interface(connection.get_object(name, path), "org.freedesktop.DBus.Mock")


def system_services(address: str, env, scratch: Path):
    nm = mock(address, "networkmanager", env, name="org.freedesktop.NetworkManager")
    manager = mock_object(nm, "org.freedesktop.NetworkManager", "/org/freedesktop/NetworkManager")
    wifi = manager.AddWiFiDevice("wlan0", "wlan0", 100)
    points = [
        ("Lighthouse", 92, WPA_PSK),
        ("Lighthouse 5G", 78, WPA_PSK),
        ("Café Aurora", 64, 0),
        ("Harbour Library", 41, WPA_PSK),
        ("Studio 4B", 33, WPA_PSK),
    ]
    paths = []
    for index, (ssid, strength, security) in enumerate(points):
        paths.append(manager.AddAccessPoint(
            wifi, f"ap{index}", ssid, f"00:11:22:33:44:{index:02x}", 2, 5180, 54000, strength,
            security,
        ))
    connection = manager.AddWiFiConnection(wifi, "Lighthouse", "Lighthouse", "wpa-psk")
    manager.AddActiveConnection([wifi], connection, paths[0], "Lighthouse", 2)
    mock_object(nm, "org.freedesktop.NetworkManager", wifi).AddProperty(
        "org.freedesktop.NetworkManager.Device.Wireless", "ActiveAccessPoint",
        dbus.ObjectPath(paths[0]))
    manager.SetGlobalConnectionState(70)
    manager.SetConnectivity(4)

    bluez = mock(address, "bluez5", env, name="org.bluez")
    adapters = dbus.Interface(bluez.get_object("org.bluez", "/"), "org.bluez.Mock")
    adapters.AddAdapter("hci0", "Laptop")
    for address_, alias, icon, connect in [
        ("11:22:33:44:55:01", "AirPods Pro", "audio-headphones", True),
        ("11:22:33:44:55:02", "Keychron K2", "input-keyboard", True),
        ("11:22:33:44:55:03", "Logitech MX Anywhere 3", "input-mouse", False),
    ]:
        device = adapters.AddDevice("hci0", address_, alias)
        adapters.PairDevice("hci0", address_)
        target = mock_object(bluez, "org.bluez", device)
        target.UpdateProperties(
            "org.bluez.Device1", {"Icon": icon, "Class": dbus.UInt32(0), "Trusted": True}
        )
        target.AddProperty("org.bluez.Device1", "Bonded", True)
        if connect:
            dbus.Interface(bluez.get_object("org.bluez", device), "org.bluez.Device1").Connect()

    upower = mock(address, "upower", env, name="org.freedesktop.UPower")
    power = mock_object(upower, "org.freedesktop.UPower", "/org/freedesktop/UPower")
    power.AddDischargingBattery("BAT0", "Laptop battery", 76.0, 3 * 3600 + 20 * 60)

    mock(address, "power_profiles_daemon", env, name="net.hadess.PowerProfiles")

    logind = mock(address, "logind", env, name="org.freedesktop.login1")
    login = mock_object(logind, "org.freedesktop.login1", "/org/freedesktop/login1")
    login.AddSeat("seat0")
    login.AddUser(1000, "sam", True)
    session = login.AddSession("c1", "seat0", 1000, "sam", True)
    mock_object(logind, "org.freedesktop.login1", session).UpdateProperties(
        "org.freedesktop.login1.Session",
        {"Timestamp": dbus.UInt64(int((time.time() - 2 * 3600 - 17 * 60) * 1_000_000))},
    )
    login.AddMethod(
        "org.freedesktop.login1.Manager", "GetSessionByPID", "u", "o", f'ret = "{session}"'
    )

    media = scratch / "media"
    for name in ["TRAVEL", "Backup"]:
        (media / name).mkdir(parents=True, exist_ok=True)
    mock(
        address,
        str(SCRIPTS / "docs_udisks2_mock.py"),
        env,
        parameters={"drives": [
            {"name": "sdb", "vendor": "SanDisk", "model": "Ultra Fit", "size": 64_000_000_000,
             "label": "TRAVEL", "mount": str(media / "TRAVEL")},
            {"name": "sdc", "vendor": "Samsung", "model": "Portable SSD T7",
             "size": 1_000_000_000_000, "label": "Backup", "mount": str(media / "Backup")},
        ]},
        name="org.freedesktop.UDisks2",
    )


def silence(path: Path, seconds=600):
    with wave.open(str(path), "wb") as out:
        out.setnchannels(2)
        out.setsampwidth(2)
        out.setframerate(48000)
        out.writeframes(b"\0" * 48000 * 4 * seconds)


def audio(env, scratch: Path):
    spawn(["pipewire"], env, scratch / "pipewire.log")
    wait_for(lambda: (Path(env["XDG_RUNTIME_DIR"]) / "pipewire-0").exists(), "pipewire")
    spawn(["pipewire-pulse"], env, scratch / "pipewire-pulse.log")
    wireplumber = Path(env["XDG_CONFIG_HOME"]) / "wireplumber" / "wireplumber.conf.d"
    wireplumber.mkdir(parents=True, exist_ok=True)
    (wireplumber / "10-docs.conf").write_text(
        "wireplumber.profiles = {\n  main = {\n"
        + "".join(f"    monitor.{name} = disabled\n"
                  for name in ["alsa", "alsa-midi", "bluez", "bluez-midi", "v4l2", "libcamera"])
        + "  }\n}\n"
    )
    spawn(["wireplumber"], env, scratch / "wireplumber.log")
    wait_for(lambda: subprocess.run(["pactl", "info"], env=env, capture_output=True).returncode == 0,
             "pipewire-pulse")
    for name, description, kind in [
        ("speakers", "Built-in Speakers", "Audio/Sink"),
        ("headphones", "AirPods Pro", "Audio/Sink"),
        ("microphone", "Built-in Microphone", "Audio/Source/Virtual"),
    ]:
        subprocess.run(
            ["pw-cli", "create-node", "adapter",
             "{ factory.name=support.null-audio-sink node.name=" + name
             + f' node.description="{description}" media.class={kind}'
             + " audio.position=[ FL FR ] object.linger=true }"],
            env=env, check=True, capture_output=True,
        )
    def listed(kind, name):
        out = subprocess.run(["pactl", "list", "short", kind], env=env, capture_output=True,
                             text=True).stdout
        return any(line.split("\t")[1:2] == [name] for line in out.splitlines())

    wait_for(lambda: listed("sinks", "speakers") and listed("sinks", "headphones")
             and listed("sources", "microphone"), "audio devices")
    subprocess.run(["pactl", "set-default-sink", "headphones"], env=env, capture_output=True)
    subprocess.run(["pactl", "set-default-source", "microphone"], env=env, capture_output=True)
    subprocess.run(["pactl", "set-sink-volume", "headphones", "62%"], env=env, capture_output=True)
    subprocess.run(["pactl", "set-source-volume", "microphone", "80%"], env=env,
                   capture_output=True)
    subprocess.run(["pactl", "set-sink-volume", "speakers", "40%"], env=env, capture_output=True)
    track = scratch / "silence.wav"
    silence(track)
    app_icons(env)
    for app, icon in [("Firefox", "firefox"), ("Spotify", "spotify")]:
        props = json.dumps({"application.name": app, "application.icon-name": icon,
                            "media.name": app})
        spawn(["pw-play", "--target", "headphones", "-P", props, track], env)
    record = json.dumps({"application.name": "Zoom", "application.icon-name": "zoom",
                         "media.name": "Zoom"})
    spawn(["pw-record", "--target", "microphone", "-P", record, scratch / "record.wav"], env)


def app_icons(env):
    folder = Path(env["XDG_DATA_HOME"]) / "icons" / "hicolor" / "128x128" / "apps"
    folder.mkdir(parents=True, exist_ok=True)
    for name, back, front in [("firefox", (255, 113, 57), (255, 205, 60)),
                              ("spotify", (30, 215, 96), (20, 40, 28)),
                              ("zoom", (45, 140, 255), (255, 255, 255)),
                              ("org.gnome.clocks", (80, 110, 200), (255, 255, 255))]:
        icon = Image.new("RGBA", (128, 128), (0, 0, 0, 0))
        draw = ImageDraw.Draw(icon)
        draw.rounded_rectangle([8, 8, 119, 119], radius=28, fill=back)
        if name == "firefox":
            draw.ellipse([30, 30, 97, 97], fill=front)
            draw.ellipse([52, 24, 108, 80], fill=back)
        elif name == "spotify":
            for top, width in [(40, 70), (60, 56), (80, 42)]:
                draw.arc([64 - width, top, 64 + width, top + 60], 200, 340, fill=front, width=9)
        elif name == "org.gnome.clocks":
            draw.ellipse([28, 28, 99, 99], outline=front, width=8)
            draw.line([(64, 64), (64, 42)], fill=front, width=7)
            draw.line([(64, 64), (80, 72)], fill=front, width=7)
        else:
            draw.rounded_rectangle([26, 42, 78, 86], radius=10, fill=front)
            draw.polygon([(84, 56), (104, 44), (104, 84), (84, 72)], fill=front)
        icon.save(folder / f"{name}.png")


def parent_windows(pid: int) -> list[dict]:
    out = subprocess.run(["niri", "msg", "-j", "windows"], capture_output=True, text=True)
    if out.returncode != 0:
        return []
    return [window for window in json.loads(out.stdout) if window.get("pid") == pid]


def niri_config(path: Path, scale: float):
    path.write_text("""
input {
    keyboard {
        xkb {
            layout "us,de,fr"
        }
    }
}
output "winit" {
    scale %SCALE%
}
%CURSOR%prefer-no-csd
hotkey-overlay {
    skip-at-startup
}
screenshot-path null
animations {
    off
}
layout {
    gaps 16
    focus-ring {
        off
    }
    border {
        off
    }
}
workspace "browse"
workspace "write"
workspace "music"
layer-rule {
    match namespace="^glimpse-popover$"
    match namespace="^glimpse-notifications$"
    match namespace="^glimpse-panel$"
    background-effect {
        xray false
    }
}
""".replace("%SCALE%", str(scale)).replace(
        "%CURSOR%", "" if VIDEO else "cursor {\n    hide-after-inactive-ms 100\n}\n"))


def nested(env, scratch: Path, size: tuple[int, int], scale: float):
    config = scratch / "niri.kdl"
    niri_config(config, scale)
    checked = subprocess.run(["niri", "validate", "-c", str(config)], capture_output=True, text=True)
    if checked.returncode != 0:
        raise SystemExit(f"the nested niri config does not parse:\n{checked.stderr}")
    marker = scratch / "nested.env"
    compositor = spawn(["niri", "-c", config, "--", "sh", "-c",
                        f"env > {marker}.tmp && mv {marker}.tmp {marker} && exec sleep infinity"],
                       env, scratch / "niri.log")
    wait_for(marker.exists, "the nested niri")
    wait_for(lambda: parent_windows(compositor.pid), "the nested niri window")
    for window in parent_windows(compositor.pid):
        identifier = str(window["id"])
        subprocess.run(["niri", "msg", "action", "move-window-to-workspace", "--window-id",
                        identifier, "--focus", "false", "glimpse"], capture_output=True)
        if not window.get("is_floating"):
            subprocess.run(["niri", "msg", "action", "toggle-window-floating", "--id", identifier],
                           capture_output=True)
        for action in ["set-window-width", "set-window-height"]:
            value = size[0] if action == "set-window-width" else size[1]
            subprocess.run(["niri", "msg", "action", action, "--id", identifier, str(value)],
                           capture_output=True)
    values = dict(line.split("=", 1) for line in marker.read_text().splitlines() if "=" in line)
    time.sleep(1.5)
    return values["WAYLAND_DISPLAY"], values["NIRI_SOCKET"]


def calendar(scratch: Path, env):
    work = scratch / "calendars" / "work"
    personal = scratch / "calendars" / "personal"
    now = datetime.now()
    tomorrow = (now + timedelta(days=1)).replace(hour=10, minute=0, second=0, microsecond=0)
    events = [
        (work, "review.ics", "Design review", 25, 45, "Room 4"),
        (work, "standup.ics", "Team standup", 26 * 60, 15, None),
        (work, "planning.ics", "Sprint planning", int((tomorrow - now).total_seconds() // 60), 60,
         "Main hall"),
        (personal, "lunch.ics", "Lunch with Maya", 150, 60, "Café Aurora"),
        (personal, "gym.ics", "Climbing", 7 * 60, 90, "Boulder Hall"),
        (personal, "flight.ics", "Flight to Lisbon", 3 * 24 * 60 + 120, 150, "Terminal 2"),
    ]
    for directory, name, title, start_in, duration, location in events:
        argv = [sys.executable, SCRIPTS / "calendar-fake-event.py", "--dir", directory, "--name",
                name, "--in", str(start_in), "--duration", str(duration), "--title", title,
                "--config", scratch / "none.toml"]
        if location:
            argv += ["--location", location]
        subprocess.run([str(part) for part in argv], env=env, check=True, capture_output=True)
    return work, personal


def home(scratch: Path) -> Path:
    root = scratch / "home"
    for name in ["Desktop", "Documents", "Downloads", "Music", "Pictures", "Videos",
                 "Projects", "Documents/Invoices", "Pictures/Holidays"]:
        (root / name).mkdir(parents=True, exist_ok=True)
    (root / ".config" / "user-dirs.dirs").parent.mkdir(parents=True, exist_ok=True)
    gtk = root / ".config" / "gtk-4.0"
    gtk.mkdir(parents=True, exist_ok=True)
    (gtk / "settings.ini").write_text("[Settings]\ngtk-cursor-blink=false\n")
    (root / ".config" / "user-dirs.dirs").write_text("\n".join(
        f'XDG_{key}_DIR="$HOME/{value}"' for key, value in [
            ("DESKTOP", "Desktop"), ("DOCUMENTS", "Documents"), ("DOWNLOAD", "Downloads"),
            ("MUSIC", "Music"), ("PICTURES", "Pictures"), ("VIDEOS", "Videos"),
        ]) + "\n")
    bookmarks = root / ".config" / "gtk-3.0" / "bookmarks"
    bookmarks.parent.mkdir(parents=True, exist_ok=True)
    bookmarks.write_text("".join(
        f"file://{root / name} {label}\n" for name, label in [
            ("Projects", "Projects"), ("Documents/Invoices", "Invoices"),
            ("Pictures/Holidays", "Holidays"),
        ]))
    return root


def zone(names: list[str]) -> str:
    return "[" + ", ".join(f'"{name}"' for name in names) + "]"


def glimpse_config(config_dir: Path, scheme: str, work: Path, personal: Path, layout: dict,
                   motion=False):
    config_dir.mkdir(parents=True, exist_ok=True)
    (config_dir / "config.toml").write_text(f"""#:schema none
[appearance]
color-scheme = "{scheme}"
desktop-color-scheme = "external"
animation-speed = {1 if motion else 0}
blur = ["all"]

[regional]
language = "en"
units = "metric"

[geolocation]
provider = "manual"
latitude = 38.72
longitude = -9.14

[notifications]
hide-delay = {2 if motion else 1}

[wallpaper]
image = "{layout["wallpaper"]}"
transition-ms = 0

[printing]
server-url = "http://127.0.0.1:16631/"

[calendar]
sources = [
    {{ id = "work", name = "Work", type = "directory", uri = "{work}", color = "#3584e4" }},
    {{ id = "personal", name = "Personal", type = "directory", uri = "{personal}", color = "#e66100" }},
]
""")
    (config_dir / "panel.toml").write_text(f"""#:schema none
[[panels]]
position = "{layout["position"]}"
size = {PANEL_HEIGHT}
left = {zone(layout["left"])}
center = {zone(layout["center"])}
right = {zone(layout["right"])}

[applets.clock]
week-numbers = true
timezones = [
    {{ label = "Tokyo", timezone = "Asia/Tokyo" }},
    {{ label = "New York", timezone = "America/New_York" }},
    {{ label = "London", timezone = "Europe/London" }},
]

[applets.weather]
label = "Lisbon"
place = {{ at = "latlon", latitude = 38.72, longitude = -9.14 }}

[applets.printing]
show = "always"

[applets.next-event]
within = 720
""")


def notifications(env):
    for app, icon, summary, body in [
        ("Calendar", "x-office-calendar", "Design review in 10 minutes", "Room 4 · with the design team"),
        ("Files", "org.gnome.Nautilus", "Copy finished", "48 photos copied to Holidays"),
        ("Fractal", "org.gnome.Fractal", "Maya", "Lunch at Café Aurora still on for 1?"),
        ("Software", "org.gnome.Software", "Updates ready", "3 apps can be updated"),
    ]:
        subprocess.run(["notify-send", "-a", app, "-i", icon, summary, body], env=env,
                       capture_output=True)
        time.sleep(0.3)


def capture(env, path: Path):
    subprocess.run(["grim", "-o", "winit", str(path)], env=env, check=True)
    return Image.open(path).convert("RGB")


def rewind(env):
    bus = dbus.bus.BusConnection(env["DBUS_SESSION_BUS_ADDRESS"])
    for name in bus.list_names():
        if str(name).startswith("org.mpris.MediaPlayer2."):
            player = bus.get_object(name, "/org/mpris/MediaPlayer2")
            player.Seek(dbus.Int64(0), dbus_interface="org.mpris.MediaPlayer2.Player")
    bus.close()
    time.sleep(0.5)


def settle(env, base: Image.Image, bar: tuple[int, int], path: Path, visible: bool,
           timeout=6.0) -> Image.Image | None:
    deadline = time.time() + timeout
    previous = None
    while time.time() < deadline:
        time.sleep(0.4)
        shot = capture(env, path)
        box = popover_mask(base, shot, bar).getbbox()
        if not visible and box is None:
            return shot
        if visible and box is not None:
            if box == previous:
                time.sleep(0.6)
                return capture(env, path)
            previous = box
    return None


def popover_mask(base: Image.Image, shot: Image.Image, bar: tuple[int, int]) -> Image.Image:
    difference = ImageChops.difference(base, shot).convert("L")
    mask = difference.point(lambda value: 255 if value > FUZZ else 0)
    mask.paste(0, (0, bar[0], mask.width, bar[1]))
    return mask


def open_each(env, ctl, base, bar, shots: Path, tag: str, applets: list[str]):
    for applet in applets:
        if applet == "mpris":
            rewind(env)
        shot = None
        for attempt in range(3):
            opened = subprocess.run([BIN / "glimpsectl", "panel", "popover", "open", "winit", applet],
                                    env=ctl, capture_output=True)
            if opened.returncode != 0:
                print(f"  {applet}: no popover ({opened.stderr.decode().strip()[:120]})")
                break
            shot = settle(env, base, bar, shots / f"{applet}-{tag}.png", True)
            if shot is not None:
                break
        subprocess.run([BIN / "glimpsectl", "panel", "popovers", "close"], env=ctl,
                       capture_output=True)
        settle(env, base, bar, shots / "closing.png", False)
        if shot is None:
            print(f"  {applet}: nothing opened")
            continue
        box = popover_mask(base, shot, bar).getbbox()
        if box is not None:
            yield applet, shot, box


def panel_session(env, scheme: str, layout: dict, config_dir: Path, scratch: Path, work, personal):
    glimpse_config(config_dir, scheme, work, personal, layout)
    app_id = f"me.aresa.Glimpse.Docs{scheme.title()}{layout['position'].title()}"
    panel = spawn([BIN / "glimpse-panel", "--config-dir", config_dir],
                  dict(env, GLIMPSE_PANEL_APP_ID=app_id),
                  scratch / f"panel-{scheme}-{layout['position']}.log")
    time.sleep(7)
    shots = scratch / "shots"
    shots.mkdir(exist_ok=True)
    tag = f"{scheme}-{layout['position']}"
    base = capture(env, shots / f"base-{tag}.png")
    bar_height = round(PANEL_HEIGHT * SCALE)
    bar = (0, bar_height) if layout["position"] == "top" \
        else (base.height - bar_height, base.height)
    ctl = dict(env, GLIMPSE_PANEL_APP_ID=app_id)
    return panel, base, bar, ctl, shots, tag


def popovers(env, scheme, config_dir, scratch, work, personal):
    layout = {"position": "bottom", "left": [], "center": APPLETS, "right": [],
              "wallpaper": scratch / "sky.jpg"}
    panel, base, bar, ctl, shots, tag = panel_session(env, scheme, layout, config_dir, scratch,
                                                      work, personal)
    try:
        for applet, shot, box in open_each(env, ctl, base, bar, shots, tag, APPLETS):
            left, upper, right, lower = box
            crop = (max(0, left - PADDING), max(0, upper - PADDING),
                    min(shot.width, right + PADDING), min(bar[0], lower + PADDING))
            target = OUT / "applets" / f"{applet}-{scheme}.png"
            shot.crop(crop).save(target, optimize=True)
            print(f"  {applet}: {target.relative_to(REPO)}")
    finally:
        os.killpg(panel.pid, signal.SIGTERM)
        panel.wait(10)


def frames(env, scheme, config_dir, scratch, work, personal, results: dict):
    wanted = [name for names in COLLAGES.values() for name in names]
    layout = dict(COLLAGE_LAYOUT, wallpaper=WALLPAPER)
    panel, base, bar, ctl, shots, tag = panel_session(env, scheme, layout, config_dir,
                                                      scratch, work, personal)
    try:
        results[f"base-{scheme}"] = base
        for applet, shot, box in open_each(env, ctl, base, bar, shots, tag, wanted):
            results[f"{applet}-{scheme}"] = (shot, box)
            print(f"  frame {applet}")
    finally:
        os.killpg(panel.pid, signal.SIGTERM)
        panel.wait(10)


def rounded(image: Image.Image, radius: int) -> Image.Image:
    alpha = Image.new("L", image.size, 0)
    ImageDraw.Draw(alpha).rounded_rectangle((0, 0, image.width - 1, image.height - 1),
                                            radius=radius, fill=255)
    piece = image.convert("RGBA")
    piece.putalpha(alpha)
    return piece


def collage(results: dict, scheme: str, title: str, names: list[str]):
    names = [name for name in names
             if all(results.get(f"{name}-{each}") for each in ["light", "dark"])]
    if not names:
        return
    boxes = {name: [results[f"{name}-{each}"][1] for each in ["light", "dark"]] for name in names}
    bottom = max(box[3] for each in boxes.values() for box in each) + FRAME_PADDING
    pieces = []
    for name in names:
        shot = results[f"{name}-{scheme}"][0]
        left = max(0, min(box[0] for box in boxes[name]) - FRAME_PADDING)
        right = min(shot.width, max(box[2] for box in boxes[name]) + FRAME_PADDING)
        pieces.append(rounded(shot.crop((left, 0, right, min(shot.height, bottom))), FRAME_RADIUS))
    gap, margin = 56, 64
    width = sum(piece.width for piece in pieces) + gap * (len(pieces) - 1) + 2 * margin
    height = max(piece.height for piece in pieces) + 2 * margin
    base = results[f"base-{scheme}"]
    wallpaper = base.crop((0, int((PANEL_HEIGHT + 6) * SCALE), base.width, base.height))
    cover = max(width / wallpaper.width, height / wallpaper.height)
    wallpaper = wallpaper.resize((round(wallpaper.width * cover), round(wallpaper.height * cover)),
                                 Image.LANCZOS)
    wallpaper = wallpaper.crop((0, 0, width, height)).filter(ImageFilter.GaussianBlur(24))
    canvas = wallpaper.convert("RGBA")
    shadow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    x = margin
    for piece in pieces:
        silhouette = Image.new("RGBA", piece.size, (0, 0, 0, 0))
        silhouette.putalpha(piece.getchannel("A").point(lambda value: value * 110 // 255))
        shadow.alpha_composite(silhouette, (x, margin + 16))
        x += piece.width + gap
    canvas.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(26)))
    x = margin
    for piece in pieces:
        canvas.alpha_composite(piece, (x, margin))
        x += piece.width + gap
    target = OUT / "home" / f"{title}-{scheme}.png"
    canvas.convert("RGB").save(target, optimize=True)
    print(f"  collage: {target.relative_to(REPO)} {canvas.width}x{canvas.height}")


def video(env, config_dir: Path, scratch: Path, work: Path, personal: Path):
    glimpse_config(config_dir, "desktop", work, personal, dict(COLLAGE_LAYOUT, wallpaper=WALLPAPER),
                   motion=True)
    app_id = "me.aresa.Glimpse.DocsVideo"
    ctl = dict(env, GLIMPSE_PANEL_APP_ID=app_id)

    def glimpsectl(*argv):
        subprocess.run([BIN / "glimpsectl", "--config-dir", config_dir, *argv], env=ctl,
                       capture_output=True)

    logical = [size / VIDEO_SCALE for size in VIDEO_WINDOW]
    pixel = logical[0] / VIDEO_SIZE[0]
    pointer = [logical[0] / 2, logical[1] / 2]

    def move(x: float, y: float):
        subprocess.run([BIN / "examples" / "click", "winit", str(round(x)), str(round(y)),
                        str(round(logical[0])), str(round(logical[1])), "move"], env=env,
                       capture_output=True)

    def glide(x: float, y: float):
        sx, sy = pointer
        steps = round(VIDEO_GLIDE * 20)
        for step in range(1, steps + 1):
            t = step / steps
            ease = t * t * (3 - 2 * t)
            move(sx + (x - sx) * ease, sy + (y - sy) * ease)
            time.sleep(VIDEO_GLIDE / steps)
        pointer[:] = [x, y]

    glimpsectl("settings", "mode", MODES[0])
    panel = spawn([BIN / "glimpse-panel", "--config-dir", config_dir], ctl, scratch / "panel-video.log")
    time.sleep(7)
    move(*pointer)
    rewind(env)
    raw = scratch / "raw.mkv"
    recorder = spawn(["wf-recorder", "-y", "-D", "-o", "winit", "-r", "30", "-c", "libx264rgb",
                      "-p", "crf=0", "-p", "preset=ultrafast", "-f", raw], env,
                     scratch / "recorder.log")
    wait_for(lambda: raw.exists() and raw.stat().st_size > 0, "the recorder")
    start = time.monotonic() + VIDEO_LEAD

    def at(second: float):
        time.sleep(max(0.0, start + second - time.monotonic()))

    def popover(applet: str | None):
        glimpsectl("panel", "popovers", "close")
        if applet:
            glide(PANEL_X[applet] * pixel, logical[1] - 24 * pixel)
            glimpsectl("panel", "popover", "open", "winit", applet)

    def volume(level: int):
        subprocess.run(["pactl", "set-sink-volume", "headphones", f"{level}%"], env=env,
                       capture_output=True)

    try:
        at(VIDEO_CUE)
        popover("clock")
        at(7.0)
        popover("network")
        at(9.0)
        popover("bluetooth")
        at(11.0)
        popover("audio")
        at(12.0)
        for level in [*range(62, 30, -2), *range(30, 76, 3)]:
            volume(level)
            time.sleep(0.07)
        at(15.0)
        popover("mpris")
        at(19.0)
        popover(None)
        subprocess.run(["notify-send", "-a", "Clocks", "-i", "org.gnome.clocks", "Timer finished",
                        "Tea is ready"], env=env, capture_output=True)
        at(21.0)
        popover("notifications")
        at(23.0)
        popover(None)
        glimpsectl("settings", "mode", MODES[1])
        at(24.5)
        popover("weather")
        at(28.0)
        popover(None)
        glimpsectl("settings", "mode", MODES[0])
        at(VIDEO_LENGTH + VIDEO_FADE + 0.5)
    finally:
        recorder.send_signal(signal.SIGINT)
        recorder.wait(15)
        os.killpg(panel.pid, signal.SIGTERM)
        panel.wait(10)
    encode(raw)


def first_change(raw: Path) -> float:
    out = subprocess.run(["ffmpeg", "-loglevel", "error", "-i", str(raw), "-vf",
                          "select='gt(scene,0.005)',metadata=print:file=-", "-an", "-f", "null",
                          "-"], capture_output=True, text=True, check=True).stdout
    for line in out.splitlines():
        if "pts_time:" in line:
            return float(line.split("pts_time:")[1].split()[0])
    raise SystemExit("the recording shows no popover opening")


def encode(raw: Path):
    VIDEO_OUT.mkdir(parents=True, exist_ok=True)
    width, height = VIDEO_SIZE
    lead = max(0.0, first_change(raw) - VIDEO_CUE - VIDEO_GLIDE)
    body, tail = lead + VIDEO_FADE, lead + VIDEO_FADE + VIDEO_LENGTH
    graph = (
        f"[0:v]fps=30,scale={width}:{height}:flags=lanczos,split[a][b];"
        f"[a]trim={body}:{tail},setpts=PTS-STARTPTS[main];"
        f"[b]trim={lead}:{body},setpts=PTS-STARTPTS[head];"
        f"[main][head]xfade=transition=fade:duration={VIDEO_FADE}"
        f":offset={VIDEO_LENGTH - VIDEO_FADE},"
        f"setpts=PTS/{VIDEO_SPEED},fps=30,format=yuv420p[out]"
    )
    for name, codec in [
        (f"demo{SUFFIX}.webm", ["-c:v", "libvpx-vp9", "-crf", "38", "-b:v", "0", "-row-mt", "1",
                       "-deadline", "good", "-cpu-used", "2"]),
        (f"demo{SUFFIX}.mp4", ["-c:v", "libx264", "-crf", "27", "-preset", "slow", "-profile:v", "high",
                      "-movflags", "+faststart"]),
    ]:
        target = VIDEO_OUT / name
        subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", str(raw), "-filter_complex",
                        graph, "-map", "[out]", "-an", *codec, str(target)], check=True)
        print(f"  video: {target.relative_to(REPO)} {target.stat().st_size // 1024} KiB")
    poster = VIDEO_OUT / f"demo{SUFFIX}-poster.jpg"
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-ss", str(VIDEO_CUE + 1.5), "-i",
                    str(VIDEO_OUT / f"demo{SUFFIX}.mp4"), "-frames:v", "1", "-q:v", "3", str(poster)],
                   check=True)
    print(f"  poster: {poster.relative_to(REPO)}")


def point(env, x: int, y: int, action="move"):
    width, height = SCREENSHOT_WINDOW
    subprocess.run([BIN / "examples" / "click", "winit", str(x), str(y), str(width), str(height),
                    action], env=env, check=True, capture_output=True)
    time.sleep(0.3)


def overlay(env, argv, path: Path, log: Path, steps, crop=None, settle=2.5):
    tool = spawn(argv, env, log)
    time.sleep(settle)
    for step in steps:
        step()
    time.sleep(1)
    shot = capture(env, path)
    subprocess.run(["wtype", "-k", "Escape"], env=env, capture_output=True)
    try:
        tool.wait(5)
    except subprocess.TimeoutExpired:
        os.killpg(tool.pid, signal.SIGTERM)
    if crop:
        ratio = shot.width / SCREENSHOT_WINDOW[0]
        shot = shot.crop(tuple(round(side * ratio) for side in crop))
    return shot


def ecosystem(env, config_dir, scratch, work, personal):
    (OUT / "ecosystem").mkdir(parents=True, exist_ok=True)
    shots = scratch / "shots"
    shots.mkdir(exist_ok=True)
    wait_for(lambda: capture(env, shots / "desktop.png").convert("L").getextrema()[1] > 0,
             "the wallpaper")
    lens = [
        ("lens-magnifier", ["--zoom", "12"], [lambda: point(env, 700, 600)],
         (480, 400, 920, 840)),
        ("lens-picker", ["--tool", "picker", "--zoom", "8"], [lambda: point(env, 1000, 640)],
         (780, 440, 1220, 920)),
        ("lens-ruler", ["--tool", "ruler", "--zoom", "4", "--radius", "120"],
         [lambda: point(env, 620, 420, "left"), lambda: point(env, 1080, 420, "left"),
          lambda: point(env, 1080, 760, "left"), lambda: point(env, 1250, 860)],
         (480, 280, 1420, 1020)),
    ]
    for name, argv, steps, crop in lens:
        shot = overlay(env, [BIN / "glimpse-lens", *argv], shots / f"{name}.png",
                       scratch / f"{name}.log", steps, crop)
        target = OUT / "ecosystem" / f"{name}.png"
        shot.save(target, optimize=True)
        print(f"  {target.relative_to(REPO)}")
    layout = dict(COLLAGE_LAYOUT, wallpaper=WALLPAPER)
    for scheme in ["light", "dark"]:
        glimpse_config(config_dir, scheme, work, personal, layout)
        app_id = f"me.aresa.Glimpse.DocsShot{scheme.title()}"
        ctl = dict(env, GLIMPSE_PANEL_APP_ID=app_id)
        panel = spawn([BIN / "glimpse-panel", "--config-dir", config_dir], ctl,
                      scratch / f"panel-shot-{scheme}.log")
        time.sleep(7)
        try:
            subprocess.run([BIN / "glimpsectl", "panel", "popover", "open", "winit", "weather"],
                           env=ctl, capture_output=True)
            time.sleep(2)
            ratio = capture(env, shots / f"shot-base-{scheme}.png").width / SCREENSHOT_WINDOW[0]
            marks = scratch / "marks.json"
            marks.write_text(json.dumps(scaled(SHOT_MARKS, ratio)))
            shot = overlay(env, [BIN / "glimpse-shot", "--config-dir", config_dir, "--screen",
                                 "--annotate", marks, "--no-flash", "--no-notify"],
                           shots / f"shot-{scheme}.png", scratch / f"shot-{scheme}.log", [],
                           settle=6)
            subprocess.run([BIN / "glimpsectl", "panel", "popovers", "close"], env=ctl,
                           capture_output=True)
            target = OUT / "ecosystem" / f"shot-editor-{scheme}.png"
            shot.save(target, optimize=True)
            print(f"  {target.relative_to(REPO)}")
        finally:
            os.killpg(panel.pid, signal.SIGTERM)
            panel.wait(10)


def scaled(marks: list[dict], ratio: float) -> list[dict]:
    def scale(value):
        if isinstance(value, list):
            return [scale(each) for each in value]
        return round(value * ratio)

    return [{key: scale(value) if key in ("x", "y", "width", "height", "from", "to", "points")
             else value for key, value in mark.items()} for mark in marks]


def libexec(name: str) -> Path:
    for directory in ["/usr/lib", "/usr/libexec"]:
        if (Path(directory) / name).exists():
            return Path(directory) / name
    raise SystemExit(f"{name} is not installed")


def portal(env, scratch: Path):
    conf = Path(env["XDG_CONFIG_HOME"]) / "xdg-desktop-portal" / "docs-portals.conf"
    conf.parent.mkdir(parents=True, exist_ok=True)
    conf.write_text("[preferred]\ndefault=gtk\n")
    desktop = dict(env, XDG_CURRENT_DESKTOP="docs")
    spawn([libexec("xdg-desktop-portal-gtk")], desktop, scratch / "portal-gtk.log")
    wait_for(lambda: owned(env["DBUS_SESSION_BUS_ADDRESS"],
                           "org.freedesktop.impl.portal.desktop.gtk"), "the GTK portal")
    spawn([libexec("xdg-desktop-portal")], desktop, scratch / "portal.log")
    wait_for(lambda: owned(env["DBUS_SESSION_BUS_ADDRESS"], "org.freedesktop.portal.Desktop"),
             "the desktop portal")


def terminated(signum, frame):
    raise SystemExit(f"stopped by signal {signum}")


def main():
    recording = VIDEO
    signal.signal(signal.SIGTERM, terminated)
    signal.signal(signal.SIGINT, terminated)
    for directory in ["applets", "home"]:
        (OUT / directory).mkdir(parents=True, exist_ok=True)
    runtime = Path(os.environ["XDG_RUNTIME_DIR"])
    parent_wayland = runtime / os.environ.get("WAYLAND_DISPLAY", "wayland-1")
    scratch = Path(tempfile.mkdtemp(prefix="gdocs-", dir=runtime))
    run = scratch / "run"
    run.mkdir(mode=0o700)
    root = home(scratch)
    env = {
        "PATH": os.environ["PATH"],
        "HOME": str(root),
        "USER": "sam",
        "XDG_CONFIG_HOME": str(root / ".config"),
        "XDG_DATA_HOME": str(root / ".local" / "share"),
        "XDG_STATE_HOME": str(root / ".local" / "state"),
        "XDG_CACHE_HOME": str(root / ".cache"),
        "XDG_RUNTIME_DIR": str(run),
        "LANG": "en_US.UTF-8",
        "LC_ALL": "en_US.UTF-8",
        "LANGUAGE": "en",
        "TZ": "Europe/Lisbon",
    }
    try:
        env["DBUS_SESSION_BUS_ADDRESS"] = bus_daemon(run / "bus", env)
        spawn([libexec("dconf-service")], env, scratch / "dconf.log")
        system = bus_daemon(run / "system_bus", env)
        env["DBUS_SYSTEM_BUS_ADDRESS"] = system
        print("system services")
        system_services(system, env, scratch)
        print("audio")
        audio(env, scratch)
        print("compositor")
        wayland, niri_socket = nested(dict(env, WAYLAND_DISPLAY=str(parent_wayland)), scratch,
                                      *((VIDEO_WINDOW, VIDEO_SCALE) if recording
                                        else (SCREENSHOT_WINDOW, SCALE)))
        env["WAYLAND_DISPLAY"] = wayland
        env["NIRI_SOCKET"] = niri_socket
        work, personal = calendar(scratch, env)
        if recording:
            portal(env, scratch)
        print("desktop")
        config_dir = scratch / "config"
        Image.open(WALLPAPER).transpose(Image.Transpose.FLIP_TOP_BOTTOM).save(scratch / "sky.jpg",
                                                                             quality=95)
        glimpse_config(config_dir, "light", work, personal, dict(COLLAGE_LAYOUT, wallpaper=WALLPAPER))
        spawn([BIN / "glimpse-wallpaper", "--config-dir", config_dir], env, scratch / "wallpaper.log")
        spawn([BIN / "glimpse-notifications", "--config-dir", config_dir], env,
              scratch / "notifications.log")
        spawn([BIN / "glimpse-weather", "--config-dir", config_dir], env, scratch / "weather.log")
        spawn([BIN / "glimpse-idle", "--config-dir", config_dir], env, scratch / "idle.log")
        spawn([BIN / "glimpse-settings-daemon", "--config-dir", config_dir], env,
              scratch / "settings.log")
        spawn(["uv", "run", "--script", SCRIPTS / "mpris-fake-players.py", "--plain-names"], env,
              scratch / "mpris.log")
        spawn([BIN / "examples" / "fake-tray"], env, scratch / "tray.log")
        spawn(["uv", "run", "--script", SCRIPTS / "printing-mock-cups.py", "--scenario", "6", "--hold"], env,
              scratch / "cups.log")
        subprocess.run(["niri", "msg", "action", "focus-workspace", "music"], env=env)
        for text in ["https://gitlab.gnome.org/GNOME/gtk", "Meeting notes: ship the docs site",
                     "#3584e4"]:
            subprocess.run(["wl-copy", text], env=env)
            time.sleep(0.4)
        time.sleep(3)
        notifications(env)
        time.sleep(4)
        if recording:
            video(env, config_dir, scratch, work, personal)
            return
        print("ecosystem")
        ecosystem(env, config_dir, scratch, work, personal)
        results: dict = {}
        for scheme in ["light", "dark"]:
            print(scheme)
            popovers(env, scheme, config_dir, scratch, work, personal)
            frames(env, scheme, config_dir, scratch, work, personal, results)
        for scheme in ["light", "dark"]:
            for title, names in COLLAGES.items():
                collage(results, scheme, title, names)
    finally:
        stop_all()
        if os.environ.get("GLIMPSE_DOCS_KEEP"):
            print(f"kept {scratch}")
        else:
            shutil.rmtree(scratch, ignore_errors=True)


if __name__ == "__main__":
    main()
