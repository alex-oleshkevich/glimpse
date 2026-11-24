# Panel visibility: slide, pin, reveal

| | |
| --- | --- |
| Bead | none yet — tasks are sliced from §10 and carry `Source: docs/plans/panel-visibility.md §N` |
| Status | Draft, revision 3. Compositor integration is removed (user, 2026-11-24): every external trigger arrives through the CLI. The §11 resolutions are proposals and wait on the user. |
| Rule | Every claim is tagged **source** (local code at `75ec3f32`, cited as `file:line`), **measured** (run on this machine, 2026-11-24) or **documented** (upstream docs or source, URL given). |

## 1. Goal

A panel slides in from its own screen edge and slides back out to it. Each panel has a config
default, `visibility = "show" | "hide"`. The CLI controls the panel at runtime, and that CLI is the
only way anything outside the panel affects its visibility. glimpse binds to no compositor event for
this. A compositor keybind, a niri overview watcher or any other script drives the panel through
`glimpsectl`.

Two verb families:

- **Pin**, with `show`, `hide` and `toggle`. A pinned panel reserves its exclusive zone, as panels do
  today, and windows reflow around it.
- **Reveal**, with `reveal`, `reveal --for <ms>` and `conceal`. A revealed panel slides in over
  windows and reserves nothing, so windows never move. `conceal` hides it again `hide-delay-ms`
  later, unless the pointer is on it or one of its popovers is open.

Inside the panel, only its own surface can reveal it: the pointer on the edge (`edge-reveal = true`)
and an open popover.

The two driving cases:

```toml
# Hidden. A user script reveals it while the niri overview is open (recipe in §4.8).
[[panels]]
visibility = "hide"

# Autohide: hidden, revealed by the pointer at the edge, or by a keybind running
# `glimpsectl panel reveal --for 2000`
[[panels]]
visibility = "hide"
edge-reveal = true
hide-delay-ms = 1000
```

- **Who owns the state:** `App` owns everything the CLI sets: pins, held reveals and their timers.
  Each panel window owns its `Presence`, which combines that with its own pointer and popover holds
  and drives the slide, the zone and the input region. No new process, no new D-Bus interface and no
  compositor dependency.

```text
 glimpsectl panel show|hide|toggle|reveal|conceal ─► org.gtk.Actions "visibility" (sssu) ─► App (pins, held) ─► Panel
 pointer enter/leave on the panel surface ─────────────────────────────────────────────────────────────────────► Panel
 popover open/close (Catcher callback) ────────────────────────────────────────────────────────────────────────► Panel
 Panel: Presence (pure) ─► Slide + exclusive zone + input region + catcher clearance
 Panel ─► Output::Presence ─► App ─► "visibility" action state ─► glimpsectl panel list
```

## 2. Current state (tree at `75ec3f32`)

