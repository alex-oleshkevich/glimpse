---
state: draft
---

# 005 — Wallpaper

Drawing a background on every output: sources, decoding, transitions and the overview backdrop.
`glimpse-wallpaper` is the artifact that renders it.

## Problem

The background must be on screen before anything else and must stay there while the panel restarts.
Decoding is the expensive part — a 4K JPEG resized and blurred is hundreds of milliseconds — and it
happens again on every theme change, every output hotplug and every step of a directory cycle.

Outputs make it harder. A monitor is announced before its geometry is known, so the naive
reconciliation either drops it or builds a surface with no size.

## Goals

- Draw a background on every output, correct across hotplug, scale change and geometry arrival.
- Follow `appearance.scheme` so light and dark wallpapers switch with the rest of the session.
- Pay the decode cost once per distinct result, not once per start.
- Change images without a visible flash, and without retaining the image that was replaced.
- Survive a dead daemon by continuing to draw the last image.

## Non-goals

- No backend connections. Theme and schedule arrive over the socket.
- No wallpaper management UI, no downloading, no collections.
- No animated or live effects. The background is a still image over a colour.
- No writing to the user's image files. The cache is derived data and lives under `XDG_CACHE_HOME`.

## Tech

### Sources

A single image, or a directory cycled on an interval, with an optional separate choice per theme
mode. `appearance.scheme` selects between the light and dark source; with the daemon down the last
known scheme keeps applying, and the last image keeps drawing.

`[wallpaper] color` is drawn whenever no image resolves — a missing file, an unreadable one, a
directory that turned out empty. There is always something on screen.

### Outputs

One layer surface per output on the background layer, anchored to all four edges, with
`exclusive_zone = -1` and no keyboard interaction.

**A monitor is announced before its geometry arrives.** An output that reports a zero size is not
skipped and not given a surface: it is recorded, and its `geometry` notification is subscribed to so
reconciliation runs again when the size lands. Skipping it means a hotplugged display with no
wallpaper; building a surface anyway means a surface with no size. This is the ordinary path on
hotplug, not an edge case.

**The decode target is `geometry × scale_factor`, never the logical size.** A 2× display decoded at
logical size is visibly soft, and the error is easy to miss because it only shows on scaled outputs.

Reconciliation is by output identity, not by index: an existing surface is reconfigured in place and
only a genuinely new output builds one.

### Decoding and the cache

Decode, resize and blur produce one result for a given set of inputs, so that result is cached on
disk under `$XDG_CACHE_HOME/glimpse/wallpaper/`. A hit skips all three stages, which is the
difference between a background at session start and a background a second later.

The cache key is a hash of **every input that changes the output pixels**:

| Component          | Why it is in the key                                                             |
| ------------------ | -------------------------------------------------------------------------------- |
| format version tag | lets the on-disk format change without ever reading a stale entry                |
| source path        |                                                                                  |
| size and mtime     | the source's own signature                                                       |
| fit mode           | a different fit crops differently                                                |
| blur radius        |                                                                                  |
| target size        | a cache keyed on the file alone returns the wrong size after a resolution change |

Entries are a one-line text header — magic, width, height, stride — followed by raw RGBA. The loader
rejects an entry whose pixel length disagrees with its header rather than trusting it.

Three rules the previous implementation arrived at the hard way, or failed to:

- **A watcher report beats the file signature.** When the change came from the watcher rather than
  from a poll, the cache read is skipped and the source is decoded again. Size and mtime collide on
  filesystems with coarse mtime granularity, and the watcher is the more reliable witness.
- **Writes are atomic** — write a temporary file in the same directory and rename over the target. A
  crash mid-write otherwise leaves a truncated entry that every later start has to detect and
  discard.
- **The cache is bounded and swept.** Every resolution change, fit change and blur change adds an
  entry rather than replacing one, so an unbounded cache grows for the life of the installation.

**Blur runs at reduced resolution.** Gaussian cost scales with radius times pixels, and the output is
blurred regardless, so image and radius are both divided by the same small factor before the blur
and the result is scaled back. **Backdrop textures are capped to fit within 1920×1080** for the same
reason: the backdrop is blurred and sits behind the overview, so full resolution is memory spent on
detail nobody sees.

**A slow format shows a preview first.** Where a format carries an embedded thumbnail — HEIC is the
one that matters — the thumbnail is decoded and displayed while the full decode runs, and is
discarded the moment the real image lands. A preview never replaces an image that is already drawn.

### Transitions

An image change crossfades rather than cutting. Two picture slots exist per surface: the incoming
image is loaded into the hidden one, and the transition is the switch between them.

