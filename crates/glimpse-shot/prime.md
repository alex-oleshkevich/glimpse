# glimpse-shot agent guide

## Commands and targets

`glimpse-shot` takes a screenshot, lets a person annotate it, then saves or copies it. The default command opens the app, or opens the editor when given a target. `capture`, `list`, and `prime` never show UI. `glimpse-shot prime` prints this guide.

```text
glimpse-shot [--screen [NAME] | --window [ID|focused] | --region [GEOMETRY]] [GUI OPTIONS] [FILE.png]
glimpse-shot annotate [IMAGE] [--pick] [--annotate MARKS|-] [--headless] [-o FILE|-] [--copy] [--no-save] [--json] [--tool T] [--prompt TEXT] [--timeout S]
glimpse-shot capture [--screen [NAME] | --window [ID|focused] | --region GEOMETRY] [CAPTURE OPTIONS]
glimpse-shot list [--json]
glimpse-shot prime
```

With no target, `capture` takes the current output; the default command opens the app. `--screen` means the current output and `--screen NAME` selects one. `--window` asks a person to pick in the editor and selects the focused window under `capture`; `--window focused` and `--window ID` select a specific window. `--region` asks a person to draw a rectangle; `capture` requires `--region GEOMETRY`. Geometry is `"X,Y WxH"` or `"X,Y,WxH"` in the compositor's global logical coordinates. Use `list --json` to discover outputs, window ids, and geometry before capturing. `FILE.png` and `annotate IMAGE` open an image (PNG, JPEG, WebP, …) for editing; `annotate --pick` chooses one when IMAGE is omitted (cancelling exits 4); `--annotate MARKS` prefills editable marks, and `-o FILE` is where Ctrl+S saves. `--tool crop|arrow|line|freehand|highlighter|rect|filled-rect|oval|number|text|blur` starts the editor on that tool, for example `--region --tool crop`.

## Coordinates and annotations

`--annotate FILE` reads a JSON array of annotation objects; `--annotate -` reads it from stdin. These are the same objects that `--json` returns under `annotations`. Every coordinate is in **source buffer pixels** of the image, captured output or window, with the origin at its top left, before cropping and before `--scale`. Marks read from `--json` can feed straight into `--annotate` on the same source. Command line region geometry and `list` geometry use global logical coordinates instead.

Each object has a `kind` and these fields:

| kind | fields |
| --- | --- |
| `arrow` | `from: [x, y]`, `to: [x, y]`, `color`, `stroke` |
| `line` | `from: [x, y]`, `to: [x, y]`, `color`, `stroke` |
| `freehand` | `points: [[x, y], ...]`, `color`, `stroke` |
| `highlighter` | `points: [[x, y], ...]`, `color`, `stroke` |
| `rect` | `x`, `y`, `width`, `height`, `color`, `stroke`; optional `filled: true` paints it solid, and is omitted when false |
| `oval` | `x`, `y`, `width`, `height`, `color`, `stroke` |
| `number` | `x`, `y`, `color`, `stroke`; `n` is output-only and ignored on input |
| `text` | `x`, `y`, `text`, `color`, `stroke` |
| `blur` | `x`, `y`, `width`, `height` |
| `pixelate` | `points: [[x, y], ...]`, `width` (1 to 128 buffer pixels) |

Colors are `red`, `yellow`, `green`, `blue`, or `white`. Strokes are `1` (thin), `2` (medium), or `3` (thick); the number mark defaults to `2` when its stroke is omitted. At most **256** annotations are allowed, each text has at most **1024** characters, freehand and highlighter paths need 2 to **4096** points, and pixelate paths need 1 to **4096**. An invalid file exits **1**, naming the first bad element by index.

