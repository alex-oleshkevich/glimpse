# The popover

An applet's second surface.

**Verified against `applet/popover.rs`, `applet/catcher.rs`, `applet/runtime.rs`.**
When this file and the code disagree, the code is right.

## The standard layout

Every popover is a `$PopoverShell` with an addon and a body. The shell is the composition; an applet chooses
what fills the body and almost never anything else. Read this before drawing a new one — every
popover agrees on it, and one that does not is a finding, not a style.

```
┌─────────────────────────────────────────────────┐
│ icon   title                        [addon]     │  header, hidden until it has content
│        subtitle                                 │
│                                                 │
│          Gtk.Box .column                        │  body, inside scroller
│                                                 │
│            $Section   TITLE   count  [trail]    │    section__header
│              $Row   icon  title       value  ✓  │
│                           subtitle              │
│              $ExpanderRow  …                 ›  │
│                                                 │
│            $Section   TITLE                     │
│              $Row   …                           │
│              $ExpanderRow "N more"           ›  │
│                                                 │
└─────────────────────────────────────────────────┘
```

The three composed widgets, each a row of slots left to right, `hexpand` marked:

```
$Hero      [icon] [title / subtitle]──►──[slot]
$Row       [icon] [start] [title / subtitle]──►──[value] [spinner] [end] [check] [chevron]
$Section   header( [title]──►──[count] [trail] )  content  [placeholder]
```

- **A slot with nothing visible in it renders nothing.** `header` starts `visible: false`, and
  `PopoverShell` watches each body child's `visible` so the scroller follows it.
- **The width floor is `.applet-popover`'s** — `min-width: var(--applet-popover-width)`, 27rem in
  `:root`, and `.popover-shell .column` repeats it. `size: wide` on the `$PopoverShell` swaps in
  `--applet-popover-width-wide`, as `NotificationsPopover` does.
  **Never write a `width-request`** — `.claude/rules/ui.md` forbids it,
  and one smaller than the floor is silently inert.

## The skeleton

Every popover template is this, and the only lines that vary are the class name and what fills the
column:

```blueprint
using Gtk 4.0;

template $XPopover: Gtk.Widget {
  layout-manager: BinLayout {};

  styles [
    "x-popover",
  ]

  $PopoverShell shell {
    icon-name: "x-symbolic";
    title: _("X");

    [addon]
    Gtk.Switch power { valign: center; }

    Gtk.Box column {
      orientation: vertical;
      hexpand: true;

      styles [
        "column",
      ]

      $Section first {
        visible: false;
        title: _("Connected");

        Gtk.Box first_rows { orientation: vertical; }
      }
    }
  }
}
```

The outer `Gtk.Widget` + `BinLayout` is not decoration: subclassing `Gtk.Widget` keeps `append` and
`remove` out of the public API, so the only way to change the contents is the widget's own reconcile.

## The five ways content varies

Pick one per list; they all grow downwards, so they compose without moving the card sideways.

| Situation | Shape | Worked example |
| --- | --- | --- |
| A list, flat | `$Section`s in one `.column`, a `$Row` each | `KeyboardPopover`, `PlacesPopover` |
| A row has detail | an `$ExpanderRow` per item: its head, then its `details()` box | `RemovablePopover`, `PowerPopover` |
| More than fits | an `$ExpanderRow` "N more" whose drawer holds the same row widget; `EventList` carries its own "N more" row | `RemovablePopover`, `CalendarPopover` |
| Empty or partial | a `$Placeholder` or a label in `$Section`'s `[placeholder]` slot | `NetworkPopover` `looking`, `CalendarPopover` `truncated` |
| One question owns the card | a `Gtk.Stack` **around** the `.column`, `vhomogeneous: false`, the other page carrying the question | `BluetoothPopover`, `NetworkPopover` |

