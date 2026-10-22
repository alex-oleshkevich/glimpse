#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11,<3.14"
# dependencies = [
#     "python-dbusmock==0.38.1",
#     "PyGObject",
#     "Pillow>=10",
#     "kokoro-onnx>=0.4",
#     "soundfile",
# ]
# ///
"""Record the release video inside the documentation sandbox.

The sandbox comes from docs-screenshots.py: a nested compositor, private buses, mocked system
services and made-up data, so nothing from the running session can reach the video. The narration
is synthesized locally with Kokoro and the music is a CC0 track; both are downloaded once into
~/.cache/glimpse-video. The result goes to var/release-video/.

Usage (through the justfile):
    just release-video
"""

import importlib.util
import json
import os
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

import dbus
import dbus.bus
import soundfile

REPO = Path(__file__).resolve().parent.parent
LENS = REPO / "target" / "debug" / "glimpse-lens"
SHOT = REPO / "target" / "release" / "glimpse-shot"
OUT = REPO / "var" / "release-video"
CACHE = Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache")) / "glimpse-video"
KOKORO = "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/"
MUSIC = {
    "file": "komiku-cliff-road-chill.mp3",
    "url": "https://files.freemusicarchive.org/storage-freemusicarchive-org/tracks/"
           "RkgeLGj2Fu1O0GItYpqGT1PnGgbQDAu3a2uZo3De.mp3",
    "credit": "Music: \"Cliff Road Chill\" by Komiku, CC0 1.0, "
              "https://freemusicarchive.org/music/Komiku/Helice_Awesome_Dance_Adventure_/"
              "cliff-road-chill/",
}
VOICE = "af_heart"
VOICE_SPEED = 1.1
VERSION = "1.0"
URL = "alex-oleshkevich.github.io/glimpse"
SIZE = (2560, 1440)
WINDOW = (2048, 1152)
SCALE = 16 / 9
FPS = 30
BODY = 56.0
END = 4.0
FADE = 0.5
CUE = 4.5
LAYOUT = {
    "position": "top",
    "left": ["workspace-name", "mpris"],
    "center": ["clock", "weather", "notifications"],
    "right": ["system-monitor", "clipboard", "keyboard", "bluetooth", "network", "display",
              "audio", "power", "session"],
}
SCENES = [
    (0.0, "glimpse 1.0", "Meet glimpse one point oh! A fresh desktop shell for Wayland."),
    (4.0, "Your day at a glance", "One click, and there's your day: meetings, weather, all of it."),
    (11.0, "Sound follows you", "Pop in your headphones, and the sound jumps right over."),
    (18.0, "Music within reach", "Your music? Always one click away."),
    (24.0, "Notifications that wait",
     "Notifications show up, wait their turn, and go quiet when you need to focus."),
    (31.0, "Pick any color", "Pick any color on screen, right down to the pixel!"),
    (38.0, "Measure in place", "Measure anything, right where it is."),
    (43.0, "Capture, mark up, blur", "Grab a screenshot, point at what matters, and blur the rest."),
    (51.0, "And much more", "Brightness, power, keyboard layouts, your clipboard, and so much more."),
    (BODY, None, "glimpse. Free, open source, and yours."),
]


