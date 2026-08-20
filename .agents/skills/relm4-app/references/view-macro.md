# `view!` Macro — Complete Reference

> **Read this when:** You're defining UI layout, setting widget properties (label, width, visible), connecting signals (button clicks, text changes), naming widgets for later access, using `#[watch]`/`#[track]`/`#[block_signal]`, or dealing with conditional/match widgets, local references, or container method syntax.

The `view!` macro is Relm4's declarative way to define UI. It generates widget construction code and the `update_view` method that synchronizes widgets with model changes.

---

## Quick Reference — What You Can Do in `view!`

| Goal | Syntax | Example |
|------|--------|--------|
| Widget auto-updates with model | `#[watch]` on a property line | `#[watch] set_label: &model.text` |
| Widget updates only when field changes | `#[track = "expr"]` | `#[track = "model.changed(M::x())"]` |
| Name a widget (access as `widgets.xxx`) | `#[name = "xxx"]` | `#[name = "my_btn"] gtk::Button {...}` |
| Connect a signal | `connect_XXX[sender] => move \|_\| { ... }` | `connect_clicked[sender] => move \|_\| { ... }` |
| Shortcut — send message on signal | `connect_XXX => Msg` | `connect_clicked => AppMsg::Click` |
| Block re-entrant signal | `#[block_signal(h)]` + `@h` | `#[block_signal(h)] set_active: x` + `... @h` |
| Child with explicit method | `method = &Widget { }` | `append = &gtk::Label { }` |
| Child with extra args | `method[a,b] = &Widget { }` | `attach[0,0,1,1] = &gtk::Label { }` |
| Wrap child in Option | `#[wrap(Some)] method = &Widget { }` | `#[wrap(Some)] set_title_widget = &gtk::Box { }` |
| Conditional widget (`if`/`match`) | `if cond { ... } else { ... }` | `if model.flag { gtk::Label { } }` |
| Transition animation on swap | `#[transition = "T"]` | `#[transition = "SlideRight"]` |
| Reference a local widget variable | `#[local]` or `#[local_ref]` | `#[local_ref] my_box -> gtk::Box { }` |
| Call method for each item in iterator | `#[iterate]` on property | `#[iterate] add_css_class: vec!["a", "b"]` |
| Only set property if Some | `setter?: value` | `set_icon_name?: icon` |
| Handle returned widget (e.g. StackPage) | `method = &Widget { } -> { props }` | `add_child = &gtk::Label { } -> { set_title: "P" }` |
| Add custom fields to Widgets struct | `additional_fields! { ... }` | `additional_fields! { flag: bool }` |
| Code before/after generated view update | `fn pre_view() { }` / `fn post_view() { }` | (inside `#[relm4::component]` block) |

---

## Widget Construction

```rust
// Default construction (calls Widget::default())
gtk::Window {
    // properties and children
}

// Constructor function
gtk::Label::new(Some("Hello")) {
    // additional properties
}

// Builder pattern — call .build() at the end
gtk::Label::builder()
    .label("Hello")
    .selectable(true)
    .build() {
    // additional properties
}

// If you need to return the constructed widget for manual setup:
let label_widget = gtk::Label::new(Some("Manual"));
widgets.label.set_label("Updated");
```

### Using Functions That Return Widgets

```rust
// When using a regular function to create a widget, specify the type:
set_property_name = create_some_widget() -> gtk::Box { ... }
```

---

## Naming Widgets

Names become fields on the auto-generated `Widgets` struct, accessible via `widgets.my_name`:

```rust
view! {
    // Attribute syntax (most common)
    #[name = "my_label"]
    gtk::Label { ... }

    // Alternative: parentheses
    #[name(my_label)]
    gtk::Label { ... }

    // Inline naming at assignment site
    set_child: main_label = gtk::Label { ... }
}
```

```rust
// Usage in init():
let widgets = view_output!();
widgets.my_label.set_label("Updated");
```

---

## Properties — Calling Methods

```rust
// Single argument (most setters)
set_label: "text",
set_visible: true,

// Optional — only sets the property if value is Some
set_icon_name?: icon_name,   // icon_name: Option<&str>

// Multiple arguments — use a tuple
set_size_request: (100, 50),
set_default_size: (300, 100),

// Method with no meaningful arguments — use empty tuple
grab_focus: (),
```