- **An open detail dims everything it is read against. This is not optional, and it is not the
  popover's code.** Opening an `Expandable` tells its `PopoverShell`, which closes every unrelated
  one and gives `drawer::RECEDED` to the largest subtree outside the open card — hero, other
  rows, other sections' content — derived from the tree on each change, so nothing lists what dims
  and a widget added later is covered. The open card takes `.open` and `.card` and keeps full
  weight; its head never moves, because only paint changes. **Section headers and separators stay
  lit**: they are the frame, not content.
- **A press on anything dimmed closes the detail and does nothing else.** The shell's capture-phase
  click claims it before any row sees it, so reaching past an open card never fires the row under
  the pointer. Inside the card, and on its own head, presses go through — and so does a press on
  another card's opener (an `$ExpanderRow` head, or anything carrying `expandable__opener`), which
  switches cards in one click. Hover is suppressed on dimmed rows except those openers, so what
  still works looks it. Scrollbars are neither dimmed nor outside.
- **The overflow row sits inside the section it belongs to.** A list that stops overflowing hides
  it rather than leave it open on nothing.
- **Detail and overflow unfold downwards.** Height is the free axis; width is not.
- **Build a detail on first open, not at construction.** Fourteen devices would otherwise carry
  fourteen detail boxes nobody asked to see.

## Typical widget composition

Nothing below is assembled in Rust when it can be declared. The frame is fixed; only the middle
column is a choice, and the choice is almost always "which list already exists".

```
$PopoverShell
├── [addon]   a Gtk.Switch or a $Readout, in the header beside icon · title/subtitle
├──           Gtk.Box .column
│              ├── $Section    title · count · [trail] · content · [placeholder]
│              │    ├── $Row           the unit of every list, a readout
│              │    ├── $ActionRow     a $Row whose body is a command
│              │    ├── $ToggleRow     a $Row carrying a switch
│              │    ├── $SliderRow     a value set by dragging
│              │    ├── $MeterRow      a value read as a bar
│              │    ├── $ExpanderRow   an $ActionRow head over a details box
│              │    ├── $Notice        an inline banner carrying a severity
│              │    └── $Placeholder   the empty state, wording in the .blp
│              └── $Section …
```

The domain lists drop into a `$Section` in place of loose rows, and each owns its own row type:

| List | Rows it builds | Declared in |
| --- | --- | --- |
| `$EventList` | its own event rows plus "N more" | `calendar_popover`, `next_event_popover` |
| `$WorldClock` | its own zone rows | `calendar_popover` |
| `$ForecastStrip` | `$ForecastHour` | `weather_popover` |
| `$ForecastList` | `$ForecastDay` → `$RangeBar` | `weather_popover` |
| `$WorkspaceList` | `$WorkspaceSection` → `$Section` | `workspaces_popover` |
| `$PlayerList` | `$PlayerRow` | `mpris_popover` |
| `$NowPlaying` | `$Scrubber` + `$Transport` | `mpris_popover` |
| `$Calendar` | its own month grid | `calendar_popover` |
| `$NotificationStack` | `$NotificationCard` → header + one body | built from Rust |
| `$TrayStrip` | `TrayChip` | built from Rust, by the tray applet |
| `$Pager` | `$PagerItem` | built from Rust, by the pager applet |

- **Declare what is fixed, build what is computed.** A list whose children come from the data is a
  named empty `Gtk.Box` in the `.blp` that Rust reconciles into; `$Section layouts` holding
  `Gtk.Box rows` in `KeyboardPopover` is the shape. Do not build the section from Rust to save the
  box.
- **Reach for `$Row` before anything else.** `$ActionRow` when clicking the row does something;
  `$ToggleRow` when it carries a switch; `$Placeholder` only for an empty list, never as a "no value
  yet" filler.
- **`$Notice` never reports a failed command.** That is a notification's job, and `spawn_reported`
  already posts one — a banner beside the row repeats it and is still there on the next open. Use
  `$Notice` only for standing content the viewer must act on, such as a weather advisory, or for the
  one condition a notification cannot carry: the notification provider itself being down.
