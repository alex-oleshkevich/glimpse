# The popover

An applet's second surface.

**Verified against `applet/popover.rs`, `applet/catcher.rs`, `applet/runtime.rs`.**
When this file and the code disagree, the code is right.

## What opens it, and what closes it

The runtime does. `HostInput::Pressed` delivers the press to `handle` **and then**, for
`Button::Left`, calls `show_popover` — which calls `applet.popover(&seat)` if nothing of this
applet's is open, and `catcher.close()` if something is. Left click is therefore a toggle the applet
does not implement and must not duplicate: an applet that also acts on
`Input::Pointer(Pointer::Press(Button::Left))` fires its action every time the popover opens.

`Opener::open_popover` and `Opener::close_popover` exist so an applet can drive that itself. Neither
has a caller yet; both are `#[allow(dead_code)]` with the reason on them.

Dismissal is a click anywhere outside the popover body, caught by a `GestureClick` on the catcher
window. It reaches the runtime as `HostInput::PopoverDismissed`, which drops the runtime's handle.

## Where it opens

**One `Catcher` per bar**, built in `Panel::init` and shared by every applet in every zone. So one
popover is open at a time per monitor, and opening a second replaces the first. It is a layer-shell
window covering the whole monitor at `Layer::Top`, exclusive zone 0, with an arrow drawn by a
`DrawingArea` and a 150ms `adw::TimedAnimation` on opacity.

**`KeyboardMode::None`.** A popover receives no key events at all — no focus, no `Escape`, no
type-ahead. Do not design a surface that needs typing; it will silently not work.

**The arrow points at `anchor()`.** The default is the applet's whole `IndicatorGroup`; override
`anchor()` to return a child instead, as the pager does to point at the workspace strip. The runtime
recomputes it in `follow_anchor` after every `deliver`, so an anchor that moves as chips appear and
disappear is followed while the popover stands open.

The catcher does the rest: `placement` centres the body on the anchor, clamps it inside the output,
and keeps the arrow off the body's rounded corners. That arithmetic is tested; do not re-derive it
in an applet.

## The four steps of `popover()`

```rust
fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
    let shown = WeatherPopover::new();                     // 1. build fresh

    let caller = seat.caller();                            // 2. wire
    shown.connect_activated(move |id| caller.call::<FocusWorkspace>(/* … */));

    let opener = seat.opener();
    shown.connect_day_selected(move |_, _| opener.wake());

    if let Some((_, command)) = &self.footer {
        let command = command.clone();
        shown.connect_footer_activated(move |_| run(&command));
    }

    self.shown.set(Some(&shown));                          // 3. hold weakly
    self.refresh();                                        // 4. dress, then hand over
    Some(Box::new(shown))
}
```

**Every open builds a new widget.** Nothing is cached and nothing is reused. State that must survive
an open/close cycle belongs to the applet — `Clock::range`, `NextEvent::chosen` — not to the widget.
`Clock` resets `self.range = None` on open for exactly this reason: the fresh calendar shows today,
so the range it asked for last time no longer describes what is on screen.

`Seat` is not `Ctx`. It hands out two things and nothing else: `seat.caller()` for commands and
`seat.opener()` for wakes. Both are `Clone + 'static`, which is what lets a signal closure capture
one; `&Ctx` is neither and cannot be captured.

## `dress` and the weak reference

The applet's only handle is `shown: glib::WeakRef<XPopover>`. The strong references are the
catcher's child and the runtime's `Option<Box<dyn PopoverHandle>>`, and both go on dismissal — so
`upgrade()` returning `None` **is** the closed state. There is no `is_open` flag to keep correct.

**Never hold a strong reference.** It keeps the widget alive past dismissal, and the next open builds
a second one while the first is still being dressed.

**One method computes the chips and dresses the popover.** Every applet with a popover has it:

```rust
fn refresh(&mut self) {
    self.spec = self.indicator().into_iter().collect();

    if let Some(shown) = self.shown.upgrade() {
        self.dress(&shown);
    }
}
```

`configure`, `handle` and `popover` all end in `refresh`. Two separate paths — one for the bar, one
for the popover — is how a popover comes to disagree with the chip above it, and the disagreement
only shows while the popover happens to be open.