### Optional Assignment

Use `?` after the method name to only call the method when the value is `Some`:

```rust
// Internally expands to: if let Some(val) = value { widget.set_property(val); }
set_icon_name?: icon_name,
add_css_class?: if model.valid { None } else { Some("error") },
```

---

## Property Attributes

Attributes are placed on individual property lines, not on the widget itself.

### `#[watch]` — Auto-update on Every Model Change

```rust
// In update_view(), this line is re-executed automatically
gtk::Label {
    #[watch]
    set_label: &format!("Counter: {}", model.counter),
}

// Works with any method, not just setters:
#[watch]
set_visible: model.is_shown,
```

**How it works:** The macro generates code in `update_view` that runs all `#[watch]`-annotated lines. Every time the model updates, the view is refreshed. Always use `#[watch]` for properties that depend on model state.

### `#[track]` — Conditional Updates

```rust
// Basic: only call if the expression evaluates to true
gtk::Label {
    #[track = "model.changed(AppModel::value())"]
    set_label: &format!("Value: {}", model.value),
}

// Shorthand with tracker crate: uses model.changed(Field::method())
#[track]
set_margin_all: model.counter.into(),

// Custom boolean expression using parentheses syntax
#[track(model.value % 10 == 0)]
set_label: &format!("Tick! ({})", model.value),
```

**When to use:** `#[watch]` always runs. When an update is expensive (e.g., complex formatting, triggering side effects), use `#[track]` to gate it.

### `#[block_signal(handler_name)]` — Prevent Re-entrant Signal Fires

Some signals fire when you programmatically change a property. For example, `connect_toggled` on a `ToggleButton` fires when `set_active` is called. This can cause infinite loops.

```rust
gtk::ToggleButton {
    set_label: "Toggle",
    #[watch]
    #[block_signal(toggle_handler)]
    set_active: model.is_active,

    connect_toggled[sender] => move |_| {
        sender.input(AppMsg::Toggle);
    } @toggle_handler,
}
```

**Pattern:** Name your signal handler with `@handler_name`, then use `#[block_signal(handler_name)]` on the conflicting property.

### `#[iterate]` — Iterative Method Calls

```rust
// Calls add_css_class for each class in the Vec
#[iterate]
add_css_class: vec!["class1", "class2", "class3"],
```

---

## Child Widgets & Container Methods

By default, nesting a widget inside a container auto-calls `container.append()`.

```rust
// Default (auto-append)
gtk::Box {
    gtk::Label { ... },
    gtk::Button { ... },
}

// Explicit method — needed when auto-append doesn't work
gtk::Box {
    append = &gtk::Label { ... },
    prepend = &gtk::Button { ... },
}

// With additional arguments (e.g. gtk::Grid::attach)
gtk::Grid {
    attach[0, 0, 1, 2] = &gtk::Label { ... },
}

// Wrap in an Option/enum (e.g. gtk::HeaderBar::set_title_widget takes Option<Widget>)
#[wrap(Some)]
set_title_widget = &gtk::Box { ... },
```

### Reference (`&`) vs Ownership

Use `&` in front of the widget type when the container method takes a reference:

```rust
// Most GTK container methods take &Widget
gtk::Box {
    append = &gtk::Label { ... },     // label.append(widget)
}

// Without &, the macro calls the method with ownership
// gtk::Window::set_child takes Option<Widget>
set_child: gtk::Box { ... },          // window.set_child(Some(box))
```

> **Important:** A common mistake is using `:` instead of `=` for widget assignment. Use `:` for simple property setters, `=` for widget children assignments.

### Returned Widgets

Some container methods return another widget. For example, `gtk::Stack::add_child()` returns a `gtk::StackPage`.

```rust
gtk::Stack {
    add_child = &gtk::Label {
        set_label: "page1",
    } -> {
        set_title: "Page Title",
        set_name: "page-1",
    }
}

// Named returned widget (accessible via widgets.page_name)
gtk::Stack {
    add_child = &gtk::Label { ... } -> PAGE_NAME: gtk::StackPage {
        set_title: "Page Title",
    }
}
```

---

## Signals / Event Handlers