- **A row highlights only if clicking its body does something.** A readout is a `$Row`; a command
  is an `$ActionRow`, which carries `row--interactive`; a switch is a `$ToggleRow`, whose body and
  knob share one `toggled` emitter, so neither can double the other. A control never sits inside a
  row that claims the click — `SourceList` parents each `SliderRow` beside its row, not inside it. A
  shared rule about a *class* of rows belongs in `glimpse-widgets`, never re-implemented per applet.

## Animation

Two tokens are the entire vocabulary, in `:root`:

```css
--gl-duration: 150ms;
--gl-ease: cubic-bezier(0.2, 0, 0, 1);
```

**Every CSS transition reads both.** A rule that writes its own duration or easing is a finding —
there is no third timing in the shell, and inventing one makes two surfaces that move together look
like they do not.

**Reduced motion is already handled, for rules that do not yet exist.** `glimpse.css` carries
`@media (prefers-reduced-motion: reduce) { * { transition: none; animation: none; } }`, and the `*`
means a new transition is covered the moment it is written. Do not add a second guard beside it, and
do not reach for `glib::timeout_add` to hand-roll motion that escapes it — `.claude/rules/ui.md`
reserves that API for animation precisely so this one rule can switch all of it off.

What actually moves:

| Motion | How | Where |
| --- | --- | --- |
| The popover fading in | `adw::TimedAnimation` driving `shade` on the body and arrow, `glimpse_widgets::duration_ms()`, `Easing::EaseOutCubic` | `applet/catcher.rs` |
| A detail unfolding under its row | `$Expandable`'s own `Gtk.Revealer`, `SlideDown`; the row and details become one `.card` | every `$ExpanderRow` |
| A page changing | `Gtk.Stack`, `transition-type: crossfade` **plus `interpolate-size: true`** | `$Calendar` |
| A chevron turning | `transform: rotate(…)` on the image, transitioned | `.row__chevron` 90°, `.tray-strip__chevron` 180° |
| Everything but the open row dimming | `opacity` on `.receded`, set by `PopoverShell`; the fade rides the permanent `.recedes` marker | every `PopoverShell` |
| A notification popup arriving | `opacity` + `transform: translate` by `--gl-popup-motion` | `glimpse-notifications` |

- **The fade is driven from the tick callback, not from `open`.** It starts once the body has been
  measured and placed, so the first frame the viewer sees is already in the right position; a fade
  begun at open would play against a body that is still being placed.
- **Closing animates from the *current* opacity**, not from 1. `Catcher::close` reads
  `body.opacity()` into `set_value_from`, so dismissing a popover mid-open reverses from where it
  got to rather than snapping to full and falling.
- **`interpolate-size: true` is not optional on a stack that animates.** Without it the stack
  requests the height of its tallest page and the popover jumps to that size before the crossfade
  starts; with it the surface grows into the page.
- **A chevron rotates; it is never swapped for another icon.** The glyph therefore has to be
  directional — a rotated symmetrical one reads as nothing happening.
- **A notification's direction comes from its edge.** `animation_class(edge)` picks
  `notification-popup__entry--top` or `--bottom`, so a card enters from the screen edge it lives on
  rather than always from above.
- **An animation driven from Rust reads `glimpse_widgets::duration_ms()`**, the Rust side of
  `--gl-duration` scaled by the animation speed, and never writes a number of its own.

## What opens it, and what closes it

The runtime does. `HostInput::Pressed` delivers the press to `handle` **and then**, for
`Button::Left`, calls `toggle_popover` — which calls `catcher.close()` if this applet's popover is
up, and otherwise `show_popover`, which calls `applet.popover(&seat)`. Left click is therefore a
toggle the applet does not implement and must not duplicate: an applet that also acts on
`Input::Pointer(Pointer::Press(Button::Left))` fires its action every time the popover opens.

