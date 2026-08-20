# Widget Templates — Reusable UI Sub-trees

> **Read this when:** You're repeating the same widget setup (same margins, same children, same CSS) in multiple places, or you want to build a library of reusable UI components.

Widget templates let you define reusable widget configurations with deeply nested children that can be accessed directly from the template.

---

## Defining a Template

Use `#[relm4::widget_template]` with `impl WidgetTemplate for YourType`:

```rust
use relm4::{gtk, WidgetTemplate};

// Simple template
#[relm4::widget_template]
impl WidgetTemplate for MyBox {
    view! {
        gtk::Box {
            set_margin_all: 10,
            inline_css: "border: 2px solid blue",
        }
    }
}

// With initial state
#[relm4::widget_template]
impl WidgetTemplate for MySpinner {
    view! {
        gtk::Spinner {
            set_spinning: true,
        }
    }
}
```

### Making Templates Public

```rust
#[relm4::widget_template(pub)]
impl WidgetTemplate for MyButton {
    view! { ... }
}
```

---

## Nested Templates with Template Children

Templates can nest other templates and define **template children** — deeply nested widgets that are accessible by name:

```rust
#[relm4::widget_template]
impl WidgetTemplate for CustomBox {
    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_margin_all: 5,
            set_spacing: 5,

            #[template]
            MyBox {
                #[template]
                MySpinner,

                #[template]
                MyBox {
                    #[template]
                    MySpinner,

                    #[template]
                    MyBox {
                        #[template]
                        MySpinner,

                        // Template child — accessible from outside
                        #[name = "child_label"]
                        gtk::Label {
                            set_label: "Default text",
                        }
                    }
                }
            }
        }
    }
}
```

**Key insight:** Any widget named with `#[name]` inside a template becomes a **template child**, no matter how deeply nested.

---

## Using Templates in a Component

### `#[template]` — Insert a Template

```rust
view! {
    gtk::Window {
        #[template]
        CustomBox {
            // Additional children added to the template's root
            gtk::Button { set_label: "Increment", ... },
            gtk::Button { set_label: "Decrement", ... },

            // Access template children
            #[template_child]
            child_label {
                #[watch]
                set_label: &format!("Counter: {}", model.counter),
            }
        },
    }
}
```

### `#[template_child]` — Access Nested Children

Use `#[template_child]` followed by the name of the child. You can set new properties and connect signals:

```rust
#[template_child]
child_label {
    #[watch]
    set_label: &format!("Counter: {}", model.counter),
}

#[template_child]
child_label {
    set_label: "Overridden",
}
```

---

## Accessing Nested Template Elements (v0.6.2+)

When templates are nested, use dot-path syntax to access deeply nested children:

### Defining Templates

```rust
#[relm4::widget_template]
impl WidgetTemplate for HomePage {
    view! {
        gtk::Box {
            #[name = "btn_go_settings"]
            gtk::Button {
                #[wrap(Some)]
                set_child = &gtk::Image { set_icon_name: Some("emblem-system-symbolic") },
            },
        }
    }
}

#[relm4::widget_template]
impl WidgetTemplate for SettingsPage {
    view! {
        gtk::Box {
            #[name = "btn_dark_mode"]
            gtk::Button { ... },
            #[name = "btn_go_homepage"]
            gtk::Button { ... },
        }
    }
}

#[relm4::widget_template]
impl WidgetTemplate for MainWindow {
    view! {
        gtk::Window {
            gtk::Box {
                #[name(stk_pages)]
                gtk::Stack {
                    #[template]
                    #[name = "home_page"]
                    add_child = &HomePage {} -> { set_name: "main", },

                    #[template]
                    #[name = "settings_page"]
                    add_child = &SettingsPage {} -> { set_name: "settings", },
                },
            },
        }
    }
}
```

### Using Nested Access

```rust
view! {
    #[template]
    MainWindow {
        // Nested template child access via dot-path
        #[template_child]
        settings_page.btn_dark_mode {
            connect_clicked => Message::DarkMode,
        },

        #[template_child]
        settings_page.btn_go_homepage {
            connect_clicked => Message::PageHome,
        },

        #[template_child]
        home_page.btn_go_settings {
            connect_clicked => Message::PageSettings,
        },

        #[template_child]
        stk_pages {
            #[watch]
            set_visible_child_name: model.current_page,
        }
    },
}
```

---

## Template Order Note

Templates initialize their children before additional children are appended. This means:

```rust
#[template]
CustomBox {
    // These two buttons will appear AFTER the template's child_label
    gtk::Button { ... },
    gtk::Button { ... },
}
```

Workaround: use `prepend`, `append`, or `insert_child_after` on the template's container, or split into smaller templates to control order.