| What | Where |
| --- | --- |
| There is one window per `(config index, connector)`, keyed `Key { index, monitor }`. A reload reconfigures windows in place; a window is built or destroyed only when its key appears or disappears. Controllers are launched with `.detach()`. | `crates/glimpse-panel/src/app.rs:487-491`, `:588-637` (`:627`) |
| `reconcile_panels` runs after almost every `AppInput` and sends `Configure` to every window | `app.rs:261-267`, `:620-623` |
| Window init: layer shell, namespace `glimpse-panel`, `Layer::Top`, `KeyboardMode::None`, then `auto_exclusive_zone_enable()` **before** the single `present()` | `crates/glimpse-panel/src/components/panel.rs:175-200` |
| `apply` sets the anchors for each `Position`, the monitor, all four layer-shell margins, `bar.set_thickness(size)`, the catcher reconfigure and blur | `panel.rs:211-261` |
| `panel::Input` has only `Configure(Config)` and `ClosePopover`; `Output = ()` | `panel.rs:148-151`, `:162-166` |
| The panel window is a plain `gtk::Window`. It is transparent only with the `blurred` class; `.popover-catcher` uses `background: none` | `panel.rs:168-173`; `crates/glimpse-widgets/styles/glimpse.css:1655-1657`, `:332-334` |
| No hide, show or autohide code exists for the panel window | `app.rs:106` (hidden root only); `applet/catcher.rs:411-414`; the exec DOM `Revealer` at `applets/exec/dom.rs:775-776` |
| Config: `Panel { size, monitor, position, margin, left, center, right }` with `deny_unknown_fields` and kebab-case; `Position { Left, Top, Right, Bottom }`; re-exported from `schema/mod.rs` | `crates/glimpse-config/src/schema/panels.rs:4-64`; `schema/mod.rs:56` |
| A range is enforced by `deserialize_with`, not by `schemars(range)`; durations in milliseconds end in `_ms` | `crates/glimpse-config/src/schema/wallpaper.rs:22-25`, `:165-174` |
| Panel GActions: `close-popovers` (no parameter) and the read-only, disabled, stateful `bars` of type `a(uss)`. Runtime applets already use a parameter and state of different types: `(sv)` / `a{sv}`. | `app.rs:141-154`, `:498-515`; `applet/runtime.rs:279-295` |
| CLI: `PanelCommand { List { watch }, Reload, Popovers(..) }`. `proxy()` builds the action proxy, and `close-popovers` is the precedent for activation. `panel list` prints one row per process, with bars joined in `BARS` as `index:connector:edge`. `Refused` is the error for a command the target cannot take. | `crates/glimpsectl/src/cli.rs:589-605`; `commands/panel.rs:192-202`, `:316-319`, `:425-452`, `:574-605`, `:324-345`; `errors.rs:39` |
| Selector convention: `--output <connector>`, `--edge <position>` | `cli.rs:517-525` |
| One catcher per panel. Its guard surface (zone 0, all edges) and its popup (anchored to the bar edge, every margin reset to 0 in `layout()`) both rely on the panel's zone to sit past the bar. `open` returns early for a `Native` child. | `applet/catcher.rs:41`, `:144-160`, `:201`, `:242-260`, `:294-303`, `:411`, `:437-458` |
| Animation: `adw::TimedAnimation` follows `gtk-enable-animations`; `duration_ms()` and `animation_ms(base)` scale with `[appearance] animation-speed`, and 0 gives 0 ms | `crates/glimpse-widgets/src/theme.rs:18-45`; `applet/catcher.rs:58-70` |
| No glimpse layer surface animates its whole window | `crates/glimpse-agents/src/app.rs:591-615`; `crates/glimpse-notifications/src/app.rs:826-866` |
| Precedent for an input region: empty, set once on realize | `crates/glimpse-wallpaper/src/surface.rs:104-110` |
| A layer surface that paints nothing is never mapped | `crates/glimpse-panel/README.md`, "The popover" |
| glimpse code must not shell out to `niri msg`. A user script in the docs is not glimpse code. | `AGENTS.md`, "Hard rules" |

## 3. External facts