`Opener` is how an applet drives that itself: the pager calls `toggle_popover` from a click it wires
inside `view()`, and bluetooth and network call `open_popover` for a question that arrived while
nothing was open.

Dismissal is a click anywhere outside the popover body, caught by a `GestureClick` on the catcher
window. It reaches the runtime as `HostInput::PopoverDismissed`, which drops the runtime's handle.

## Where it opens

**One `Catcher` per bar**, built in `Panel::init` and shared by every applet in every zone. So one
popover is open at a time per monitor, and opening a second replaces the first. A bare layer-shell
window covering the whole monitor at `Layer::Top` catches outside clicks; the body sits above it on
a `Layer::Top` surface sized to itself, with a `PopoverArrow` and an `adw::TimedAnimation` fade.

**`KeyboardMode::None` until the applet asks.** A popover receives no key events — no focus, no
`Escape`, no type-ahead — until its applet calls `opener.typing(true)`, which switches the surface
to `KeyboardMode::Exclusive`; `Catcher::open` and `Catcher::close` reset it. The clipboard and
workspace-name applets type this way, and network does while its password page is up.

**The arrow points at `anchor()`.** The default is the applet's whole `IndicatorGroup`; override
`anchor()` to return a child instead, as the pager does to point at the workspace strip. The runtime
recomputes it in `follow_anchor` after every `deliver`, so an anchor that moves as chips appear and
disappear is followed while the popover stands open.

The catcher does the rest: `placement` centres the body on the anchor, clamps it inside the output,
and keeps the arrow off the body's rounded corners. That arithmetic is tested; do not re-derive it
in an applet.

**It re-places whenever the body's measurement changes**, not only at open — a tick callback lives
as long as the popover does. That is what stops content revealed after the first frame from growing
against a margin computed for the narrow body and walking off the output. It is a safety net, not a
licence: re-placement is motion the user sees, so keeping the size stable is still the applet's job.

**The gutter yields to the arrow.** A chip within roughly `arrow + arrow / 2` of the output edge
cannot both keep a gutter and point at its item, and the arrow wins — so a popover opened from the
last chip on the bar sits flush against the edge. That is the design, not a clipping bug.

## The four steps of `popover()`

```rust
fn popover(&mut self, seat: &Seat) -> Option<gtk4::Widget> {
    let shown = WorkspacesPopover::new();                  // 1. build fresh

    let compositor = self.compositor.clone();              // 2. wire
    shown.connect_activated(move |id| {
        let compositor = compositor.clone();
        spawn_command("compositor.focus_workspace", async move {
            compositor.focus_workspace(WorkspaceRef::Id { id }).await
        });
    });

    self.shown.set(Some(&shown));                          // 3. hold weakly
    self.refresh();                                        // 4. dress, then hand over
    Some(shown.upcast())
}
```

Step 2 is `pager::Pager`'s; a signal that changes what the applet renders captures `seat.opener()`
instead and wakes it — see the wake loop below.

**Every open builds a new widget.** Nothing is cached and nothing is reused. State that must survive
an open/close cycle belongs to the applet — `Clock::range`, `NextEvent::chosen` — not to the widget.

**A fresh widget is not a fresh popover, and the framework cannot make it one.** The widget is new;
the applet's fields are not, and `popover()` re-dresses the new widget from them, so anything that
*selects or expands* something comes back. Reset it explicitly in `popover()`: `Clock` sets
`self.range = None` and calls `open_on(today)`, `Bluetooth` resets `expanded` and `nearby_toggled`.
The symptom when you forget is a popover that opens with a section already unfolded, under a row the
user did not press this time.

