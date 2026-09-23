# The popover

An applet's second surface.

**Verified against `applet/popover.rs`, `applet/catcher.rs`, `applet/runtime.rs`.**
When this file and the code disagree, the code is right.

## The standard layout

Every popover is a `$PopoverShell` with three slots. The shell is the composition; an applet chooses
what fills the middle and almost never anything else. Read this before drawing a new one — eight
popovers already agree on it and a ninth that does not is a finding, not a style.

```
┌─────────────────────────────────────────────────┐
│ [hero]   $Hero                                  │  hero_box, hidden until a child is visible
│            icon   title             [slot]      │
│                   subtitle                      │
├─────────────────────────────────────────────────┤  hero_rule, follows hero_box
│          Gtk.Box .column                        │  content_box, vexpand
│                                                 │
│            $Section   TITLE   count  [trail]    │    section__header
│              $Row   check icon lead  title   ›  │
│                                      subtitle   │
│              $Row   …                           │
│                                                 │
│            $Section   TITLE                     │
│              $Row   …                           │
│              $Row   view-more-symbolic "N more" │
│                                                 │
├─────────────────────────────────────────────────┤  footer_rule, follows footer_box
│ [footer] $Row   settings-symbolic  "Settings"   │  footer_box, horizontal, 0..N children
└─────────────────────────────────────────────────┘
```

The three composed widgets, each a row of slots left to right, `hexpand` marked:

```
$Hero      [icon] [title / subtitle]──►──[slot]
$Row       [check] [icon] [lead] [title / subtitle]──►──[value] [trail]
$Section   header( [title]──►──[count] [trail] )  content  [placeholder]
```

- **A slot with nothing visible in it renders nothing, including its rule.** `hero_box` and
  `footer_box` start `visible: false` and `PopoverShell` watches `notify::visible` on what is
  appended, so a hero whose title is cleared takes its separator with it.
- **`[footer]` takes more than one child.** `NotificationsPopover` appends `clear` and then `footer`
  into the same horizontal box.