**`dress` writes and does not read.** It takes `&self` and `&Popover` and pushes every value in. The
one exception is state the viewer owns: `Clock::paint` reads `shown.selected()` and
`shown.shown_month()` back, because which day is showing was the viewer's choice, not the applet's.

## The wake loop

A signal handler runs with no `&mut self`, so it cannot update the applet. It calls
`opener.wake()`, which sends `HostInput::Woken` → `Input::Woken` → `handle` → `refresh`.

```rust
let opener = seat.opener();
shown.connect_month_shown(move |_, _, _| opener.wake());
```

**`Input::Woken` must be matched.** An applet whose `handle` matches only `Topic` and `Tick` and
returns on everything else swallows the wake and the popover never updates. Both spellings in the
tree are explicit: `Input::Tick | Input::Woken => {}` followed by a `refresh`, and a `matches!` on
the pair.

Commands from a popover go through `seat.caller()`, which is the same fire-and-forget
`Caller::call::<C>` as `ctx.call` — the reply is discarded and the topic reconciles. `WorkspacesPopover`
sends `FocusWorkspace` and `FocusWindow` this way, and emits **ids** rather than capturing widgets,
because a row is rebuilt whenever the list changes.

## The footer

`config.common.settings()` returns `Option<(&str, &[String])>` — a label and an argv, guaranteed by
the loader to be set together or not at all. Every applet stores it identically in `configure`:

```rust
self.footer = config
    .common
    .settings()
    .map(|(label, command)| (label.to_owned(), command.to_vec()));
```

The label goes to `shown.set_footer(...)` in `dress`; `connect_footer_activated` is connected **at
build time**, in `popover()`, and runs `popover::run(&command)`. Connecting a signal inside `dress`
stacks one handler per event, and the symptom is a footer that launches four copies of the settings
app.

`run` spawns through `gio::Subprocess` and warns if the program does not start. It is the one place
in the panel that starts a process; nothing else shells out.

`WorkspacesPopover` has no footer. Three of the four popovers do.

## Size

**Nothing in the panel scrolls.** `PopoverShell` does not, the catcher does not cap the height
against the work area, and there is no `ScrolledWindow` anywhere in the crate. A popover is bounded
because the applet bounds it:

- counts come from the config — `upcoming`, `horizon`, `days`, `hours`
- `EventList` moves the remainder behind a "N more events" row that toggles a drawer
- a drawer opens **to the side**, so the list it came from stays on screen

A surface that can grow with the data and has no cap will run off the monitor.

## What belongs to the widget

The composition is a Blueprint template in `glimpse-widgets`; the applet supplies data through
setters. See the `widget` skill and `glimpse-widgets/README.md` for the widget half. Three rules
reach back across the boundary:

- **Every user-visible string is formatted by the applet**, because what an event's time says
  depends on `now`. The widget owns structure and no content.
- **Except placeholder wordings, which live in the template.** Nothing in this tree translates a
  Rust string reaching a label — writing "Nothing scheduled" from Rust is the bug both popovers were
  fixed for.
- **Formatting lives in a `render.rs` beside the applet** — `weather/render.rs`,
  `next_event/render.rs`, `clock/popover.rs` — pure functions over `now` and the payload, tested
  headlessly with no GTK. The applet module is then wiring, and the tests do not need a display.

## Pitfalls

Symptom first. Every one of these was a real defect or a near miss.

**The popover shows stale data while the bar is current.** Dressing is on a different path from
`indicators()`. Funnel both through one `refresh`.

**A row works once and then stops, or fires twice.** The handler was connected in `dress`, which runs
on every event. Connect in `popover()`; dress only writes values.

**The applet's action fires whenever the popover opens.** The applet matched
`Pointer::Press(Button::Left)`. The runtime already owns left click as the popover toggle.

**The second open shows a blank or stale popover.** A strong reference kept the first widget alive,
so the second `dress` wrote into a widget the catcher never received. `glib::WeakRef` only.

**Picking a day or a page changes nothing.** Either the signal was wired without `opener.wake()`, or
`handle` drops `Input::Woken` through a fall-through arm.

**`Escape` does not close it and nothing can be typed into it.** `KeyboardMode::None`, deliberately.
Dismissal is a click outside.

**The popover runs off the bottom of the screen.** Nothing scrolls and nothing caps. Cap the list.

**Opening one applet's popover closed another's.** One catcher per bar. That is the design, not a
defect.
