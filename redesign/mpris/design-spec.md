# MPRIS Applet Redesign — macOS Now Playing

## Goal

Rework the mpris popover into a calmer, macOS-leaning Now Playing surface.
Keep the existing two-tier structure — one **main player** plus a list of
**secondary players** — but redesign each tier with tighter typography,
smaller artwork, an interactive scrubber, and a full-width transport row.

## Why change

The current main card is bulky (96×96 art, three-column inner layout) and
its progress bar is non-interactive — a glaring miss for a media surface.
The secondary rows duplicate the main card's visual weight with their own
art, status text, and play button, making the popover feel repetitive.

## Constraints

- Match the column width of the audio/network/battery redesigns
  (`--popover-xlarge-width`).
- Preserve all behavior of the current applet: prev / play-pause / next,
  raise on click, multi-player support, artwork or fallback icon,
  position/length display, panel label/tooltip via existing format strings,
  filter regex, hide-when-empty.
- Add: seek by clicking the scrubber (new MPRIS `SetPosition` command,
  gated on `can_seek`).
- Reuse existing widget primitives where the new media widgets can wrap
  them (`popover_shell`, `tile`, `row`).
- Empty state stays: "No media playing" + subtitle.

## ASCII mockup — main player

Main player is a compact card. Artwork is 48×48 (rectangle, rounded
corners). Title is bold; artist sits underneath in muted color. The
progress bar spans full width; current position and total length sit on
their own row below it with the times pushed to the edges. Transport
controls take their own full-width row at the bottom, centered.

```text
+----------------------------------------------------+
|                                                    |
|  [art ]   Bohemian Rhapsody                        |
|  48×48    Queen — A Night at the Opera             |
|                                                    |
|  ──────────●─────────────────────────────────      |
|  1:24                                       5:55   |
|                                                    |
|            «          ▶          »                 |
|                                                    |
+----------------------------------------------------+
```

## ASCII mockup — secondary players

Each secondary player is a single 40px row: artwork, title with subtitle
underneath, and a two-button transport on the trailing edge (play-pause +
next). Prev is dropped in this tier to keep the row uncluttered; previous
is still available via raise → app window.

```text
+----------------------------------------------------+
|  Other players                                     |
|                                                    |
|  [art]  Podcast Title                    ▶    »    |
|         Show Name                                  |
|                                                    |
|  [art]  Firefox Tab                      ❚❚   »    |
|         youtube.com — Some Video                   |
+----------------------------------------------------+
```

## ASCII mockup — empty

```text
+----------------------------------------------------+
|                                                    |
|              ♪                                     |
|        No media playing                            |
|        Start playback in any MPRIS player          |
|                                                    |
+----------------------------------------------------+
```

## Component layout

```
PopoverShell
└── content
    ├── NowPlayingCard               (main player; hidden when none)
    │   ├── row 1
    │   │   ├── MediaArtwork (48×48)
    │   │   └── MediaMeta    (title bold, subtitle muted)
    │   ├── MediaScrubber            (full-width interactive bar)
    │   ├── ScrubberTimes            (position left, length right)
    │   └── MediaTransport           (prev / play-pause / next, centered, full width)
    ├── SectionHeader "Other players" (hidden when list empty)
    ├── secondary list
    │   └── SecondaryPlayerRow ×N
    │       ├── MediaArtwork (40×40)
    │       ├── MediaMeta
    │       └── trailing buttons (play-pause, next)
    └── EmptyState                    (shown when no players visible)
```

## Proposed widgets (in `glimpse-shell/src/widgets/`)

The current `applets/mpris/components/` widgets are tightly coupled to
mpris-specific types. The rework extracts visual atoms into reusable
widgets in `widgets/`, leaving the applet to wire mpris state into them.