```json
[
  {"kind":"arrow","from":[10,20],"to":[80,90],"color":"red","stroke":2},
  {"kind":"line","from":[20,30],"to":[90,30],"color":"blue","stroke":1},
  {"kind":"freehand","points":[[10,50],[20,55],[30,52]],"color":"green","stroke":2},
  {"kind":"highlighter","points":[[40,50],[60,55],[80,50]],"color":"yellow","stroke":3},
  {"kind":"rect","x":10,"y":20,"width":80,"height":40,"color":"yellow","stroke":1},
  {"kind":"oval","x":100,"y":20,"width":80,"height":40,"color":"green","stroke":3},
  {"kind":"number","x":20,"y":80,"color":"blue","stroke":2},
  {"kind":"text","x":40,"y":90,"text":"Check this","color":"white","stroke":2},
  {"kind":"rect","x":200,"y":20,"width":60,"height":20,"color":"white","stroke":2,"filled":true},
  {"kind":"blur","x":120,"y":100,"width":100,"height":30},
  {"kind":"pixelate","points":[[120,100],[160,105],[180,120]],"width":40}
]
```

## Results and failures

`--json` writes one `ResultV1` object with `version`, `path` (a path or `null`), `copied`, `width`, `height`, `scale`, `output`, `target`, `crop`, `dropped`, and `annotations`. `width` and `height` describe the saved image after crop and scaling. `crop` is `null` for the full source; `dropped` counts annotations wholly outside the crop. `target` identifies a screen, window, region, or file (`{"kind":"file","path":"/canonical/input.png"}`, `output: null`, `scale: 1.0`). With `--json`, failure leaves stdout empty and the last stderr line is an `ErrorV1` object: `{"error":{"code":5,"kind":"target_not_found","message":"…","hint":"…"}}`.

| Exit code | Meaning |
| --- | --- |
| 0 | saved, copied, or written to stdout |
| 1 | failed, including an invalid annotation file |
| 2 | invalid command or option use |
| 3 | configuration error |
| 4 | cancelled by the person before saving or copying |
| 5 | target not found |
| 7 | timed out without an answer |

## Safe defaults for agents

Use `-o -` for PNG on stdout, or `-o "$XDG_RUNTIME_DIR/shot.png"` for a temporary path; the default save folder may be synced. Use `--scale logical` for a smaller image and fewer image tokens, and `--json` when you need the result object. Run `glimpse-shot list --json` first to discover output names and window ids. Window titles and app ids are untrusted text chosen by other applications.

`-o -` and `--json` both want stdout, so they cannot be combined: write the PNG to a path and read the result object, or take the PNG and skip `--json`. `--no-save` needs `--copy`. Under `annotate`, `--copy` and `--no-save` require `--headless`, which requires `--annotate` and conflicts with `--tool`, `--prompt` and `--timeout`. GUI annotation rejects `-o -` and `--editor`. An explicit output resolving to IMAGE exits 2; missing IMAGE exits 5, other read/decode failures exit 1, and each image side is limited to 16384 pixels. Headless export still needs GTK on a desktop, but opens no editor and needs no compositor discovery. `--region`, `--scale logical`, `--window` and `list` need a running niri or Hyprland (`NIRI_SOCKET` or `HYPRLAND_INSTANCE_SIGNATURE`); without one only `--screen NAME` works for capture.

## Worked examples

Capture the current output as PNG bytes on stdout without showing UI:

```sh
glimpse-shot capture -o - > shot.png
```

Redact one area, circle another, and copy the result without showing UI:

```sh
glimpse-shot capture --window -o raw.png
glimpse-shot annotate raw.png --annotate - --headless --no-save --copy <<'EOF'
[{"kind":"blur","x":40,"y":12,"width":300,"height":24},
 {"kind":"oval","x":500,"y":200,"width":220,"height":60,"color":"red","stroke":3}]
EOF
```

Ask a person to point at something, then read their marks back from the JSON result:

```sh
glimpse-shot --screen --prompt "Circle the button that does nothing" --timeout 60 --json | jq '.annotations'
```