- **`.column` is what carries the width floor** — `min-width: var(--gl-popover-min-width)`, 27rem in
  `:root`. Put it on the content box, or on the single `$Section` when there is no box
  (`KeyboardPopover`). Overriding the token per popover is the supported way to be narrower;
  `KeyboardPopover` sets 8rem. **Never write a `width-request`** — `.claude/rules/ui.md` forbids it,
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
    [hero]
    $Hero hero {
      icon-name: "x-symbolic";

      [slot]
      Gtk.Switch power { valign: center; }     // optional
    }

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

    [footer]
    $Row footer {
      visible: false;
    }
  }
}
```

The outer `Gtk.Widget` + `BinLayout` is not decoration: subclassing `Gtk.Widget` keeps `append` and
`remove` out of the public API, so the only way to change the contents is the widget's own reconcile.

## The five ways content varies

Pick one. They compose badly — a drawer beside an inline expansion moves on both axes at once.

| Situation | Shape | Worked example |
| --- | --- | --- |
| A list, flat | `$Section`s in one `.column`, a `$Row` each | `BluetoothPopover`, `KeyboardPopover` |
| A row has detail | an `$Expandable` per item: the `$Row` or `$SplitRow` head, then `[details]` | `WorkspacesPopover` windows |
| More than fits | a `Gtk.Revealer drawer` **beside** the column, `transition-type: slide_right`, holding a vertical `Gtk.Separator` and a `.drawer-page` | `CalendarPopover` |
| Empty has variants | a `Gtk.Stack` in `$Section`'s `[placeholder]` slot, one `$Placeholder` per page | `CalendarPopover` `nothing` / `truncated` |
| One question owns the card | a `Gtk.Stack` **around** the `.column`, `vhomogeneous: false`, the other page carrying the question | `BluetoothPopover` pairing prompts |

- **An open detail dims everything it is read against. This is not optional, and it is not the
  popover's code.** Opening an `Expandable` tells its `PopoverShell`, which closes every unrelated
  one and gives `drawer::RECEDED` to the largest subtree outside the open card — hero, footer, other
  rows, other sections' content — derived from the tree on each change, so nothing lists what dims
  and a widget added later is covered. The open card takes `.open` and `.card` and keeps full
  weight; its head never moves, because only paint changes. **Section headers and separators stay
  lit**: they are the frame, not content. A `SplitRow` is one leaf, so its own divider is not frame.
- **A press on anything dimmed closes the detail and does nothing else.** The shell's capture-phase
  click claims it before any row sees it, so reaching past an open card never fires the row under
  the pointer. Inside the card, and on its own head, presses go through — and so does a press on
  another card's opener (a `SplitRow`'s chevron, a plain `Row` head), which switches cards in one
  click. Hover is suppressed on dimmed rows except those openers, so what still works looks it.
  Scrollbars are neither dimmed nor outside.
- **Not yet moved:** a popover still on `crate::drawer::holder` writes its own recede loop and has
  none of the above. The trap there is a list with nothing open in it — `ForecastList::reveal(None)`
  *clears* receding, so a popover whose open item lives elsewhere must dim that list explicitly.
- **The overflow row is a `$Row` with `view-more-symbolic`**, inside the section it belongs to, and
  it toggles — `crate::drawer::toggle`, never `set_reveal_child` at a call site. A list that stops
  overflowing must close its drawer rather than leave it open on nothing.
- **Detail unfolds downwards, overflow slides sideways.** Height is the free axis; width is not. A
  drawer is only safe because its width is known before it opens, which is why neither stack in the
  table is a second revealer.
- **Build the panel on first open, not at construction.** Fourteen devices would otherwise carry
  fourteen row boxes nobody asked to see.

## Typical widget composition

Nothing below is assembled in Rust when it can be declared. The frame is fixed; only the middle
column is a choice, and the choice is almost always "which list already exists".

```
$PopoverShell
├── [hero]    $Hero            icon · title/subtitle · [slot] ← a Gtk.Switch or a $Readout
├──           Gtk.Box .column
│              ├── $Section    title · count · [trail] · content · [placeholder]
│              │    ├── $Row           the unit of every list
│              │    ├── $SplitRow      a $Row plus a trailing button, divided by a hairline
│              │    ├── $Notice        an inline banner carrying a severity
│              │    └── $Placeholder   the empty state, wording in the .blp
│              └── $Section …
└── [footer]  $Row             the settings row, 0..N of them
```

The domain lists drop into a `$Section` in place of loose rows, and each owns its own row type:

| List | Rows it builds | Declared in |
| --- | --- | --- |
| `$EventList` | `$EventRow` | `calendar_popover`, `next_event_popover` |
| `$WorldClock` | `$ClockRow` | `calendar_popover` |
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
  named empty `Gtk.Box` in the `.blp` that Rust reconciles into; `$Section connected` holding
  `Gtk.Box connected_rows` is the shape. Do not build the section from Rust to save the box.
- **Reach for `$Row` before anything else.** `$SwitchRow` when the row carries a switch; `$SplitRow`
  only when a row needs a second, separately activatable target; `$Placeholder` only for an empty
  list, never as a "no value yet" filler.
- **`$Notice` never reports a failed command.** That is a notification's job, and `spawn_reported`
  already posts one — a banner beside the row repeats it and is still there on the next open. Use
  `$Notice` only for standing content the viewer must act on, such as a weather advisory, or for the
  one condition a notification cannot carry: the notification provider itself being down.
- **A row highlights only if clicking its body does something**, and the two ways of honouring that
  are not interchangeable. A pure readout takes `set_activatable(false)`, which drops `can-target`
  and so makes the row unreachable by pointer, focus and tooltip alike. A row carrying a **control**
  must never use it — `can-target` is inherited by the whole subtree, so the control would stop
  responding too. Put the behaviour in a widget instead: `$SwitchRow`'s body flips its own knob, and
  the knob's notify is the single emitter, so neither can double the other. A shared rule about a
  *class* of rows belongs in `glimpse-widgets`, never re-implemented per applet.
- **`$ChoiceList` has no caller.** It is exported and tested and nothing builds one; do not take its
  existence as a pattern to follow.

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
| The popover fading in | `adw::TimedAnimation` on the catcher slot's `opacity`, 150ms, `Easing::EaseOutCubic` | `applet/catcher.rs` |
| A drawer opening sideways | `Gtk.Revealer`, `transition-type: slide_right` | calendar, workspaces |
| A detail unfolding under its row | `$Expandable`'s own `Gtk.Revealer`, `SlideDown`; the row and details become one `.card` | `WorkspacesPopover` |
| A drawer page changing | `Gtk.Stack`, `transition-type: crossfade` **plus `interpolate-size: true`** | calendar |
| A chevron turning | `transform: rotate(…)` on the image, transitioned | `.split-row__detail` 90°, `.tray-strip__chevron` 180° |
| Everything but the open row dimming | `opacity` on `.receded`, set by `PopoverShell`; the fade rides the permanent `.recedes` marker | `WorkspacesPopover` |
| A notification popup arriving | `opacity` + `transform: translate` by `--gl-popup-motion` | `glimpse-notifications` |

- **The fade is driven from the tick callback, not from `open`.** It starts once the body has been
  measured and placed, so the first frame the viewer sees is already in the right position; a fade
  begun at open would play against a body that is still being placed.
- **Closing animates from the *current* opacity**, not from 1. `Catcher::close` reads
  `slot.opacity()` into `set_value_from`, so dismissing a popover mid-open reverses from where it
  got to rather than snapping to full and falling.
- **`interpolate-size: true` is not optional on a stack that animates.** Without it the stack
  requests the height of its tallest page and the popover jumps to that size before the crossfade
  starts; with it the surface grows into the page.
- **A chevron rotates; it is never swapped for another icon.** The glyph therefore has to be
  directional — a rotated symmetrical one reads as nothing happening.
- **A notification's direction comes from its edge.** `animation_class(edge)` picks
  `notification-popup__entry--top` or `--bottom`, so a card enters from the screen edge it lives on
  rather than always from above.
- **`ANIMATION_MILLIS` in `glimpse-notifications` is 150 because `--gl-duration` is.** They are two
  independent numbers that have to be changed together; nothing checks it.

## What opens it, and what closes it

The runtime does. `HostInput::Pressed` delivers the press to `handle` **and then**, for
`Button::Left`, calls `show_popover` — which calls `applet.popover(&seat)` if nothing of this
applet's is open, and `catcher.close()` if something is. Left click is therefore a toggle the applet
does not implement and must not duplicate: an applet that also acts on
`Input::Pointer(Pointer::Press(Button::Left))` fires its action every time the popover opens.

`Opener::open_popover` is how an applet drives that itself: the pager wires its click inside
`view()`, and bluetooth raises a pairing question that arrived while nothing was open.

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

**It re-places whenever the body's measurement changes**, not only at open — a tick callback lives
as long as the popover does. That is what stops content revealed after the first frame from growing
against a margin computed for the narrow body and walking off the output. It is a safety net, not a
licence: re-placement is motion the user sees, so keeping the size stable is still the applet's job.

**The gutter yields to the arrow.** A chip within roughly `arrow + arrow / 2` of the output edge
cannot both keep a gutter and point at its item, and the arrow wins — so a popover opened from the
last chip on the bar sits flush against the edge. That is the design, not a clipping bug.

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

**A fresh widget is not a fresh popover, and the framework cannot make it one.** The widget is new;
the applet's fields are not, and `popover()` re-dresses the new widget from them, so anything that
*selects or expands* something comes back. Reset it explicitly at the top of `popover()`: `Clock`
sets `self.range = None` and `open_on(today)`, `Bluetooth` clears `selected` and `expanded`. The
symptom when you forget is a popover that opens with a device already unfolded, under a row the user
did not press this time.

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

## Dialogs and prompts

**A pairing prompt is a page in the popover, not a sheet over it.** `BluetoothPopover` holds a
`Gtk.Stack pages` whose two pages are the device column and the prompt. A prompt swaps the column
out and leaves the hero and the footer insensitive, so the surface carries one question and nothing
else to press. A view over a popover is not a shape this shell has; a stack also needs no dismissal
guard on `Catcher`.

**The two prompts that need typing keep the dialog.** The catcher is `KeyboardMode::None`, so
`RequestPin` and `RequestPasskey` cannot be answered in a popover at all. `render::typed` makes the
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
destructive accept, so `App` no longer owns a confirmation dialog at all — one page serves both
questions and the applet routes the single `answered` signal by what `render::asked` says is on it.
Two consequences follow. **A dismissed popover must answer a confirmation**, because that state is
glimpse's own and nothing outside will ever retire it — unlike a pairing prompt, which BlueZ cancels.
And because one card serves two questions, `Ask` carries a **key**; when it changes the buttons are
bounced insensitive for an instant, which cancels any press already in flight so a click meant for
one question cannot answer the other.

**`App` owns the one dialog that is left**, pairing it with **its `SignalHandlerId`**.

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

`WorkspacesPopover` is the only one of the eight popovers with no footer.

## Size

**Nothing in the panel scrolls.** `PopoverShell` does not, and the catcher does not cap the height
against the work area. The crate's one `Gtk.ScrolledWindow` is `NotificationsPopover`'s `scroller`,
and it does not scroll either: `propagate-natural-height: true` with no `max-content-height` asks
for the full height of its child. Do not read it as the escape hatch. A popover is bounded because
the applet bounds it:

- counts come from the config — `upcoming`, `horizon`, `days`, `hours`
- `EventList` moves the remainder behind a "N more events" row that toggles a drawer

A surface that can grow with the data and has no cap will run off the monitor.

**Growth has a direction, and only one of them is free.** Height already varies with the data — a
device list, a scan's results — and has never caused a placement bug, because a popover grows away
from the bar. Width is the constrained axis: a card that grows sideways near an output edge leaves
the screen, and re-placing it slides the row out from under the pointer that just opened it. **A
detail therefore unfolds under its own row, not beside the list.** Each item is an `Expandable`,
reconciled by key like any row; the popovers still on `crate::drawer::holder` are being moved one at
a time, by the checklist below.

**A drawer is what is left when the content is not a row's own detail.** `CalendarPopover`'s month
overflow still slides one out, because what it reveals belongs to the whole popover rather than to a
line in it. Weather had one for a day's facts and those facts belong to the day, which is why it lost
the drawer rather than kept it — the same reason `WorkspacesPopover`'s window list moved off the side
drawer it was first built with: a workspace's windows are that row's own detail, not overflow.

## Moving a popover onto `Expandable`

One popover per change, verified live before the next. `WorkspacesPopover` is the finished example.

1. **Holders become `Expandable::new(&head)`.** `drawer::head::<T>(&holder)` becomes
   `holder.head::<T>()`, `drawer::panel(&holder)` becomes `holder.details::<T>()` /
   `set_details`, and `by_key`'s widget type becomes `Expandable`.
2. **Delete the open state.** The `opened`/`open` cell, `toggle_detail`, `reveal`, `close_drawer`
   and the recede loop all go, and so does the `split.connect_details(... toggle ...)` wiring — the
   head toggles its own `Expandable`. What stays is data: fill the details on the head's signal
   (lazily, the first time) and refresh the one that is `expanded()` when new state arrives.
3. **Delete the card styling.** No `.detail-card` on the details box and no `OPEN` class on the
   head — `expandable.open` and `expandable.card` carry both, and the chevron rotation is shared.
4. **Keep what is not a row's detail.** An overflow drawer, a sideways page, an audio slider — none
   of them are `Expandable`s.
5. **A forced close is `set_expanded(false)`** — a detail that closes on success, a device that
   vanished. Removing an open `Expandable` from the tree releases the shell's dimming by itself.
6. **Tests assert on the `Expandable`**, not the head: `.open`, `.card` and `.receded` sit on it,
   and `PopoverShell::dismiss_from(&widget)` is the headless stand-in for a press.
7. **Convert the popover's board** in `var/widget_examples/` to `$Expandable { … [details] … }` in
   the same change, and drop any `expander` / `open` / `receded` classes it baked in.

## A popover's width is a promise

The card is as wide as the widest label's **natural** request. `ellipsize: end` does not bound that
— it only lets a label shrink when it is squeezed — and capping the *text* does not either, because
`clean(name, 24)` still asks for twenty-four characters. `max-width-chars` is the only lever that
caps the request while still letting the label fill a wider allocation.

Measured on `BluetoothPopover`, September 2026: the card sits at **450px** and every content
scenario must land on it — bare, long name, opened, long hero, long value, scanning, and each prompt
page. An uncapped hero subtitle, a long `Services` value or a peer-supplied code each widened it;
`Hero`'s title and subtitle cap at 24, `Row`'s value at 18, and `prompt_code` at 16, which is what
holds it. A 32-character code measured **912** before that last cap existed — twice the card — and
the assertion is what caught it. Assert it: `measure(Horizontal, -1).1` against a floor taken from
the same probe is one line, and it is the only thing standing between a stable card and one that
resizes while it is being read. A `Gtk.Stack` swapping a page answers the same question the same
way: `hhomogeneous` is left at its default, so the card takes the wider of the two pages once rather
than resizing when one replaces the other.

## Deciding a popover's shape

A layout question — drawer, navigation, inline expansion, one shade — is answered on screen, not in
a discussion, and `just preview` is faster than the panel for it.

- **Build each candidate as a board in `var/widget_examples/`**, inside a mock output: a box with
  `overflow: hidden`, a fixed `width-request`, a bar across the top and a chip at its right edge.
  A card that runs past that frame is a card that would run off a real screen, which is the whole
  point — a candidate that looks fine floating on a checkerboard can still be broken.
- **Drive it with the fixtures that already run for every example.** An `$Expandable` needs none —
  it toggles itself, and its `PopoverShell` dims and dismisses as it does on the bar. `expanders`
  binds every `.expander` row to its next-sibling `Gtk.Revealer`; `drawer_nav` binds `nav__<page>` rows, but
  only to the *first* `Gtk.Revealer` holding a `Gtk.Stack`, so a board with several frames can only
  make one of them live.
- **Iterate in `var/widget_examples/_shared.css`**, which hot-reloads, then port the settled values
  into `glimpse.css`. Keep the board on the same numbers afterwards, or the example stops being
  evidence of what ships.
- **Synthetic clicks cannot reach a preview**, but the keyboard can: focus the window with
  `niri msg action focus-window --id`, then `wtype -k Tab` to walk and `wtype -k space` to activate.
  `Return` does not activate a `$Row`; space does.

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

**A dialog is up and nothing anywhere is clickable — not the dialog's buttons, not the popover's
rows, though both still highlight under the pointer.** The catcher is a layer surface at
`Layer::Top` anchored to all four edges, so it sits *above* every ordinary toplevel — including the
`adw::AlertDialog` presented on the panel's host window, which therefore never sees the press. The
other half is GTK's: that dialog is modal within the application, so its grab swallows the presses
the catcher does get. **Close every popover before presenting a dialog** — `App` sends
`panel::Input::ClosePopover` to each panel, and `Catcher::close` tears down properly on its own,
because `finish_close` unmaps the window and `release` runs the dismissal callback the runtime is
waiting on.

**The detail page is missing, or half of it is off the screen.** Placement was computed once, at
open, against a body that has since grown. The catcher re-places on measurement changes now; if a
surface resizes outside that path it has the same bug.

**The row you clicked slides away as it opens.** The card grew on the constrained axis and was
re-placed under the pointer. Grow down.

**The card is a different width every time it opens.** A label's natural width, not its text length.
`max-width-chars`.

**A row's title collapses to an ellipsis while its value is fully readable.** `Row` gives the value
its natural width first. Bound what the applet hands over — three profile names and an ellipsis
rather than six — and cap the label.