Two lifetime rules make that safe, and both guard against asynchronous work landing out of order:

- **Every load carries a request id**, bumped on each reconfigure. A result whose id is stale is
  discarded. Without it a slow decode of the previous image lands after the new one and the wrong
  wallpaper is displayed.
- **The replaced slot is cleared once the transition has elapsed**, on a timer, and only if the id
  still matches. Skipping this retains two full-resolution textures per output for the life of the
  process, which is invisible in a short test and obvious after a day of directory cycling.

### Backdrop

`[backdrop]` is a second, blurred image drawn on its own surface for the compositor's overview to
show behind the workspaces. It falls back to the wallpaper's image when it has no path of its own.

Under niri this requires a layer rule so the overview composites it; that is a compositor
configuration note, not something the binary can arrange for itself.

## The binary

`glimpse-wallpaper` is the artifact. Everything above is what it renders; this is how it is invoked.

```
glimpse-wallpaper [OPTIONS]
```

No subcommands and no arguments. `--image` exists rather than a positional path so the flagless
invocation always means "use my configuration".

| Flag                    | Default                                  | Purpose                                           |
| ----------------------- | ---------------------------------------- | ------------------------------------------------- |
| `-c`, `--config <PATH>` | the layered stack                        | Use exactly this file                             |
| `--socket <PATH>`       | `$XDG_RUNTIME_DIR/glimpse/glimpsed.sock` | Daemon socket                                     |
| `-i`, `--image <PATH>`  | from config                              | Draw this image and ignore the configured source  |
| `--fit <MODE>`          | from config                              | `cover`, `contain`, `fill`                        |
| `--output <NAME>`       | all outputs                              | Restrict to one output; repeatable. Debugging aid |
| `--check-config`        | off                                      | Validate configuration, print problems, exit      |
| `--log <FILTER>`        | `info`                                   | `tracing-subscriber` filter                       |
| `-V`, `--version`       |                                          |                                                   |
| `-h`, `--help`          |                                          |                                                   |

`--fit` takes the same values as `[wallpaper] fit`. A flag that overrides a setting uses that
setting's vocabulary.

### Environment

| Variable                   | Use                             |
| -------------------------- | ------------------------------- |
| `WAYLAND_DISPLAY`          | required                        |
| `XDG_CACHE_HOME`           | decoded image cache             |
| `GLIMPSE_WALLPAPER_APP_ID` | override the GTK application id |

### Files

| Path                                       | Role                                                       |
| ------------------------------------------ | ---------------------------------------------------------- |
| `$XDG_CONFIG_HOME/glimpse/config.toml`     | the `[wallpaper]` and `[backdrop]` tables; schema in `010` |
| `$XDG_CACHE_HOME/glimpse/wallpaper/*.rgba` | decoded, resized and blurred results                       |

### Exit codes

| Code | Meaning                                                         |
| ---- | --------------------------------------------------------------- |
| 0    | clean exit                                                      |
| 1    | configuration invalid per `--check-config`, or image unreadable |
| 2    | usage error                                                     |
| 5    | no Wayland display, or no layer-shell support                   |

Invalid configuration is not an exit. It logs and falls back to defaults at startup, and is dropped
on reload — see `010_configuration.md`.

## Risks

- **Technical** — the cache is the difference between a fast start and a slow one, and it is also the
  component most able to serve a wrong or corrupt image. Its correctness rules are load-bearing.

## Changelog

- 2026-08-20 — created, split out of `001_architecture.md`.
- 2026-08-20 — configuration moved into the shared `config.toml` under `[wallpaper]`.
- 2026-08-20 — invalid configuration logs and falls back to defaults instead of exiting; exit 1 is now only `--check-config`.
- 2026-08-20 — owns `[backdrop]` alongside `[wallpaper]`, per `_old`.
- 2026-08-20 — renamed from `005_glimpse_wallpaper.md` and reorganised around wallpaper behaviour, with the binary and its flags as one section rather than the subject.
- 2026-08-20 — specified outputs, decoding and the cache, and transitions, from what `_old/glimpse-wallpaper` got right and wrong: deferred zero-geometry outputs, scale-aware decode targets, the cache key and its version tag, watcher-beats-signature, atomic and bounded cache writes, reduced-resolution blur, capped backdrop textures, preview-then-full decode, request ids and the deferred slot clear.
- 2026-08-20 — `--mode` becomes `--fit` and takes `cover`/`contain`/`fill`, matching `[wallpaper] fit`; it previously offered a vocabulary the config did not have.
- 2026-08-20 — dropped live effects entirely, with the `--effect` flag and the frame-budget rule that existed for them.