**An open detail survives its own action and closes only when that action succeeds.** Pressing a row
inside a detail leaves the detail standing: it is the only place progress can be shown, and a
failure needs its context. **The command's own reply is the answer** — `relm4::spawn_local` awaits
it on the main context, so the `Ok` arm can clear the applet's `selected` directly and the `Err` arm
reports through the same wording a notification would. Do not infer completion from a busy edge
instead: a command refused before it starts never sets `busy` at all, and a `watch` coalesces, so the
busy window can pass between two wakes and be missed entirely. Finish the job too: pairing ends
**connected**, not merely bonded, which the service does by trusting and connecting once the bond
appears.

**Work in flight is a spinner and never a word.** `Row` owns a `busy` property; the device row spins
where its trailing value would have been, and the action row that started the work spins too. A
label saying "Connecting…" beside a spinner that already says it is read twice, and once the detail
stopped collapsing on click the two sat directly above each other. The render types carry `busy` so
the decision stays in `render.rs` and the widget only draws it.

`Seat` is not `Ctx`. It hands out one thing: `seat.opener()`, for wakes and for opening and closing.
`Opener` is `Clone + 'static`, which is what lets a signal closure capture it; `&Ctx` is neither and
cannot be captured. Commands need no seat: the closure captures a cloned service handle.

## `dress` and the weak reference

The applet's only handle is `shown: glib::WeakRef<XPopover>`. The strong references are the
catcher's child and the runtime's `Option<gtk4::Widget>`, and both go on dismissal — so
`upgrade()` returning `None` **is** the closed state. There is no `is_open` flag to keep correct.

**Never hold a strong reference.** It keeps the widget alive past dismissal, and the next open builds
a second one while the first is still being dressed.

**One method computes the chips and dresses the popover.** `idle::Idle`'s is the shape:

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

**`Input::Woken` must be matched.** An applet whose `handle` matches only `Tick` and returns on
everything else swallows the wake and the popover never updates. Both spellings in the tree are
explicit: `idle::Idle` matches `Input::Woken` and `Input::Tick` before its `refresh`, and
`clock::Clock` uses a `matches!` on the pair.

Commands from a popover are spawned on a cloned service handle, exactly as from `handle`; the
snapshot carries the result back as a wake. `WorkspacesPopover` emits workspace and window **ids**
rather than capturing widgets, because a row is rebuilt whenever the list changes, and `pager::Pager`
turns them into `focus_workspace` and `focus_window`.

## Dialogs and prompts

**A pairing prompt is a page in the popover, not a sheet over it.** `BluetoothPopover` holds a
`Gtk.Stack pages` whose two pages are the device column and the prompt. A prompt swaps the column
out and leaves the hero insensitive, so the surface carries one question and nothing
else to press. A view over a popover is not a shape this shell has; a stack also needs no dismissal
guard on `Catcher`.

**The two prompts that need typing keep the dialog.** `RequestPin` and `RequestPasskey` are answered
in `App`'s `PairingDialog`, not on the popover's page. `render::typed` makes the
split inside the watch and **narrows the type while it is there**, handing `App` a `PairingEntry`
rather than a `Prompt` — so the dialog has no case to refuse and a prompt the popover renders never
reaches `App`, and therefore never trips `close_popovers` on its way past, which would close the
surface it is about to be drawn on.

**A prompt arriving with no popover open opens one.** That is `Opener::open_popover`'s caller,
guarded by the **device id**: BlueZ escalates a pairing mid-flow, confirm then passkey, and a guard
on "a prompt exists" would re-open a popover the viewer had just dismissed. Dismissing does not
answer the prompt — the chip stays in its acquiring state and pressing it re-opens onto the page.
BlueZ's own agent timeout is the backstop.

**A confirmation is the same page.** `Confirmation::Forget` renders through the same `Ask` with a
destructive accept, so `App` owns no confirmation dialog at all — one page serves both
questions and the applet routes the single `answered` signal by what `render::asked` says is on it.
Two consequences follow. **A dismissed popover must answer a confirmation**, because that state is
glimpse's own and nothing outside will ever retire it — unlike a pairing prompt, which BlueZ cancels.
And because one card serves two questions, `Ask` carries a **key**; when it changes the buttons are
bounced insensitive for an instant, which cancels any press already in flight so a click meant for
one question cannot answer the other.