| Fact | Tag |
| --- | --- |
| The reserved zone is `margin.anchored + size + margin.opposite`. The compositor adds the anchored margin; gtk4-layer-shell's auto zone adds the opposite one. | **source** (upstream): gtk4-layer-shell `layer-surface.c:237-244`, smithay `desktop/wayland/layer.rs:371-373` (per review, §12) |
| niri has no layer open/close animation. Hyprland animates layers on map and unmap. | **documented** [niri animations](https://github.com/niri-wm/niri/wiki/Configuration:-Animations), [Hyprland animations](https://wiki.hypr.land/configuring/core/animations/) |
| A client sees pointer enter and leave only on its own surfaces, so edge reveal needs an input strip | **documented** [wlr-layer-shell](https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/raw/master/unstable/wlr-layer-shell-unstable-v1.xml) |
| For the docs recipe only: niri 26.04's event stream opens with `{"OverviewOpenedOrClosed":{"is_open":false}}` and repeats the event on every change. Top-layer surfaces stay above the overview. | **measured** (`niri msg --json event-stream`); **documented** [niri wiki: Overview](https://github.com/niri-wm/niri/wiki/Overview) |
| Whether a top-layer surface takes pointer input while the niri overview is open | **unknown**; AC-4 checks it live |
| Prior art. waybar: `start_hidden`, `mode = dock/hide/overlay`, SIGUSR1 toggle. ironbar: `start_hidden`, `autohide` in ms, IPC `show/hide/toggle_visible/get_visible`, no zone while hidden. | **documented** [waybar(5)](https://man.archlinux.org/man/extra/waybar/waybar.5.en), [ironbar IPC](https://github.com/JakeStanger/ironbar/wiki/IPC) |

## 4. Architecture

### 4.1 The surface stays mapped and the content slides

The window is presented once and never unmapped or resized for visibility. A new
`glimpse_widgets::Slide` sits between the window and the bar:

- It measures as its child.
- It allocates the child shifted toward the panel's edge by `(1 − progress) × extent`.
- It sets the child's opacity to `progress` and clips with `overflow: hidden`.
- At progress 0 it does not snapshot the child, so no `backdrop-filter` region is left behind. It
  paints a transparent node over its bounds instead: GTK skips the frame of a window whose snapshot
  is empty (`surface_render_cb` in `gtk/gtknative.c`), so without it the hidden surface never maps
  at startup and a changed input region is never committed (**measured**, `WAYLAND_DEBUG=client`).
- Because `Slide` always paints, the surface still maps while hidden.

The alternatives were rejected:

- **Animating the layer-shell margin.** The zone includes the margin, so every frame would reflow
  every tiled window and add a configure round trip.
- **A `Gtk.Revealer`.** It changes the size request, and with the auto zone that reflows every frame.
- **Unmapping.** It loses the edge strip, costs a configure before the first frame, and Hyprland
  would add its own layer animation on top of ours.

**The anchored-edge margin becomes a widget margin.** The opposite and along-axis margins stay
layer-shell margins. The catcher's `inset` reads the along-axis margins (`panel.rs:245-250`).
gtk4-layer-shell adds the opposite margin to the auto zone, and the compositor adds the anchored
margin, which is now 0. The reserved total `margin.anchored + size + margin.opposite` is therefore
unchanged. Content now slides off the real screen edge, and the edge strip sits on it.

**The window takes class `panel-window` with `background: none`**, as `.popover-catcher` does.
Without it, the band that stays mapped while hidden would paint the window background over other
windows.

### 4.2 The exclusive zone follows the pin

- Pinned: `auto_exclusive_zone_enable()`.
- Not pinned: `set_exclusive_zone(0)`.
- **Init:** the zone is set from the initial pin before `present()`. A panel that starts hidden
  never reserves space, not even for one commit.
- **Order:** pinning reserves first, then slides in. Unpinning slides out first, then releases.

### 4.3 Input region

| State | Region |
| --- | --- |
| Shown or sliding | the whole surface |
| Hidden, with `edge-reveal = true` | an `EDGE_STRIP` along the anchored edge, spanning the surface's along-axis extent |
| Hidden otherwise | empty |

The region is recomputed from the surface size in `gdk::Surface::connect_layout` (after realize), on
every `Step::Region` and on every `Configure`. It is never computed from `bar.compute_bounds`, since
the bar is translated mid-slide. With a whole-surface region, a pointer resting at y = 0 stays inside
the window after the reveal, so the panel cannot flap.

### 4.4 Presence: one pure state machine per window

`Presence` has no GTK, no timers and no clock. It takes events and returns `Step`s, and the panel
component executes them. It is tested in `presence/tests.rs`.

- **Holds** keep an unpinned panel revealed:
  - `Command` is set by `App`, from a CLI `reveal` until `conceal` or until the `--for` timer ends.
  - `Pointer` is the pointer on the panel surface.
  - `Popover` is an open popover.
- **Linger** is the `hide-delay-ms` timer. It starts when the last hold ends on an unpinned panel,
  and any new hold cancels it (`Settle`).
- **`expire()`** slides out only when the panel is unpinned and no hold is on.
- **`pin(false)`** drops every hold, so `hide` always hides. `App` closes the popover and clears its
  own held reveal for the key first.
- **A hold set while pinned** is recorded and has no visible effect. If the pin is then removed by
  config, the hold applies. If `hide` removes it, `hide` clears the hold.
- **`slid()`** emits `Release` if and only if the panel is neither pinned nor visible. It works from
  `(pinned, visible)` alone.
- **A superseded slide's `done` is dropped.** `Slide::reveal` called mid-animation replaces the
  callback.
- **Toggle is resolved in `App`**, not in the panel: `!pins.get(key).unwrap_or(visibility == Show)`.
  It flips the pin, not what is on screen, so toggling a revealed, unpinned panel pins it.

### 4.5 Pointer and popover holds

- An `EventControllerMotion` sits on the window.
  - `enter` while hidden starts a `DWELL = 150 ms` timer, and `leave` before it fires cancels it.
    When it fires, it turns `Hold::Pointer` on.
  - `enter` while shown turns `Hold::Pointer` on at once, so a pointer on a CLI-revealed panel keeps
    it up after `conceal`.
  - `leave` turns it off.
- `Catcher::new` takes an `on_open: Box<dyn Fn(bool)>`. It is called with `true` after `open`'s
  early return for a `Native` child (`catcher.rs:157-160`), and with `false` from `finish_close`
  (`:411`). The panel maps it to `Hold::Popover`.
- **Clearance.** `Catcher` stores `clearance: Cell<i32>`. `layout()` applies it as the bar-edge
  margin of the guard and the popup.
  - While pinned it is 0.
  - Otherwise it is the panel window's measured extent along the axis plus `margin.opposite`,
    re-applied on resize.
  - It is set through `Catcher::set_clearance` and survives every `reconfigure`.

  This keeps the popup past the bar, and leaves the bar clickable while a popover is open in
  overlay mode.

### 4.6 App owns everything the CLI sets

- **`App.pins: HashMap<Key, bool>`** is written by `show`, `hide` and `toggle`.
- **`App.held: HashMap<Key, Option<glib::SourceId>>`**: an entry means `Hold::Command` is on for that
  window.
  - `Some(id)` is a `--for` timer that removes the entry when it fires.
  - A new `reveal` for the key cancels the old timer.
  - `conceal` and `hide` remove the entry and cancel its timer.
- **Into the window config.** A new or reconfigured window gets `pinned = pins.get(key)`, or failing
  that `visibility == Show` from config, and `held = held.contains_key(key)`. A hotplugged window
  and a reloaded one both see the current CLI state.
- **Reloads.** A reload that changes `config.panels` drops every pin and every held reveal, because
  keys are index-based and a reorder would move them onto the wrong panel. Other reloads keep them.
- Nothing is persisted across a panel restart.

### 4.7 CLI

```text
glimpsectl panel show    [--output <connector>] [--edge <position>]
glimpsectl panel hide    [--output <connector>] [--edge <position>]
glimpsectl panel toggle  [--output <connector>] [--edge <position>]
glimpsectl panel reveal  [--for <ms>] [--output <connector>] [--edge <position>]
glimpsectl panel conceal [--output <connector>] [--edge <position>]
```

With no selector, a command applies to every panel window.

1. The CLI reads `bars` and the `visibility` action description.
2. If nothing matches, it exits with `Refused("no panel matches --output DP-3")`, because an
   `org.gtk.Actions` activation has no reply to carry that error.
3. If the panel predates the `visibility` action, it exits with `Refused("this panel cannot hide")`.
4. Otherwise it activates `visibility` with `(verb, connector, edge, for_ms)`, where `""` means any
   and `0` means until `conceal`.

`panel list` extends each `BARS` entry to `index:connector:edge:state`. The state is `shown`,
`revealed` or `hidden`, or `unknown` against an older panel.

### 4.8 Recipes, documentation only

These ship in `website/config/panels.md`. They are user configuration, not glimpse code.

```kdl
// niri: peek at the panel for 2 s, or pin and unpin it
binds {
    Mod+B { spawn "glimpsectl" "panel" "reveal" "--for" "2000"; }
    Mod+Shift+B { spawn "glimpsectl" "panel" "toggle"; }
}
```

```sh
# Show the panel while the niri overview is open. Run it from spawn-at-startup or a user unit.
niri msg --json event-stream \
  | jq --unbuffered -r '.OverviewOpenedOrClosed.is_open // empty' \
  | while read -r open; do
      if [ "$open" = true ]; then glimpsectl panel reveal; else glimpsectl panel conceal; fi
    done
```

The event stream repeats the current state on start (§3), so a panel revealed by an earlier run is
concealed again on restart.

## 5. Mockup

A top panel with `visibility = "hide"` and `edge-reveal = true`, at 0, 50 and 100 % `progress`. `·`
marks the 2 px input strip, which is not visible on screen.

```text
 hidden                     sliding (50 %, half opacity)     revealed (overlay, no zone)
┌────────────────────────┐ ┌────────────────────────┐       ┌────────────────────────┐
│························│ │ ░░ 12:04 ░░░░ ◐ ▮ ⏻ ░░│       │ ▤ 1 2 3   12:04  ◐ ▮ ⏻ │
│ window                 │ │ window                 │       │ window (not moved)     │
```

Before the plan is final, a live capture from a nested niri replaces this (`docs/live-testing.md`).

## 6. Flows

**F1: `glimpsectl panel hide` on a pinned top panel with no holds.**
1. The CLI reads `bars`, finds a match and calls `Activate("visibility", ("hide","","",0))`.
2. `AppInput::Visibility` reaches `App`, which for each match sets `pins[key] = false`, removes
   `held[key]`, sends `ClosePopover` and then `Configure` with `pinned = false, held = false`.
3. `Presence::pin(false)` returns `[SlideOut]`, and the panel calls `Slide::reveal(false, done)`
   (`animation_ms(200)`, EaseInCubic).
4. `done` calls `Presence::slid()`, which returns `[Release, Region]`: `set_exclusive_zone(0)`, a
   clearance equal to the extent, and an input region that is the strip or empty.
5. The panel emits `Output::Presence`, and `App` updates the `visibility` state.

**F2: hide while a popover is open.** `pin(false)` drops `Popover` itself, so the slide-out starts at
once while the catcher fades. F1 follows from step 3.

**F3: a script reveals the panel during the niri overview.**
1. `glimpsectl panel reveal` makes `App` insert `held[key] = None` and send `Configure` with
   `held = true`.
2. `hold(Command, true)` returns `[Settle, SlideIn, Region]`. The region becomes the whole surface,
   and the zone stays 0.
3. `glimpsectl panel conceal` removes the entry. `hold(Command, false)` returns `[Linger]`.
4. After `hide-delay-ms`, `expire()` returns `[SlideOut]`, and `slid()` returns `[Region]`. There is
   no `Release`, because the zone is already 0.

**F4: `glimpsectl panel reveal --for 2000` from a keybind.**
1. `App` inserts `held[key] = Some(timeout_add_local_once(2000))` and sends F3's steps.
2. When the timer fires, `App` removes the entry and sends `Configure` with `held = false`, then F3
   continues from step 3.
3. A pointer `enter` in the meantime turns `Pointer` on, and the panel stays until `leave`.
4. A second `reveal --for` restarts the timer.

**F5: edge reveal.**
1. The pointer enters the strip. After `DWELL`, `hold(Pointer, true)` returns `[SlideIn, Region]`.
2. The pointer moves onto the bar, still inside the window, and opens a popover:
   `hold(Popover, true)`.
3. The pointer moves into the popup surface. The window's `leave` gives `hold(Pointer, false)`, and
   `Popover` keeps the panel up.
4. The popover closes: `hold(Popover, false)` returns `[Linger]`.

**F6: a reload changes `visibility` from `hide` to `show`.**
1. `ConfigChanged` drops the pins and the held reveals, because `panels` changed.
2. `reconcile_panels` sends `Configure` with `pinned = true`.
3. `Presence` gets `pin(true)` and returns `[Reserve, SlideIn, Region]`.

**F7: monitor hotplug.**
1. A new `Key` builds a window, and the zone is set from `pinned`.
2. `Presence::new(pinned)` runs and `held` is applied.
3. `Slide::jump(visible)` sets the start state. Nothing animates on the first map.

A wildcard `reveal` that arrived before the plug does not cover the new connector, because `App`
records it only for the keys that existed.

**F8: interrupted slides.**
- `reveal` arrives during a slide-out: `SlideIn` animates from the current progress, and the old
  `done` is dropped.
- `hide` arrives during a slide-in: `SlideOut` runs, then `slid()` returns `Release`.

## 7. Types and config

Comments in these sketches are for the plan only. The code carries none (handbook).

```rust
// crates/glimpse-config/src/schema/panels.rs: fields added to `Panel`, with defaults in `Panel::default()`
/// Whether the panel is shown when it starts. `glimpsectl panel show|hide|toggle` changes it until
/// the panel restarts or the panel configuration changes.
pub visibility: Visibility,
/// Reveal a hidden panel while the pointer rests on its screen edge.
pub edge_reveal: bool,
/// Milliseconds a revealed panel stays after the last reason to show it ends.
#[serde(deserialize_with = "hide_delay_ms")]
#[schemars(range(max = 60000))]
pub hide_delay_ms: u32,

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Visibility {
    #[default]
    Show,
    Hide,
}
// schema/mod.rs:56 → pub use panels::{Margin, Panel, Position, Visibility};
// defaults: visibility Show, edge_reveal false, hide_delay_ms 1000
```

```rust
// crates/glimpse-panel/src/components/panel/presence.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold { Command, Pointer, Popover }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step { Reserve, Release, SlideIn, SlideOut, Linger, Settle, Region }

pub struct Presence { pinned: bool, holds: [bool; 3], visible: bool }

impl Presence {
    pub fn new(pinned: bool) -> Self;
    pub fn pin(&mut self, pinned: bool) -> Vec<Step>;
    pub fn hold(&mut self, hold: Hold, on: bool) -> Vec<Step>;
    pub fn expire(&mut self) -> Vec<Step>;
    pub fn slid(&mut self) -> Vec<Step>;
    pub fn pinned(&self) -> bool;
    pub fn visible(&self) -> bool;
}
```

```rust
// crates/glimpse-panel/src/components/panel.rs
const DWELL: Duration = Duration::from_millis(150);
const EDGE_STRIP: i32 = 2;

pub struct Config {
    // existing fields, plus:
    pub pinned: bool,
    pub held: bool,
    pub edge_reveal: bool,
    pub hide_delay: Duration,
}

pub enum Input { Configure(Config), ClosePopover }

pub enum Output { Presence { pinned: bool, visible: bool } }
```

Pin and held changes arrive as `Configure`, since `reconcile_panels` already sends it to every
window after each `AppInput`. The panel compares the new values with `Presence::pinned()` and its
current `Command` hold and steps only on a change.

```rust
// crates/glimpse-panel/src/app.rs
pub enum AppInput {
    // existing variants, plus:
    Visibility { verb: Verb, connector: String, edge: String, for_ms: u32 },
    RevealExpired(Key),
    Presence { key: Key, pinned: bool, visible: bool },
}
pub enum Verb { Show, Hide, Toggle, Reveal, Conceal }
pub struct App {
    // existing fields, plus:
    pins: HashMap<Key, bool>,
    held: HashMap<Key, Option<glib::SourceId>>,
    presence: HashMap<Key, (bool, bool)>,
}
// Panel controllers: `.launch(cfg).forward(sender.input_sender(), move |Output::Presence { .. }| AppInput::Presence { key, .. })`
```

```rust
// crates/glimpse-panel/src/applet/catcher.rs
pub fn new(monitor: Option<&gdk::Monitor>, side: Position, on_open: Box<dyn Fn(bool)>) -> Rc<Self>;
pub fn set_clearance(&self, pixels: i32);
```

```rust
// crates/glimpse-widgets/src/slide/mod.rs
impl Slide {
    pub fn new() -> Self;
    pub fn set_child(&self, child: Option<&impl IsA<gtk4::Widget>>);
    pub fn set_edge(&self, edge: gtk4::PositionType);
    pub fn reveal(&self, shown: bool, done: impl FnOnce() + 'static);
    pub fn jump(&self, shown: bool);
}
```

| GAction | Parameter | State | Notes |
| --- | --- | --- | --- |
| `visibility` | `(sssu)`: verb `show\|hide\|toggle\|reveal\|conceal`, connector or `""`, edge or `""`, `for` in ms or `0` | `a(ussbb)`: index, connector, edge, pinned, visible | Enabled. `connect_change_state(\|_, _\| {})`, so `SetState` from the bus does nothing. An unknown verb is logged and ignored. `for` is read only by `reveal`. |

```rust
// crates/glimpsectl/src/cli.rs: PanelCommand gains
#[command(about = "Show the selected panels and reserve their space")]
Show(PanelTarget),
#[command(about = "Hide the selected panels")]
Hide(PanelTarget),
#[command(about = "Show hidden panels, or hide shown ones")]
Toggle(PanelTarget),
#[command(about = "Slide the selected panels in over windows")]
Reveal(RevealArgs),
#[command(about = "End a reveal")]
Conceal(PanelTarget),

#[derive(Debug, Args)]
pub struct PanelTarget {
    #[arg(long, help = "Output connector")]
    pub output: Option<String>,
    #[arg(long, help = "Panel edge")]
    pub edge: Option<Position>,
}

#[derive(Debug, Args)]
pub struct RevealArgs {
    #[arg(long = "for", value_name = "MS", help = "Conceal after this many milliseconds")]
    pub duration: Option<u32>,
    #[command(flatten)]
    pub target: PanelTarget,
}
```

## 8. How the pieces connect

- **CLI to App.** A `PanelTarget` or `RevealArgs` becomes the `visibility` parameter `(sssu)` and
  arrives as `AppInput::Visibility`. `App` matches it against each `Key` and
  `config.panels[key.index].position`, then updates its state:
  - `Show` and `Hide` write `App.pins`, and `Hide` also removes `App.held` for the key.
  - `Toggle` is resolved from `pins` and config.
  - `Reveal` inserts into `App.held`, with a `timeout_add_local_once` that sends
    `AppInput::RevealExpired(key)` when `for_ms > 0`.
  - `Conceal` and `RevealExpired` remove the entry.
- **App to windows.** `reconcile_panels` (`app.rs:606-619`) fills `panel::Config.pinned` and
  `panel::Config.held` from `App.pins`, `App.held` and `glimpse_config::Panel.visibility`. `Hide`
  sends `ClosePopover` to the matched windows before the `Configure`.
- **Configure to Presence.** On `Configure`, the panel calls `Presence::pin` when `pinned` differs
  from `Presence::pinned()`, and `Presence::hold(Hold::Command, held)` when `held` differs from the
  current hold.
- **Steps to GTK.** The `Presence` steps drive `Slide::reveal`, `auto_exclusive_zone_enable` or
  `set_exclusive_zone(0)`, `Catcher::set_clearance`, `gdk::Surface::set_input_region`, and the
  linger and dwell `glib::SourceId` timers.
- **State back to the CLI.** The panel emits `Output::Presence`, which arrives as
  `AppInput::Presence`. `App` updates `App.presence` and sets the `visibility` action state, which
  `glimpsectl panel list` reads.

## 9. Acceptance criteria

```
AC-1  As a user, I want a panel that starts hidden, so that my screen starts uncluttered.
      GIVEN visibility = "hide" WHEN the panel starts THEN nothing is drawn, windows start at y = 0
      with no intermediate reflow, clicks at y = 5 reach the window below, and with edge-reveal =
      true the surface is mapped (the strip reveals it).
      Verify by: live run in a nested niri (docs/live-testing.md): screenshot, click, edge dwell.

AC-2  As a user, I want to pin and unpin from the CLI, with the panel sliding to and from its edge.
      GIVEN a pinned top panel WHEN `glimpsectl panel hide` THEN the bar slides up and fades in
      about 200 ms, and windows then grow into the space. `show` reserves the space first and then
      slides down. Bottom slides down, left slides left, right slides right.
      Verify by: a live capture for each edge; Presence unit tests for the step order.

AC-3  As a keyboard user, I want `toggle` and a timed `reveal` to bind to keys.
      GIVEN a hidden panel WHEN `toggle` THEN it is pinned, and a second `toggle` hides it.
      WHEN `reveal --for 2000` THEN it slides in over windows, without moving them, and slides out
      2000 ms + hide-delay-ms later. A second `reveal --for` restarts the countdown.
      Verify by: App unit tests for toggle resolution and the held map; Presence unit tests; live run.

AC-4  As a niri user, I want my own script to show the panel during the overview.
      GIVEN the §4.8 script WHEN the overview opens THEN the bar slides in over the overview and its
      applets take clicks. WHEN the overview closes THEN the bar leaves after hide-delay-ms.
      Verify by: live run in a nested niri with the recipe.

AC-5  As a mouse user with edge-reveal = true, I want the panel when I push the pointer to its edge.
      GIVEN a hidden panel WHEN the pointer rests on the edge for at least 150 ms THEN it slides in.
      WHEN the pointer only crosses the edge THEN nothing happens. WHEN the pointer rests at y = 0
      after the reveal THEN the panel stays. WHEN the pointer leaves THEN the bar slides out after
      hide-delay-ms.
      Verify by: live run (wayland pointer move and dwell).

AC-6  As a user, I want an open popover to keep a revealed panel up and to sit below the bar.
      GIVEN a revealed, unpinned panel WHEN I open a popover and run `conceal` THEN the bar stays,
      the popover sits past the bar, and clicking another applet on the bar switches popovers.
      WHEN the popover closes THEN the linger starts.
      Verify by: live screenshot; Presence unit test.

AC-7  Error path: GIVEN no panel on DP-3 WHEN `glimpsectl panel hide --output DP-3` THEN the exit
      status is non-zero, stderr names the selector, and no panel changes. GIVEN a panel without the
      `visibility` action THEN `hide` is Refused and `list` prints `unknown`.
      Verify by: glimpsectl tests on a decoded `bars` / action description.

AC-8  Error path: GIVEN visibility = "hidden" (a typo) or hide-delay-ms = 90000 WHEN the config
      loads THEN it is rejected with the field named, and the running panel keeps its previous
      config.
      Verify by: glimpse-config parse tests.

AC-9  Regression: GIVEN visibility = "show", margin.top = 8 and margin.bottom = 4, with blur on and
      with blur off, WHEN the panel starts THEN the bar and the windows' offset are pixel-identical
      to the build before this change.
      Verify by: live baseline_compare against a capture taken on the old build.

AC-10 Reduced motion: GIVEN animation-speed = 0 or gtk-enable-animations = false WHEN any slide
      runs THEN the state switches in one frame and every `done` still runs.
      Verify by: Slide GTK test (#[ignore], one per process).

AC-11 Robustness: GIVEN a held reveal WHEN `hide` THEN the hold is gone and a later `show`/`hide`
      cycle does not bring the reveal back. GIVEN a popover is open WHEN `hide` THEN the popover
      closes and the slide-out starts at once. GIVEN a held reveal WHEN a monitor is unplugged and
      replugged THEN the replugged window is revealed.
      Verify by: Presence and App unit tests; live run.
```

## 10. Slices

| # | Slice | Files | Depends |
| --- | --- | --- | --- |
| S1 | Config: `visibility`, `edge-reveal`, `hide-delay-ms`, plus parse tests; then `just gen-config-default panel`, `just gen-config-schema panel`, `just gen-config-commented panel` | `glimpse-config/src/schema/panels.rs`, `schema/mod.rs`, `data/panel.*` | — |
| S2 | `Slide` widget and its ignored GTK test | `glimpse-widgets/src/slide/`, `lib.rs` | — |
| S3 | `Presence` and its unit tests, covering F1–F8 | `glimpse-panel/src/components/panel/presence.rs`, `presence/tests.rs` | — |
| S4 | Panel component: Slide wiring, inner margin, the `panel-window` class, zone before present, input region, dwell, linger, `Output`; catcher `on_open` and clearance | `components/panel.rs`, `applet/catcher.rs`, `glimpse-widgets/styles/glimpse.css` | S1–S3 |
| S5 | App: the `visibility` action, `Verb`, pins, held reveals and their timers, forwarded controllers | `app.rs` | S4 |
| S6 | CLI: `show/hide/toggle/reveal/conceal`, `PanelTarget`, `RevealArgs`, the `BARS` state suffix, Refused paths; then `just docs-gen-cli` | `glimpsectl/src/cli.rs`, `commands/panel.rs`, `website/config/cli/glimpsectl.md` | S5 |
| S7 | User docs: a "Hiding the panel" section in `panels.md` with the §1 configs and the §4.8 recipes, noting that compositor layer rules (Hyprland `layerrule blur`, niri shadow) cover the whole surface; panel README rule lines for §4.1–4.3 (new rules, replacing nothing) | `website/config/panels.md`, `crates/glimpse-panel/README.md` | S4–S6 |
| S8 | Live acceptance for AC-1…AC-6, AC-9 and AC-11 in a nested niri | — | all |

## 11. Open questions

1. ~~Does a revealed panel reserve space?~~ **No.** It overlays, as in ironbar and waybar `overlay`.
   Only pinning reflows windows (§4.2). *Proposed; awaiting the user.*
2. ~~Does `toggle` flip the pin or what is on screen?~~ **The pin** (§4.4). *Proposed.*
3. ~~Do CLI changes survive a panel restart or a `panels` reload?~~ **No to both** (§4.6).
   *Proposed.*
4. ~~Should a wildcard `reveal` cover a monitor plugged in later?~~ **No.** It covers the windows
   that existed when it arrived (F7). *Proposed.*
5. ~~Make the dwell and strip size configurable?~~ **No.** They are constants: `DWELL = 150 ms`,
   `EDGE_STRIP = 2 px`. *Proposed.*
6. ~~Slide timing~~ **`animation_ms(200)`**, EaseOutCubic in and EaseInCubic out, with opacity
   following progress. *Proposed.*
7. ~~Add a signal such as SIGUSR1, as waybar does?~~ **No.** SIGHUP is reload, and the CLI is the
   one control path. *Proposed.*
8. ~~Does the inner margin change the `.panel` box-shadow on the anchored side?~~ AC-9 decides. If
   it differs, `Slide` clips to the child's margin box. *Open until S8.*

## 12. Review record

| When | Who | Result |
| --- | --- | --- |
| 2026-11-24 | haiku ×5 | Research: panel window, CLI/IPC, compositor events, animation, prior art |
| 2026-11-24 | opus (`PlanReview`) | 7 blocking findings, all applied in revision 2: opaque window background, clearance wiped by `layout()`, `title_only`, input region flap and translated bounds, overview hold stuck, F2 contradiction, split toggle ownership. Non-blocking findings applied as well. |
| 2026-11-24 | user | Revision 3: no compositor binding. The overview chain (old S1), `Hold::Overview` and `reveal = [..]` are removed. CLI `reveal`/`conceal`/`--for` and `Hold::Command` are added, with App-owned held reveals. `edge-reveal` becomes a bool. Review findings tied to the overview (`title_only`, the provider version skew, `FakeNiri` fixtures) no longer apply. |

## 13. Struck

- **Any compositor integration (niri overview, urgency, fullscreen, workspaces)** (user,
  2026-11-24). Scripts reach the panel through the CLI instead (§4.8).
- **Animating the layer-shell margin, or unmapping the window.** Either would reflow windows every
  frame, and on Hyprland our animation would run on top of its own (§4.1).
- **`visibility = "autohide"` as a third mode.** It is `hide` plus `edge-reveal = true`: one
  concept, not two.
- **A `exclusive = false` knob per panel.** The pin decides the zone (§4.2).
- **A D-Bus interface for the panel.** The panel's control surface is GActions (`app.rs:141-154`).
- **A signal such as waybar's SIGUSR1** (§11 Q7).

## 14. Beyond v1: candidates in this area

All of these stay CLI-driven or panel-internal. None of them is in the slices above.

| Candidate | What | Cost |
| --- | --- | --- |
| Scripts follow visibility | `glimpsectl panel list --watch` streams state changes, e.g. for an indicator or for a script that chains actions | small |
| Peek handle | A thin visible line while hidden, so the panel can be found | small |
| Exec applet `reveal` request | An applet asks its own panel to reveal, e.g. when a timer ends or a build fails. This is panel-internal, not a compositor binding. | medium |
| More recipes | Urgency, fullscreen or empty-workspace scripts built on `niri msg event-stream` or `hyprctl` events driving `reveal`/`conceal`/`hide` | docs only |
| Hold to peek | `reveal` on key press and `conceal` on release, once niri ships `release` binds (`docs/plans/win-board-research.md:24-28`), or with Hyprland's `bindr` today | docs only |