def sandbox():
    spec = importlib.util.spec_from_file_location("sandbox", REPO / "scripts" / "docs-screenshots.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.VIDEO_WINDOW = WINDOW
    module.VIDEO_SCALE = SCALE
    return module


def fetch(name: str, url: str) -> Path:
    path = CACHE / name
    if not path.exists():
        CACHE.mkdir(parents=True, exist_ok=True)
        print(f"downloading {name}")
        partial = path.with_suffix(path.suffix + ".part")
        request = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
        with urllib.request.urlopen(request) as response:
            partial.write_bytes(response.read())
        partial.rename(path)
    return path


def narration(work: Path) -> list[tuple[float, float, Path, str]]:
    from kokoro_onnx import Kokoro

    voice = Kokoro(str(fetch("kokoro-v1.0.onnx", KOKORO + "kokoro-v1.0.onnx")),
                   str(fetch("voices-v1.0.bin", KOKORO + "voices-v1.0.bin")))
    lines, problems = [], []
    for index, (start, _, text) in enumerate(SCENES):
        samples, rate = voice.create(text, voice=VOICE, speed=VOICE_SPEED, lang="en-us")
        path = work / f"line-{index}.wav"
        soundfile.write(path, samples, rate)
        length = len(samples) / rate
        following = SCENES[index + 1][0] if index + 1 < len(SCENES) else BODY + END
        room = following - start - 0.4
        print(f"  line {index}: {length:.1f} s of {room:.1f} s")
        if length > room:
            problems.append(index)
        lines.append((start + 0.2, length, path, text))
    if problems:
        raise SystemExit(f"narration lines {problems} run longer than their scenes")
    return lines


def others(ds) -> list[int]:
    out = subprocess.run(["niri", "msg", "-j", "windows"], capture_output=True, text=True)
    if out.returncode != 0:
        return []
    mine = {process.pid for process in ds.children}
    return [window["pid"] for window in json.loads(out.stdout)
            if window.get("app_id") == "niri" and window.get("pid") not in mine]


def alone(ds):
    deadline = time.monotonic() + 600
    while others(ds):
        if time.monotonic() > deadline:
            raise SystemExit("another nested compositor stayed on screen for 10 minutes")
        print("  waiting: another nested compositor is on screen and would stall the recording")
        time.sleep(15)


def watchdog(raw: Path, start: float):
    import threading

    stalled = threading.Event()

    def watch():
        size, since = -1, time.monotonic()
        while not stalled.is_set():
            time.sleep(1)
            current = raw.stat().st_size if raw.exists() else 0
            if current != size:
                size, since = current, time.monotonic()
            elif time.monotonic() - since > 6:
                print(f"the recording stalled {time.monotonic() - start - 6:.1f} s into the "
                      "timeline: the nested compositor stopped drawing; its window must stay "
                      "visible on a powered screen", flush=True)
                for what in ["workspaces", "windows"]:
                    listing = subprocess.run(["niri", "msg", "-j", what], capture_output=True,
                                             text=True).stdout
                    (OUT / f"stall-{what}.json").write_text(listing)
                os.kill(os.getpid(), 15)
                return

    threading.Thread(target=watch, daemon=True).start()
    return stalled


def record(ds, env, config_dir: Path, scratch: Path, work: Path, personal: Path):
    ds.glimpse_config(config_dir, "desktop", work, personal, dict(LAYOUT, wallpaper=ds.WALLPAPER),
                      motion=True)
    app_id = "me.aresa.Glimpse.ReleaseVideo"
    ctl = dict(env, GLIMPSE_PANEL_APP_ID=app_id)

    def glimpsectl(*argv):
        subprocess.run([ds.BIN / "glimpsectl", "--config-dir", config_dir, *argv], env=ctl,
                       capture_output=True)

    def popover(applet: str | None):
        glimpsectl("dev", "close-popovers")
        if applet:
            glimpsectl("applet", "open", applet)

    outputs = json.loads(subprocess.run(["niri", "msg", "-j", "outputs"], env=env,
                                        capture_output=True, text=True, check=True).stdout)
    logical = outputs["winit"]["logical"]
    width, height = logical["width"], logical["height"]
    pointer = [width // 2, height // 2]

    def click(x: float, y: float, action="move"):
        subprocess.run([ds.BIN / "examples" / "click", "winit", str(round(x)), str(round(y)),
                        str(width), str(height), action], env=env, capture_output=True)

    def glide(x: float, y: float, seconds: float):
        sx, sy = pointer
        steps = max(1, round(seconds * FPS))
        for step in range(1, steps + 1):
            t = step / steps
            ease = t * t * (3 - 2 * t)
            click(sx + (x - sx) * ease, sy + (y - sy) * ease)
            time.sleep(seconds / steps)
        pointer[:] = [x, y]

    system = dbus.bus.BusConnection(env["DBUS_SYSTEM_BUS_ADDRESS"])
    headphones = dbus.Interface(system.get_object("org.bluez", "/org/bluez/hci0/dev_11_22_33_44_55_01"),
                                "org.bluez.Device1")
    session = dbus.bus.BusConnection(env["DBUS_SESSION_BUS_ADDRESS"])
    players = [name for name in session.list_names()
               if str(name).startswith("org.mpris.MediaPlayer2.")]

    def player(method: str):
        for name in players[:1]:
            dbus.Interface(session.get_object(name, "/org/mpris/MediaPlayer2"),
                           "org.mpris.MediaPlayer2.Player").get_dbus_method(method)()

    glimpsectl("appearance", "set", "light")
    headphones.Disconnect()
    panel = ds.spawn([ds.BIN / "glimpse-panel", "--config-dir", config_dir], ctl, scratch / "panel.log")
    time.sleep(8)
    ds.rewind(env)
    raw = scratch / "raw.mkv"
    recorder = ds.spawn(["wf-recorder", "-y", "-D", "-o", "winit", "-r", str(FPS), "-c", "libx264",
                         "-p", "crf=10", "-p", "preset=ultrafast", "-f", raw], env,
                        scratch / "recorder.log")
    ds.wait_for(lambda: raw.exists() and raw.stat().st_size > 0, "the recorder")
    start = time.monotonic() + 2.0
    stopped = watchdog(raw, start)

    def at(second: float):
        time.sleep(max(0.0, start + second - time.monotonic()))

    region = (width * 0.26, height * 0.045, width * 0.40, height * 0.68)
    ratio = SIZE[0] / width

    def buffer(x: float, y: float) -> list[int]:
        return [round(x * width * ratio), round(y * height * ratio)]

    marks = scratch / "marks.json"
    marks.write_text(json.dumps([
        {"kind": "arrow", "from": buffer(0.31, 0.50), "to": buffer(0.435, 0.375), "color": "red",
         "stroke": 3},
        {"kind": "number", "x": buffer(0.30, 0.52)[0], "y": buffer(0.30, 0.52)[1], "color": "red",
         "stroke": 2},
        {"kind": "blur", "x": buffer(0.33, 0.57)[0], "y": buffer(0.33, 0.57)[1],
         "width": round(width * 0.26 * ratio), "height": round(height * 0.07 * ratio)},
    ]))
    geometry = f"{round(region[0])},{round(region[1])} {round(region[2])}x{round(region[3])}"
    try:
        at(CUE)
        popover("clock")
        at(8.0)
        popover("weather")
        at(11.0)
        popover("bluetooth")
        at(12.6)
        headphones.Connect()
        at(14.6)
        popover("audio")
        at(15.4)
        for level in [*range(62, 34, -2), *range(34, 80, 3)]:
            subprocess.run(["pactl", "set-sink-volume", "headphones", f"{level}%"], env=env,
                           capture_output=True)
            time.sleep(0.06)
        at(18.0)
        popover("mpris")
        at(20.4)
        player("Pause")
        at(21.6)
        player("Next")
        player("Play")
        at(24.0)
        popover(None)
        for app, icon, summary, body in [
            ("Calendar", "x-office-calendar", "Design review in 10 minutes", "Room 4"),
            ("Clocks", "org.gnome.clocks", "Timer finished", "Tea is ready"),
        ]:
            subprocess.run(["notify-send", "-a", app, "-i", icon, summary, body], env=env,
                           capture_output=True)
            time.sleep(1.0)
        at(27.0)
        popover("notifications")
        at(29.0)
        glimpsectl("notifications", "dnd", "on")
        at(31.0)
        glimpsectl("notifications", "dnd", "off")
        popover("clock")
        click(width * 0.5, height * 0.8)
        pointer[:] = [width * 0.5, height * 0.8]
        at(31.6)
        lens = ds.spawn([LENS, "--tool", "picker", "--zoom", "10"], env, scratch / "lens.log")
        at(32.6)
        glide(width * 0.16, height * 0.30, 1.6)
        glide(width * 0.20, height * 0.62, 1.4)
        at(36.4)
        click(*pointer, "left")
        lens.wait(5)
        at(38.0)
        ruler = ds.spawn([LENS, "--tool", "ruler", "--zoom", "4", "--radius", "110"], env,
                         scratch / "ruler.log")
        at(38.8)
        glide(width * 0.70, height * 0.24, 0.5)
        click(*pointer, "left")
        glide(width * 0.92, height * 0.24, 1.2)
        click(*pointer, "left")
        glide(width * 0.92, height * 0.62, 1.0)
        click(*pointer, "left")
        at(42.2)
        subprocess.run(["wtype", "-k", "Escape"], env=env, capture_output=True)
        ruler.wait(5)
        glimpsectl("applet", "open", "clock")
        time.sleep(0.4)
        shot = ds.spawn([SHOT, "--config-dir", config_dir, "--region", geometry, "--annotate", marks,
                         "--no-flash"], env, scratch / "shot.log")
        at(48.8)
        subprocess.run(["wtype", "-M", "ctrl", "-k", "s", "-m", "ctrl"], env=env, capture_output=True)
        at(50.6)
        if shot.poll() is None:
            ds.os.killpg(shot.pid, 15)
            shot.wait(5)
        at(50.9)
        glimpsectl("notifications", "clear")
        at(51.0)
        popover("display")
        at(52.0)
        popover("power")
        at(53.0)
        popover("keyboard")
        at(54.0)
        popover("clipboard")
        at(54.6)
        glimpsectl("appearance", "set", "dark")
        at(55.0)
        popover("system-monitor")
        at(BODY + 1.0)
        popover(None)
        at(BODY + 2.0)
    finally:
        stopped.set()
        recorder.send_signal(2)
        recorder.wait(20)
        ds.os.killpg(panel.pid, 15)
        panel.wait(10)
    OUT.mkdir(parents=True, exist_ok=True)
    kept = OUT / "raw.mkv"
    kept.write_bytes(raw.read_bytes())
    return kept


def first_change(raw: Path) -> float:
    out = subprocess.run(["ffmpeg", "-loglevel", "error", "-i", str(raw), "-vf",
                          "select='gt(scene,0.01)',metadata=print:file=-", "-an", "-f", "null",
                          "-"], capture_output=True, text=True, check=True).stdout
    for line in out.splitlines():
        if "pts_time:" in line:
            return float(line.split("pts_time:")[1].split()[0])
    raise SystemExit("the recording shows no popover opening")


def font(style: str) -> str:
    return subprocess.run(["fc-match", "-f", "%{file}", f"Adwaita Sans:{style}"],
                          capture_output=True, text=True, check=True).stdout


def text(value: str) -> str:
    return value.replace("\\", "\\\\").replace(":", "\\:").replace("'", "\u2019")


def end_card(work: Path) -> Path:
    logo = work / "logo.png"
    subprocess.run(["rsvg-convert", "-w", "220", "-h", "220", "-o", str(logo),
                    str(REPO / "website" / "public" / "assets" / "logo.svg")], check=True)
    card = work / "end.mp4"
    width, height = SIZE
    bold, regular = font("bold"), font("regular")
    graph = (
        f"[0:v]scale={width}:{height}:force_original_aspect_ratio=increase,crop={width}:{height},"
        f"boxblur=40:3,eq=brightness=-0.18[bg];"
        f"[bg][1:v]overlay=(W-w)/2:H*0.24[l];"
        f"[l]drawtext=fontfile={bold}:text='glimpse {VERSION}':fontsize=132:fontcolor=white:"
        f"x=(w-tw)/2:y=h*0.46,"
        f"drawtext=fontfile={regular}:text='A desktop shell for Wayland':fontsize=58:"
        f"fontcolor=white@0.9:x=(w-tw)/2:y=h*0.58,"
        f"drawtext=fontfile={regular}:text='{URL}':fontsize=46:fontcolor=white@0.85:"
        f"x=(w-tw)/2:y=h*0.70,"
        f"drawtext=fontfile={regular}:text='Free and open source · BSD 3-Clause':fontsize=36:"
        f"fontcolor=white@0.7:x=(w-tw)/2:y=h*0.77,format=yuv420p[out]"
    )
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-loop", "1", "-i", str(REPO / "data" /
                    "themes" / "adwaita" / "default.jpg"), "-loop", "1", "-i", str(logo),
                    "-filter_complex", graph, "-map", "[out]", "-t", str(END + FADE), "-r",
                    str(FPS), "-c:v", "libx264", "-crf", "12", "-preset", "fast", str(card)],
                   check=True)
    return card


