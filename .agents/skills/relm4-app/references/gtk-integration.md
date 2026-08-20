# GTK4 Integration — Widgets, Libadwaita, CLI, Resources

> **Read this when:** You need to use a specific GTK4 widget (Entry, ComboBoxText, SpinButton, HeaderBar, Stack, etc.), integrate libadwaita, handle CLI arguments, bundle icons/resources, or apply CSS styling.

Relm4 builds on top of `gtk4-rs`. Most GTK4 widgets work directly in the `view!` macro.

---

## 🧩 Widget Reference — GTK4 Widgets in the `view!` Macro

### Entry (Text Input) — text field for user input

```rust
gtk::Entry {
    set_placeholder_text: Some("Enter text..."),
    connect_changed[sender] => move |entry| {
        sender.input(AppMsg::TextChanged(entry.text().to_string()));
    },
}
```

### ComboBoxText (Dropdown)

```rust
gtk::ComboBoxText {
    append_text: "Option 1",
    append_text: "Option 2",
    append_text: "Option 3",
    connect_changed[sender] => move |combo| {
        if let Some(text) = combo.active_text() {
            sender.input(AppMsg::Selected(text.to_string()));
        }
    },
}
```

### CheckButton / ToggleButton

```rust
gtk::CheckButton {
    set_label: "Enable feature",
    #[watch]
    set_active: model.is_enabled,
    connect_toggled[sender] => move |btn| {
        sender.input(AppMsg::Toggled(btn.is_active()));
    },
}

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

### SpinButton

```rust
gtk::SpinButton {
    set_adjustment: Some(&gtk::Adjustment::new(0.0, 0.0, 100.0, 1.0, 10.0, 0.0)),
    set_digits: 2,
    connect_value_changed[sender] => move |btn| {
        sender.input(AppMsg::ValueChanged(btn.value()));
    },
}
```

### ScrolledWindow + ListBox

```rust
gtk::ScrolledWindow {
    set_vexpand: true,
    gtk::ListBox {
        for (index, item) in model.items.iter().enumerate() {
            gtk::ListBoxRow {
                gtk::Label {
                    #[watch]
                    set_label: &item.text,
                },
            }
        }
    },
}
```

### Image

```rust
gtk::Image {
    #[watch]
    set_icon_name: Some("document-open-symbolic"),
    set_pixel_size: 24,
}
```

### HeaderBar (Titlebar)

```rust
gtk::Window {
    // Options: Some(headerbar) to show, None to hide default titlebar
    set_titlebar: Some(&gtk::HeaderBar::new()),

    // Or wrap for greater flexibility:
    #[wrap(Some)]
    set_titlebar = &gtk::HeaderBar {
        pack_start = &gtk::Button { ... },
        pack_end = &gtk::Button { ... },
    },
}
```

### LevelBar

```rust
gtk::LevelBar {
    set_min_value: 0.0,
    set_max_value: 100.0,
    #[watch]
    set_value: model.progress as f64,
}
```

### Stack + StackSwitcher

```rust
gtk::Box {
    gtk::StackSwitcher {
        set_stack: Some(&stack),
    },
    #[name(stack)]
    gtk::Stack {
        add_child = &gtk::Label { set_label: "Page 1" } -> {
            set_title: "First",
        },
        add_child = &gtk::Label { set_label: "Page 2" } -> {
            set_title: "Second",
        },
    },
}
```

---

## 🎨 Libadwaita Support

> Use when: You want the modern GNOME look with Adwaita-styled windows, header bars, and toolbar views.

Add feature to `Cargo.toml`:

```toml
relm4 = { version = "0.10.1", features = ["libadwaita"] }
```

Then use `relm4::adw::*`:

```rust
use relm4::adw::prelude::AdwWindowExt;

// Manual component with libadwaita
impl SimpleComponent for AppModel {
    type Root = adw::Window;

    fn init_root() -> Self::Root {
        adw::Window::builder().title("App").build()
    }

    // In init():
    let vbox = gtk::Box::builder().orientation(Orientation::Vertical).build();
    let header = adw::HeaderBar::builder()
        .title_widget(&gtk::Label::new(Some("App")))
        .build();
    let content = gtk::Box::builder()
        .orientation(Orientation::Vertical)
        .spacing(5).build();

    window.set_content(Some(&vbox));
    vbox.append(&header);
    vbox.append(&content);
    content.append(&inc_button);
    content.append(&label);
}
```

With the view! macro, adw types work directly:

```rust
view! {
    adw::Window {
        set_default_width: 400,
        set_default_height: 300,

        adw::ToolbarView {
            add_top_bar = &adw::HeaderBar { ... },

            #[wrap(Some)]
            set_content = &gtk::Box { ... },
        },
    }
}
```

---

## 🏗️ CLI Argument Handling (with clap)

> Use when: Your app takes custom command-line arguments that would otherwise conflict with GTK's own argument parsing.

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
```