| Widget               | Role                                                                  | Built on                                         |
|----------------------|-----------------------------------------------------------------------|--------------------------------------------------|
| `media_artwork`      | Square, rounded, clipped picture with fallback icon and sizing API    | `gtk::Box` + `gtk::Picture` + `gtk::Image`       |
| `media_meta`         | Title (bold) + subtitle (muted) label pair with ellipsize rules       | `gtk::Box` + 2× `gtk::Label`                     |
| `media_scrubber`     | Interactive position bar with `seek-requested` signal                 | `gtk::Scale`                                     |
| `scrubber_times`     | Two tabular-num labels (position left, length right) on one row       | `gtk::Box` + 2× `gtk::Label`                     |
| `media_transport`    | Prev / play-pause / next cluster, centered, with one enlarged center button and per-button enable flags | `gtk::Box` + 3× `gtk::Button` |
| `now_playing_card`   | Composes header row (artwork + meta), scrubber, times, and transport  | `gtk::Box` (template only — no signals)          |
| `secondary_player_row` | One-line row: artwork + meta + trailing play-pause + next            | `gtk::Box` (template only)                       |

All widgets follow the existing `widgets/` pattern: `mod.rs` exposing a
`GObject` subclass, `imp.rs` for state, and a `template.blp` for layout.
None of them know about MPRIS types — they take primitive properties
(strings, fractions, durations, icon names, paintables).

### Why split scrubber and times into separate widgets

In a `gtk::Scale` the value labels sit *next to the handle*, not at the
ends of the track. The mockup wants times pinned to the track's edges, so
the cleanest split is: `media_scrubber` owns the bar + seek interaction,
`scrubber_times` owns a `0%`/`100%`-aligned label pair. The
`now_playing_card` stacks them so they appear as one element.

### Signal surface (Rust-side)

```text
MediaScrubber:
  seek-requested(fraction: f64)

MediaTransport:
  previous-clicked()
  play-pause-clicked()
  next-clicked()

SecondaryPlayerRow:
  play-pause-clicked()
  next-clicked()
  activated()              // emitted by click on artwork/meta — raise
```

### Properties

```text
MediaArtwork:
  paintable: Option<gdk::Paintable>
  fallback-icon-name: String
  size: u32                          // pixel side (48 for main, 40 for row)

MediaMeta:
  title: String
  subtitle: String
  title-bold: bool                   // main = true, row = true (subtitle muted)

MediaScrubber:
  position-seconds: f64
  length-seconds: f64
  seekable: bool

ScrubberTimes:
  position-text: String
  length-text: String

MediaTransport:
  status: enum { Playing, Paused, Stopped }
  can-prev / can-play-pause / can-next: bool

SecondaryPlayerRow:
  (composes MediaArtwork + MediaMeta + two buttons; exposes the union of
  their properties as its own GObject props)
```

## Interactions

- **Click scrubber** → `seek-requested(fraction)` → applet sends MPRIS
  `SetPosition` (new command, gated on `can_seek`).
- **Click art / meta** (main or secondary row) → existing "raise" behavior.
- **Click prev / play-pause / next on main** → existing transport commands.
- **Click play-pause / next on secondary row** → existing transport
  commands targeting that row's `player_id`.

## Behavior parity checklist

- [x] Panel label + tooltip use `format::label` / `format::tooltip`.
- [x] Hide-when-empty preserved.
- [x] Filter regex preserved.
- [x] Artwork loading via `Artwork::FilePath` / `FileUri` only.
- [x] Disabled state for prev/next when `can_go_*` is false.
- [x] Play-pause icon swaps with `PlaybackStatus`.
- [x] Raise on art / meta click when `can_raise`.
- [ ] **Added:** seek when `can_seek` (new MPRIS command).
- [ ] **Changed:** secondary row drops "prev"; gains "next".
- [ ] **Changed:** main player position/length move below scrubber.

## Open questions

1. **Scrubber animation** — interpolate position client-side between
   service ticks, or render only on service updates? Recommend client-side
   interpolation at 1 Hz (cheap, prevents visible jumps), guarded by
   `playback_status == Playing`.
2. **Status text on secondary rows** — keep "Playing"/"Paused" label, or
   drop it now that there's a visible play/pause icon on each row? Recommend
   dropping; the icon already encodes state.
3. **Secondary row "next" when `can_go_next == false`** — render disabled,
   or hide entirely? Recommend disabled (matches main player's prev/next).

## Out of scope

- Lyrics, queue, like/dislike, output device picker, casting.
- New MPRIS service capabilities beyond `SetPosition`.
- Panel-side changes (label format, indicator).
- Theming changes outside the new `.mpris-*` selectors.
