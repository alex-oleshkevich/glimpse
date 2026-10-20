# Previewing a widget

`just preview <path/to/blueprint.blp>` renders one blueprint with the **real widgets** and
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

**Stylesheets.** The theme's `dark.css` loads at `USER + 1`, and only while the
scheme is dark — any sheet named `dark.css` is gated that way. `_shared.css` beside the example loads
at `USER + 2` and `<name>.css` at `USER + 3`, all silently absent-tolerant; the checkerboard sits at
`USER + 4`.
`_shared.css` holds the shared floors — `.column`, `.drawer-page`, `.block`, `.caption`, `.slider`,
`.mute` — so demo-only rules stay out of the shipped `glimpse.css`; a name meaning something in
exactly one example stays in that example's sheet.
`var/widget_examples/` holds the examples. **An example is a
top-level object, never a `template`** — `Builder` cannot instantiate a template whose class does
not exist, so a `template` root renders as nothing.

**A widget is only declarable if it says so.** `PopoverShell` and `Hero` implement `Gtk.Buildable`,
which is what lands `[addon]` (shell) and `[slot]` (`Hero`) in the right internal box, and `Hero` exposes
`title`, `subtitle` and `icon-name` as properties so a `.blp` sets them through the same capped
setters Rust uses. `add_child` must ignore the widget's **own** template children — `init_template`
adds them through the very interface being overridden, so an unguarded override routes `card`
into `body` and panics on an unbound `TemplateChild`. The guard is
`self.body.try_get().is_none()`.

**PyGObject cannot host this.** It cannot override an interface vfunc the parent already implements,
so a `do_add_child` on a Python subclass is accepted, never called. Measured. Any preview host that
needs real widgets has to be Rust.

**`blueprint-compiler lint` has two false positives.** It reports `scrollable_parent` for any extern
`$CustomType` inside a container, and rejects a `Gtk.Adjustment` carrying anything besides `lower`,
`upper` and `value` as `adjustment_prop_order` — the order is not what it checks; adding
`step-increment` fires it regardless of position. There is no way to exclude a single rule, so
`lint-blueprints` strips ANSI colour and fails only on a line that is not `scrollable_parent`.
**Embed our own widgets declaratively** and bind them as ordinary `TemplateChild`s; set adjustment
increments from Rust, and assert them, because nothing in the template guards them any more.