```rust
use clap::Parser;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    non_gtk_arg: bool,

    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
    gtk_options: Vec<String>,
}

fn main() {
    let args = Args::parse();

    // Pass GTK-specific args through
    let mut gtk_args = vec![std::env::args().next().unwrap()];
    gtk_args.extend(args.gtk_options.clone());

    let app = RelmApp::new("com.example.app");
    app.with_args(gtk_args).run::<AppModel>(());
}
```

---

## 📦 Resource Bundles (GResource for Icons/Assets)

> Use when: You need to bundle static assets (icons, images, UI files) into your application binary.

### Directory Structure

```
data/
  icons/
    icon-foo.svg
    icon-bar.svg
  icons.gresource.xml
```

### `icons.gresource.xml`

```xml
<?xml version="1.0" encoding="UTF-8"?>
<gresources>
    <gresource prefix="/com/example/app/icons">
        <file alias="icon-foo.svg">icons/icon-foo.svg</file>
        <file alias="icon-bar.svg">icons/icon-bar.svg</file>
    </gresource>
</gresources>
```

### `build.rs`

```rust
use glib_build_tools::compile_resources;

fn main() {
    compile_resources(
        &["data"],
        "data/icons.gresource.xml",
        "icons.gresource",
    );
}
```

### Loading in `main.rs`

```rust
fn register_icons() {
    gio::resources_register_include!("icons.gresource").unwrap();
    let display = gdk::Display::default().unwrap();
    let theme = gtk::IconTheme::for_display(&display);
    theme.add_resource_path("/com/example/app/icons");
}

fn main() {
    register_icons();
    let app = RelmApp::new("com.example.app");
    app.run::<AppModel>(());
}
```

Then use in view!:
```rust
gtk::Button {
    set_icon_name: "icon-foo",
}
```

---

## 🎨 CSS Styling

> Use when: You want custom colors, padding, borders, animations, or theme overrides beyond what standard GTK4 widgets provide.

```rust
fn main() {
    // Using relm4's helper (simplest):
    relm4::set_global_css(".my-class { background: blue; }");

    // Or via gtk (more control):
    let provider = gtk::CssProvider::new();
    provider.load_from_string(".red-bg { background-color: red; }");
    gtk::StyleContext::add_provider_for_display(
        &gdk::Display::default().unwrap(),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let app = RelmApp::new("com.example.app");
    app.run::<AppModel>(());
}
```

### Adding CSS Classes to Widgets

```rust
// In view!:
gtk::Label {
    add_css_class: "my-class",
    // Optional conditional:
    add_css_class?: if model.valid { None } else { Some("error") },
}

// Using set_class_active for toggle:
#[watch]
set_class_active: ("highlight", model.is_active),
```

---

## 🔧 GTK Inspector — Debugging Your UI

Press **Ctrl+Shift+D** while the app is running to open the GTK inspector.
- Browse the full widget tree
- Inspect/modify any widget's properties live
- Apply custom CSS and see results immediately
- Monitor rendering performance

---

## 📚 Our Ecosystem

Relm4 offers extra crates to speed up development:

- **[relm4-template](https://github.com/Relm4/relm4-template)**: Template for flatpak apps
- **[relm4-icons](https://github.com/Relm4/icons)**: 2000+ ready-to-use icons

  ```toml
  relm4-icons = "0.10.1"
  ```

  ```rust
  fn main() {
      let app = RelmApp::new("com.example.app");
      relm4_icons::initialize_icons();
      app.run::<AppModel>(());
  }
  ```

  Then in view!:
  ```rust
  gtk::Button {
      set_icon_name: "my-icon-name",
  }
  ```

- **[relm4-components](https://docs.rs/relm4-components/)**: Reusable components (AlertDialog, etc.)

---

## ⚡ Trait Disambiguation for Methods

> Use when: You get a "multiple applicable items in scope" error because two traits define the same method.

When multiple traits implement the same method, use fully-qualified syntax:

```rust
view! {
    gtk::WidgetType {
        TraitName::method_name: value,
    }
}
```

Both the trait name or full path work.



| Issue | Cause | Solution |
|-------|-------|----------|
| `Method 'container_add' is missing` | Widget doesn't support `RelmContainerExt` (e.g., `gtk::HeaderBar`, `gtk::Grid`) | Use explicit method name: `pack_start = &Child {}` or `attach[args] = &Child {}` |
| `Private type in public interface` | Generated `Widgets` struct is private | Use `#[relm4::component(pub)]` |
| Message recursion / freeze | Signal fires when `#[watch]` updates a property | Use `#[block_signal(handler_name)]` |
| Sending errors / panic | Dropped `Controller` or using `detach()` without receiver | Store controllers in model, use `detach_runtime()` if needed |
| `Unknown option --some-flag` | GTK parsing user CLI args | Use `with_args(gtk_args)` |
| App frozen during update | Long synchronous operation | Use Worker, Command, or AsyncComponent |
