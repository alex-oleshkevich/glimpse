# Previewing a widget

`just preview <path/to/blueprint.blp> [fixture]` renders one blueprint with the **real widgets** and
reloads on save. The path is resolved against the working directory — there is no search of
`var/widget_examples/`, so a bare `notifications.blp` fails. Compile errors render into the window
in red. It is a cargo example in `glimpse-widgets`, so `Builder` resolves `$PopoverShell` and
`$Hero` to the Rust types. `Esc` closes it. `--scheme dark|light` forces the scheme; a widget is not
checked until it has been seen under both.

Four things it must do, each of which fails silently otherwise:

- **Touch every widget type before building.** A Rust GType registers lazily, so a `$Hero` nothing
  has instantiated is an unknown class to `Builder`; `ensure_types` names them all. An *embedded*
  `TemplateChild<Scrubber>` needs no such call — binding it registers the type — but a blueprint
  example naming `$Scrubber` with no Rust touching it does.
- **Take `ApplicationFlags::NON_UNIQUE`.** The application ID is on the shared session bus, so a
  second preview otherwise hands off to the first, possibly on another monitor, and exits 0 with no
  window and no message.
- **Read `glimpse.css` from disk, not through `Styles::install`**, which loads it with
  `include_str!` — a preview built on that renders a compiled-in copy no edit can reach.
- **Spell a watched path the way the file monitor spells it back.** A relative argument or a `..`
  component compares unequal to the absolute path GIO reports, so every event is discarded;
  `resolve` canonicalises the blueprint and each stylesheet.

Live reload watches each file's **directory** and treats a rename onto the path as a change, because
an editor that saves via a temporary file destroys the inode a file monitor holds. The rename
arrives as `RENAMED`, whose first argument is the temporary path and whose `other_file` is the one
you asked for — match both. Events coalesce over 40ms.

The window paints a checkerboard and every child stays transparent, so whatever the widget does not
paint reads as pattern rather than a flat background it never asked for. **Both halves of that rule
must be scoped to `window.preview`**: GTK4 parents tooltips, popovers and drag icons as direct
children of the window, so a bare `window.preview > * { background-color: transparent; }` blanks
them at `USER + 2` against libadwaita's `tooltip.background`. The transparency rule names the
preview's own slot instead.

It opens floating through a `window-rule` on `^me\.aresa\.WidgetPreview` in the **user's own** niri
config — nothing here installs it. `open-on-workspace "glimpse"` is only half the rule: without a
matching `workspace "glimpse" { open-on-output "eDP-1" }` declaration the workspace is created on
whichever output is focused and previews scatter between runs. Layer-shell was tried first and
rendered nothing.

**Fixtures and stylesheets.** A fixture name defaults to the blueprint's own stem, so `calendar.blp`
shows sample events by being opened. The theme's `dark.css` loads at `USER + 1`, and only while the
scheme is dark — any sheet named `dark.css` is gated that way. `_shared.css` beside the example loads
at `USER + 2` and `<name>.css` at `USER + 3`, all silently absent-tolerant; the checkerboard sits at
`USER + 4`.
`_shared.css` holds the shared floors — `.column`, `.drawer-page`, `.block`, `.caption`, `.slider`,
`.mute` — so demo-only rules stay out of the shipped `glimpse.css`; a name meaning something in
exactly one example stays in that example's sheet.

`var/widget_examples/` holds whole compositions (one `popover_shell_full.blp` per applet popover)
and a states board per widget, `<widget>_states.blp`, showing one instance per state under a
caption. A states board is pure Blueprint whenever the state is a property or a child; a widget fed
through a Rust setter cannot have one until something supplies that data. **An example is a
top-level object, never a `template`** — `Builder` cannot instantiate a template whose class does
not exist, so a `template` root renders as nothing.

Fixtures that run for **every** example, not a named list — gating one on a match arm is what made a
new example's drawer silently inert:

- `expanders` gives any row carrying `.expander` a handler toggling its **next sibling**, and
  complains on stderr when that sibling is not a `Gtk.Revealer`. Matching on position keeps it out
  of the blueprint. `Row`'s rule is still that it navigates rather than expands; this is the
  exception, an audio stream revealing its volume slider.