**`App` owns the dialogs that are left** — `PairingDialog` and `SecretDialog` — pairing each with
**its `SignalHandlerId`**.

**The host is the component root, and it is invisible.** `App`'s `view!` is an
`adw::ApplicationWindow` declared `set_visible: false`. Panels are not its children; it exists only
to give `adw::Dialog` something to present on.

**Make the host visible before `present`, and hide it when the last dialog goes.** `present()` on a
never-mapped window **queues the dialog and shows nothing** — no error, no warning, no drawn pixel.
Raise the host immediately before presenting, and lower it once both slots are empty.

**Close every popover first.** The catcher is a layer surface above every toplevel *and* the dialog
is modal within the application, so leaving a popover up makes the whole screen unclickable — see the
pitfall below. `App` broadcasts `panel::Input::ClosePopover` to each panel before either show.

**One dialog serves every variant it is given.** The existing dialog is *re-dressed* rather than
rebuilt, and its `answered` signal is connected exactly once at build. That is the same
connect-at-build, dress-on-every-event rule a popover follows, for the same reason.

**`force_close` fires the close response.** Tearing a dialog down would therefore answer the prompt —
cancelling a pairing the user never saw. Block the stored handler around the close; that is what the
handler id is kept for.

**A failed command is reported, not swallowed.** A command's reply carries a typed failure the applet
turns into wording (`spawn_reported`), so the viewer gets a notification rather than a `warn!` line
they will never read. Only failures answering something the viewer asked for: a device dropping on
its own belongs on its row, not in a notification.

## Size

**The height cap is a backstop.** `PopoverShell`'s `scroller` propagates its natural height until
the body reaches 60% of the monitor's height (`body_cap`), and only then shows a scrollbar. Do not
read it as the escape hatch: a card that scrolls has lost its shape. A popover is bounded because
the applet bounds it:

- counts come from the config — `upcoming`, `horizon`, `days`, `hours`
- `EventList` moves the remainder behind a "N more" row that unfolds in place

**Growth has a direction, and only one of them is free.** Height already varies with the data — a
device list, a scan's results — and has never caused a placement bug, because a popover grows away
from the bar. Width is the constrained axis: a card that grows sideways near an output edge leaves
the screen, and re-placing it slides the row out from under the pointer that just opened it. **A
detail therefore unfolds under its own row, not beside the list.** Each item is an `$ExpanderRow`,
reconciled by key like any row (`by_key`).

- **The head toggles its own `Expandable`.** The applet keeps no open flag; it fills the details on
  first open and refreshes the one that is `expanded()` when new state arrives.
- **A forced close is `set_expanded(false)`** — a detail that closes on success, a device that
  vanished. Removing an open `Expandable` from the tree releases the shell's dimming by itself.
- **Tests assert on the `Expandable`**, not the head: `.open`, `.card` and `.receded` sit on it,
  and `PopoverShell::dismiss_from(&widget)` is the headless stand-in for a press.

## A popover's width is a promise

The card is as wide as the widest label's **natural** request. `ellipsize: end` does not bound that
— it only lets a label shrink when it is squeezed — and capping the *text* does not either, because
`clean(name, 24)` still asks for twenty-four characters. `max-width-chars` is the only lever that
caps the request while still letting the label fill a wider allocation.

Every content scenario of a popover must land on one width — bare, long name, opened, long hero,
long value, scanning, and each prompt page. An uncapped hero subtitle, a long value or a
peer-supplied code each widens the card; `Hero`'s title and subtitle and `Row`'s title and subtitle
cap at 24, `Row`'s value at 16, and `BluetoothPopover`'s `prompt_code` at 16, which is what holds
it. Assert it: `measure(gtk4::Orientation::Horizontal, -1).1` against a floor taken from the same
probe is one line, as `bluetooth_popover_checks.rs` does, and it is the only thing standing between
a stable card and one that resizes while it is being read. A `Gtk.Stack` swapping a page answers the same question the same
way: `hhomogeneous` is left at its default, so the card takes the wider of the two pages once rather
than resizing when one replaces the other.

