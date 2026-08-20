# Efficient UI — Tracker and the `#[track]` Attribute

> **Read this when:** Your UI updates are getting slow because too many widgets re-render on every change, or you want to optimize CSS class toggling and expensive formatting. The `tracker` crate provides field-level change detection so widgets only update when their specific data changes.

Relm4 separates data from widgets. To avoid unnecessary UI updates, use **trackers** (field-level change detection) and the `#[track]` attribute to conditionally update widgets.

---

## The `#[tracker::track]` Macro

Add `tracker = "0.2.2"` to `Cargo.toml`, then annotate your model:

```rust
#[tracker::track]
struct AppModel {
    first_icon: &'static str,
    second_icon: &'static str,
    identical: bool,
}
```

This generates:

| Method | Behavior |
|--------|----------|
| `self.field()` | Immutable access (no change tracking) |
| `self.get_field()` | Immutable access |
| `self.set_field(value)` | Set value, marks changed **only if new ≠ old** |
| `self.get_mut_field()` | Mutable reference, **always** marks as changed |
| `self.update_field(fn)` | Update via closure, **always** marks as changed |
| `self.changed(Struct::field())` | Check if field was changed since last `reset()` |
| `self.reset()` | Clear all change flags |

### Required `tracker` Field

The macro adds a `tracker: u8` field. Initialize it to `0`:

```rust
let model = AppModel {
    first_icon: "starred-symbolic",
    second_icon: "starred-symbolic",
    identical: false,
    tracker: 0,   // required by the macro
};
```

### Update Pattern — Reset then Modify

```rust
fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
    self.reset();                    // 1. Clear old tracking state
    match message {
        AppInput::UpdateFirst => {
            self.set_first_icon(new_icon);   // 2. Use setters (marks changes)
        }
        AppInput::UpdateSecond => {
            self.set_second_icon(new_icon);
        }
    }
    self.set_identical(self.first_icon == self.second_icon);
}
```

---

## The `#[track]` Attribute in `view!`

Use `#[track]` with tracker models to only update widgets when relevant fields change.

### Basic — Changes on Any Field

```rust
gtk::Label {
    #[track]
    set_margin_all: model.counter.into(),
    // Only calls set_margin_all when counter changed
},
```

The shorthand `#[track]` expands to `#[track = "model.changed(AppModel::counter())"]`.

### Custom Expression

```rust
gtk::Label {
    #[track = "model.changed(AppModel::value())"]
    set_label: &format!("Value: {}", model.value),
}

// Custom boolean expression
#[track(model.value % 10 == 0)]
set_label: &format!("Tick! ({})", model.value),
```

### CSS Class Toggling with Tracker

```rust
#[root]
gtk::ApplicationWindow {
    #[track = "model.changed(AppModel::identical())"]
    set_class_active: ("identical", model.identical),

    // ... widgets with #[track] on their icon properties
    gtk::Image {
        #[track = "model.changed(AppModel::first_icon())"]
        set_icon_name: Some(model.first_icon),
    },
}

fn main() {
    relm4::set_global_css(".identical { background: #00ad5c; }");
    app.run::<AppModel>(());
}
```

---

## `#[track]` vs `#[watch]` — When to Use What

| Attribute | Behavior | Use When |
|-----------|----------|----------|
| `#[watch]` | Always runs in `update_view` | Simple properties, always updated |
| `#[track]` | Conditionally runs (expressions) | Expensive updates, CSS classes, fine-grained control |

**Rule of thumb:** For most cases, `#[watch]` is sufficient. Use `#[track]` when:
- The update is expensive (complex formatting, triggering side effects)
- You have a tracker-annotated model and want minimal updates
- You need a custom boolean condition

---

## Factory Efficiency

`FactoryVecDeque` uses a **diff algorithm** to perform minimal UI changes. When you drop the guard after modifications, the factory compares old and new states and only applies the necessary widget additions/removals/reorderings. This is far more efficient than rebuilding the entire list.