- `drawer_nav` returns immediately when the tree holds no `Gtk.Revealer`, so a popover written
  entirely in Blueprint — a `Revealer` holding a `Gtk.Stack`, rows carrying `nav__<page>` —
  navigates with no Rust. A `nav__` class with no page behind it is reported on stderr.
- `actions` logs `action: <name>` when a row carrying `action__<name>` fires, giving an example a
  way to show a command without pretending it exists. On a `$SplitRow` it connects `activated` so
  `nav__` keeps the chevron; on anything else that is a `Gtk.Button`, `clicked`. A class on
  something clickable by neither is reported.
- `pager` fills each `$Pager` from its `demo__<case>` class, since its slots come from Rust. An
  unrecognised case and a missing class are both reported.
- `busy` spins anything carrying the `busy` class. A `$SplitRow` spins through its inner `$Row`,
  because `SplitRow` exposes no `busy` property of its own and its `row` is a template child a
  blueprint cannot reach; a `$Row` can equally say `busy: true` in the blueprint and needs no class.
  Anything else carrying the class is reported, since it has no spinner to turn on.
- `expanded` opens each `$Expandable` carrying `state__open`, since `expanded: true` in a blueprint is
  applied before `[details]` exists and is dropped.
- `indicators` configures each `$Indicator` from `icon__<name>`, `overlay__<name>`, `label__<text>`,
  `badge__<text>`, `dot__<hex>` (no `#`: a class cannot carry one),
  `severity__<info|warning|error>`, `state__attention` and `state__notice` — `Indicator` has
  **no GObject properties at all**, so a states board cannot otherwise set one from Blueprint. An
  indicator carrying **none** of those classes is left completely alone: `TrayStrip` builds its own
  chips, and a fixture that wrote to every `Indicator` it found would wipe them. The two flags are
  namespaced because bare `attention` and `notice` are already styling hooks elsewhere — on a
  `Gtk.Image` in `workspaces.blp` and on the `Notice` template — and either landing on an
  `$Indicator` would otherwise be read as state.

**A widget is only declarable if it says so.** `PopoverShell` and `Hero` implement `Gtk.Buildable`,
which is what lands `[hero]`, `[footer]` and `[slot]` in the right internal box, and `Hero` exposes
`title`, `subtitle` and `icon-name` as properties so a `.blp` sets them through the same capped
setters Rust uses. `add_child` must ignore the widget's **own** template children — `init_template`
adds them through the very interface being overridden, so an unguarded override routes `hero_box`
into `content_box` and panics on an unbound `TemplateChild`. The guard is
`self.content_box.try_get().is_none()`.

**PyGObject cannot host this.** It cannot override an interface vfunc the parent already implements,
so a `do_add_child` on a Python subclass is accepted, never called. Measured. Any preview host that
needs real widgets has to be Rust.

**A blueprint cannot open a popover, and a preview must be focused to keep one open.** `active: true`
on a `Gtk.MenuButton` is applied by `Builder` before the button is realized and `GtkMenuButton` drops
it, so no popup is ever created. The class `open-on-map` on the button is what the host uses instead,
and it waits for the window to become *active* — a compositor dismisses a popup belonging to an
unfocused window with `xdg_popup.popup_done` the moment it appears, and a preview opens on its own
workspace. This is how the tray menu board is seen: `just preview var/widget_examples/tray.blp`, then
focus the window.

**`blueprint-compiler lint` has two false positives.** It reports `scrollable_parent` for any extern
`$CustomType` inside a container, and rejects a `Gtk.Adjustment` carrying anything besides `lower`,
`upper` and `value` as `adjustment_prop_order` — the order is not what it checks; adding
`step-increment` fires it regardless of position. There is no way to exclude a single rule, so
`lint-blueprints` strips ANSI colour and fails only on a line that is not `scrollable_parent`.
**Embed our own widgets declaratively** and bind them as ordinary `TemplateChild`s; set adjustment
increments from Rust, and assert them, because nothing in the template guards them any more.