### Simple — Send Message Directly

```rust
connect_clicked => AppMsg::Increment,
```

Expands to: `widget.connect_clicked(move |_| sender.input(AppMsg::Increment))`

Works with any handler where the closure signature is `Fn(T)`, sending the message directly.

### With Cloned Variables

```rust
connect_clicked[sender] => move |_| {
    sender.input(AppMsg::SomeMsg);
},

connect_changed[sender] => move |entry| {
    sender.input(AppMsg::TextChanged(entry.text().to_string()));
},
```

Variables listed in `[brackets]` are cloned before being moved into the closure.

### With Named Variables

```rust
connect_clicked[sender = model.worker.sender().clone()] => move |_| {
    sender.send(WorkerMsg::DoWork).unwrap();
},

connect_toggled[sender, index] => move |btn| {
    if btn.is_active() {
        sender.output(CounterOutput::Moved(index.clone())).unwrap();
    }
},
```

### Named Handler (for `#[block_signal]`)

```rust
connect_toggled[sender] => move |_| {
    sender.input(AppMsg::Increment);
} @toggle_handler,
```

---

## Conditional Widgets (`if` / `match`)

These use `gtk::Stack` internally to swap visible widgets efficiently.

### `if` / `else`

```rust
if model.value % 2 == 0 {
    gtk::Label { set_label: "Value is even" },
} else {
    gtk::Label { set_label: "Value is odd" },
}

// With explicit append method and transition animation
// Two syntax variants:
#[transition = "SlideLeft"]     // string syntax
// or: #[transition(SlideLeft)]  // bare identifier syntax
append = if model.is_loading {
    gtk::Spinner { set_spinning: true },
} else {
    gtk::Label { set_label: &model.data },
},
```

### `match`

```rust
#[transition = "SlideRight"]
append = match model.value {
    0..=2 => {
        gtk::Label { set_label: "Small" },
    }
    3..=9 => {
        gtk::Label { set_label: "Medium" },
    }
    _ => {
        gtk::Label { set_label: "Large" },
    }
},
```

### Destructuring enums in match

```rust
append = match &model.foo {
    Foo::Bar(num) => {
        gtk::SpinButton {
            #[watch]
            set_value: num,
        }
    }
    Foo::Baz(text) => {
        gtk::Label {
            #[watch]
            set_text: &text,
        }
    }
}
```

> **Note:** Always use `#[watch]` or `#[track]` when referencing destructured variables inside match arms, otherwise they won't be accessible due to macro expansion limitations.

---

## Local References

Sometimes you need to create a widget manually before the `view!` macro (e.g., using a factory's container widget), then reference it inside the `view!` block.

### `#[local]` — Owned Widget

```rust
// In init(), before view_output!():
let local_label = gtk::Label::new(Some("Created manually"));

// In view!():
#[local]
local_label -> gtk::Label {
    set_opacity: 0.7,
    set_halign: gtk::Align::Center,
},
```

### `#[local_ref]` — Borrowed Widget

```rust
// In init():
let counter_box = model.counters.widget();  // returns &gtk::Box

// In view!():
#[local_ref]
counter_box -> gtk::Box {
    set_orientation: gtk::Orientation::Vertical,
    set_spacing: 5,
},
```

---

## Iterating Over Collections

```rust
for (index, item) in model.items.iter().enumerate() {
    gtk::Label {
        #[watch]
        set_label: &item.text,
    }

    gtk::Button {
        set_label: "Delete",
        connect_clicked[sender, index] => move |_| {
            sender.input(AppMsg::Delete(index));
        },
    }
}
```

---

## Trait Disambiguation

When two traits have a method with the same name, use fully-qualified syntax:

```rust
TraitName::method_name: value,
```

---

## Additional Fields on Widgets

Add extra fields to the auto-generated Widgets struct:

```rust
additional_fields! {
    custom_flag: bool,
}

// In init():
let custom_flag = true;
let widgets = view_output!();
```

---

## Pre/Post View Hooks

Custom code that runs around the generated `update_view` logic.

```rust
fn pre_view() {
    // Runs before the auto-generated view update code
    // Cannot use early return
}

fn post_view() {
    // Runs after the auto-generated view update code
}
```
