---
name: widget
description: Building GObject widgets in glimpse-widgets — glib::wrapper and object_subclass, Blueprint templates compiled by build.rs, the three registration points a new template needs, compare-before-write setters, signals with typed connect_ wrappers, and accessibility. Use for any change under crates/glimpse-widgets/, any new .blp file, and any change to build.rs or the gresource manifest. Trigger on the location, not the wording. General GTK4 and libadwaita craft belongs to the gtk4-styles and libadwaita-styles skills; this covers the local mechanics.
---

# widget

A widget in `glimpse-widgets` takes values and emits signals. It never holds a `glimpse-ipc` client,
never knows a topic name, never reaches the daemon — that is what lets it be built in a test with a
literal value and nothing behind it.

**Verified against the tree at `crates/glimpse-widgets/`.** `Panel`, `Indicator` and
`IndicatorGroup` are the three worked examples; when this file and they disagree, they are right.

## The two shapes

**With a template**, when the widget has static internal structure (`Panel`, `Indicator`):
`blueprints/<name>.blp` declares the children, `imp.rs` holds `TemplateChild` fields plus
`bind_template` / `init_template` / `dispose_template`, `mod.rs` holds the public setters.

**Without one**, when every child is created at runtime (`IndicatorGroup`): no `.blp`, a
`BoxLayout` set in `class_init`, and children parented with `insert_after`. A template would declare
nothing.

Subclass `gtk4::Widget`, not `gtk4::Box`, when the contents are computed — it keeps `append` and
`remove` out of the public API so the only way to change them is your own reconcile.

## Registering a new template — three places, all easy to miss

1. `build.rs` — add the pair to the existing `compile_blueprints(&[...])` slice.
2. `resources/glimpse-panel.gresource.xml` — add `<file>widgets/<name>.ui</file>`.
3. `src/lib.rs` — declare the module and re-export the type.

The path in `#[template(resource = ...)]` must be `/me/aresa/GlimpseShell/widgets/<name>.ui`, matching
the gresource `prefix`. Template children bind by string at class-init, so a renamed id fails at the
first instantiation rather than at compile time.

A binary that constructs one of these must have called `register_resources()` first; so must a test.

## Rules

1. **Every setter compares before it writes.** Return early when the value is unchanged. This is not
   an optimisation — an unnecessary `set_*` on a GTK property is a style recomputation and, for CSS
   classes, a visible flicker. `gio::Icon` compares with `IconExt::equal`, which is why
   `set_from_gicon` is never called on an equal icon.

2. **A child that has nothing to show is `visible: false`, not empty.** An empty `Gtk.Label` still
   claims its `BoxLayout` spacing, so a label-only indicator would reserve a phantom icon slot. Set
   the initial `visible: false` in the `.blp` too, or the first frame has the gap.

3. **Truncate with `.chars().take(n)`, never a byte slice.** A multi-byte SSID or track title panics
   on a byte index. Caps live as consts (`LABEL_MAX_CHARS`, `TOOLTIP_MAX_CHARS`) and apply to every
   text inlet including tooltips — tray titles, notification bodies and MPRIS metadata are
   attacker-controlled and unbounded.

4. **No markup setter.** Plain `set_text` only. Assembling markup from another application's string
   is the injection this rule exists to prevent.

5. **Signals get typed `connect_*` wrappers.** `emit_by_name` and `param_types` are checked at
   runtime, not compile time, so a mismatch is a panic in the field. The wrapper plus a test that
   actually emits is what catches it.

6. **Weak references from child to parent.** A group forwarding a child's signal must capture itself
   with `#[weak]`; a strong reference is a cycle that leaks the whole group.

7. **`dispose` unparents.** A `gtk4::Widget` subclass finalized with children still attached emits a
   GTK runtime warning. Template-based widgets call `dispose_template()`; container widgets unparent
   every remaining child.

8. **Decorative children take `accessible-role: presentation`.** An icon beside a labelled indicator
   is otherwise announced twice. The blueprint linter catches the missing case
   (`missing_descriptive_text`) and `just lint` runs it — `build.rs` only compiles, it does not lint.

## What lives elsewhere

- Colours come from libadwaita semantic variables, in `data/themes/<theme>/panel.css`, never as hex
  in Rust. Blueprint for structure, CSS for appearance, Rust for behaviour.
- There is no `stale` or `degraded` rendering. `IndicatorState` was removed; a widget renders what it
  is given and empty when it is given nothing.
- Tests need a display and one thread — see the `testing` skill.

## Definition of done

- The three registration points are all done, and the widget instantiates in a test.
- Every setter has a compare-before-write guard, and a test proves at least one of them
  short-circuits — a `notify::` counter on an internal child is the observable proof when the setter
  is a plain method rather than a GObject property.
- Untrusted text is capped, and the cap is exercised with a multi-byte string.
- `just lint` is clean, including `lint-blueprints`.
- `crates/glimpse-widgets/README.md` documents the widget and the decisions behind it, in the same
  commit. Code carries no comments; the README is where the reasoning goes.