## Deciding a popover's shape

A layout question — drawer, navigation, inline expansion, one shade — is answered on screen, not in
a discussion, and `just preview` is faster than the panel for it.

- **Build each candidate as a board in `var/widget_examples/`**, inside a mock output: a box with
  `overflow: hidden`, a fixed `width-request`, a bar across the top and a chip at its right edge.
  A card that runs past that frame is a card that would run off a real screen, which is the whole
  point — a candidate that looks fine floating on a checkerboard can still be broken.
- **Drive it with the fixtures that already run for every example**, listed in `docs/preview.md`.
  An `$Expandable` needs none to toggle — it toggles itself, and its `PopoverShell` dims and
  dismisses as it does on the bar; the `expanded` fixture opens one carrying `state__open`, and
  `actions` logs a row carrying `action__<name>`.
- **Iterate in `var/widget_examples/_shared.css`**, which hot-reloads, then port the settled values
  into `glimpse.css`. Keep the board on the same numbers afterwards, or the example stops being
  evidence of what ships.
- **Synthetic clicks cannot reach a preview**, but the keyboard can — only inside a nested niri,
  never on the user's session (`docs/live-testing.md`). `Tab` walks and `space` activates;
  `Return` does not activate an `$ActionRow`.

## What belongs to the widget

The composition is a Blueprint template in `glimpse-widgets`; the applet supplies data through
setters. See the `widget` skill and `glimpse-widgets/README.md` for the widget half. Three rules
reach back across the boundary:

- **Every user-visible string is formatted by the applet**, because what an event's time says
  depends on `now`. The widget owns structure and no content.
- **Except fixed wordings, which live in the template.** A placeholder such as "Nothing scheduled"
  is `_("…")` in the `.blp`; a string the applet formats is marked with `gettext` in Rust.
- **Formatting lives in a `render.rs` beside the applet** — `weather/render.rs`,
  `next_event/render.rs`, `clock/popover.rs` — pure functions over `now` and the snapshot, tested
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

**`Escape` does not close it and nothing can be typed into it.** `KeyboardMode::None` until the applet
calls `opener.typing(true)`. Dismissal is a click outside.

**The popover scrolls, or grows towards the far edge of the screen.** The applet handed over an
unbounded list and hit `PopoverShell`'s height cap. Cap the list.

**Opening one applet's popover closed another's.** One catcher per bar. That is the design, not a
defect.

**A dialog is up and nothing anywhere is clickable — not the dialog's buttons, not the popover's
rows, though both still highlight under the pointer.** The catcher is a layer surface at
`Layer::Top` anchored to all four edges, so it sits *above* every ordinary toplevel — including the
dialog presented on the panel's host window, which therefore never sees the press. The
other half is GTK's: that dialog is modal within the application, so its grab swallows the presses
the catcher does get. **Close every popover before presenting a dialog** — `App` sends
`panel::Input::ClosePopover` to each panel, and `Catcher::close` tears down properly on its own,
because `finish_close` unmaps the window and `release` runs the dismissal callback the runtime is
waiting on.

**The detail page is missing, or half of it is off the screen.** Placement was computed once, at
open, against a body that has since grown. The catcher re-places on measurement changes; a surface
that resizes outside that path has the same bug.

**The row you clicked slides away as it opens.** The card grew on the constrained axis and was
re-placed under the pointer. Grow down.

**The card is a different width every time it opens.** A label's natural width, not its text length.
`max-width-chars`.

**A row's title collapses to an ellipsis while its value is fully readable.** `Row` gives the value
its natural width first. Bound what the applet hands over — three profile names and an ellipsis
rather than six — and cap the label.