def subtitles(lines, path: Path):
    def stamp(seconds: float) -> str:
        millis = round(seconds * 1000)
        return f"{millis // 3600000:02}:{millis // 60000 % 60:02}:{millis // 1000 % 60:02},{millis % 1000:03}"

    path.write_text("".join(
        f"{index}\n{stamp(start)} --> {stamp(start + length)}\n{line}\n\n"
        for index, (start, length, _, line) in enumerate(lines, 1)))


def render(raw: Path, lines):
    work = OUT / "work"
    work.mkdir(parents=True, exist_ok=True)
    card = end_card(work)
    lead = max(0.0, first_change(raw) - CUE)
    width, height = SIZE
    bold = font("bold")
    captions = []
    for index, (start, caption, _) in enumerate(SCENES):
        if not caption:
            continue
        stop = SCENES[index + 1][0] - 0.3
        size = 96 if index == 0 else 64
        fade = f"if(lt(t,{start + 0.3}),(t-{start})/0.3,if(gt(t,{stop - 0.3}),({stop}-t)/0.3,1))"
        captions.append(
            f"drawtext=fontfile={bold}:text='{text(caption)}':fontsize={size}:fontcolor=white:"
            f"box=1:boxcolor=black@0.38:boxborderw=28|40:x=96:y=h-th-120:"
            f"alpha='{fade}':enable='between(t,{start},{stop})'"
        )
    voices = "".join(
        f"[{index + 3}:a]aresample=48000,aformat=channel_layouts=stereo,adelay={round(start * 1000)}:all=1[v{index}];"
        for index, (start, _, _, _) in enumerate(lines))
    mix = "".join(f"[v{index}]" for index in range(len(lines)))
    total = BODY + END
    graph = (
        f"[0:v]trim={lead}:{lead + BODY + FADE},setpts=PTS-STARTPTS,fps={FPS},"
        f"scale={width}:{height}:flags=lanczos,format=yuv420p,settb=1/{FPS},fade=t=in:st=0:d=0.8,"
        + ",".join(captions)
        + f"[body];[1:v]fps={FPS},format=yuv420p,setpts=PTS-STARTPTS,settb=1/{FPS}[card];"
        f"[body][card]xfade=transition=fade:duration={FADE}:offset={BODY},"
        f"scale=out_range=tv:out_color_matrix=bt709,format=yuv420p[video];"
        + voices
        + f"{mix}amix=inputs={len(lines)}:normalize=0,volume=1.6,apad,atrim=0:{total}[voice];"
        f"[voice]asplit[voice1][voice2];"
        f"[2:a]aresample=48000,aformat=channel_layouts=stereo,atrim=0:{total},volume=0.55,afade=t=in:st=0:d=1.5,"
        f"afade=t=out:st={total - 3}:d=3[music];"
        f"[music][voice1]sidechaincompress=threshold=0.03:ratio=8:attack=40:release=500[ducked];"
        f"[ducked][voice2]amix=inputs=2:normalize=0,alimiter=limit=0.9[audio]"
    )
    inputs = ["-i", str(raw), "-i", str(card), "-i", str(fetch(MUSIC["file"], MUSIC["url"]))]
    for _, _, path, _ in lines:
        inputs += ["-i", str(path)]
    target = OUT / f"glimpse-{VERSION}.mp4"
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", *inputs, "-filter_complex", graph,
                    "-map", "[video]", "-map", "[audio]", "-t", str(total), "-c:v", "libx264",
                    "-crf", "16", "-preset", "slow", "-profile:v", "high", "-pix_fmt", "yuv420p",
                    "-color_range", "tv", "-colorspace", "bt709", "-color_primaries", "bt709",
                    "-color_trc", "bt709",
                    "-c:a", "aac", "-b:a", "256k", "-movflags", "+faststart", str(target)],
                   check=True)
    subtitles(lines, OUT / f"glimpse-{VERSION}.en.srt")
    (OUT / "credits.txt").write_text(
        f"{MUSIC['credit']}\nNarration: synthesized with Kokoro (Apache-2.0), voice {VOICE}.\n")
    print(f"  video: {target.relative_to(REPO)}")


def main():
    work = OUT / "work"
    work.mkdir(parents=True, exist_ok=True)
    print("narration")
    lines = narration(work)
    raw = OUT / "raw.mkv"
    if "--reuse" not in sys.argv[1:] or not raw.exists():
        ds = sandbox()
        captured = {}

        def video(env, config_dir, scratch, work_calendar, personal):
            captured["raw"] = record(ds, env, config_dir, scratch, work_calendar, personal)

        ds.video = video
        nested = ds.nested

        def guarded(*args):
            alone(ds)
            subprocess.run(["niri", "msg", "action", "power-on-monitors"], capture_output=True)
            return nested(*args)

        ds.nested = guarded
        sys.argv = [sys.argv[0], "--video"]
        ds.main()
        raw = captured["raw"]
    print("render")
    render(raw, lines)


if __name__ == "__main__":
    main()
