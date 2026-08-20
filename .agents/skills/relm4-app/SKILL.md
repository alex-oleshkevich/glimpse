---
name: relm4-app
description: Build GTK4 GUI applications with Relm4. Covers the Elm-architecture (Model-Update-View), the `view!` macro, SimpleComponent/Component/AsyncComponent/Worker traits, child components (Controller + forward), FactoryVecDeque, WidgetTemplate, tracker-based efficient UI, commands, and GTK4/libadwaita integration in Rust. Use this whenever the user mentions Relm4, relm4, the `view!` macro, `#[relm4::component]`, `#[relm4::factory]`, `#[relm4::widget_template]`, SimpleComponent, FactoryComponent, Worker, AsyncComponent, FactoryVecDeque, WidgetTemplate, or asks about building a GTK4 GUI application in Rust. Also use when the user needs patterns like child components, output forwarding, reusable components, command-based background tasks, worker threads, or widget templates. The skill includes comprehensive code examples for every concept.
---

# Relm4 App Developer

> **Relm4 0.10.x** | Rust 2024 (or 2021) | GTK4 | Optional: libadwaita

Relm4 is an idiomatic Rust GUI library inspired by Elm and based on GTK4. Think of it as: **your state lives in a struct → user clicks something → you get a message → you update the struct → the UI syncs automatically.**

---

## 🆘 First: Is Something Not Working?

If the user is stuck, start here. Match their symptom:

| The user says... | Most likely cause | Quick fix |
|---|---|---|
| "My label/text/button won't update when the value changes" | Missing `#[watch]` on the property | `references/view-macro.md` → `#[watch]` section |
| "The whole app freezes when I click a button" | Long sync operation blocking the UI thread | Use commands or workers: `references/threads-and-async.md` |
| "I get `private type in public interface`" | `Widgets` struct is private | Add `(pub)` to the macro: `#[relm4::component(pub)]` |
| "I get `method 'container_add' is missing`" | Widget doesn't support auto-append (e.g., HeaderBar, Grid) | Use explicit method: `pack_start = &Child {}` or `attach[0,0,1,1] = &Child {}` |
| "My toggle/switch causes an infinite loop and crashes" | Signal re-entrance — `set_active` fires `connect_toggled` | `references/view-macro.md` → `#[block_signal]` |
| "The dialog/window closes immediately" | Dropped a `Controller` — runtime destroyed | Store all `Controller<T>` in the model struct |
| "I get `Unknown option --my-arg` when running my app" | GTK is parsing your CLI args | Use `app.with_args(gtk_args)` → `references/gtk-integration.md` → CLI section |
| "The counter app compiles but nothing shows up" | Missing `gtk::init()` or GTK not installed | Check `pkg-config --libs gtk4` works, or install GTK4 dev packages |
| "My factory list is not updating after I push data" | Forgot to use `guard()` — direct pushes without guard don't trigger UI updates | `references/factories.md` → Guard Pattern |

> **First time helping with Relm4?** Read the annotated Quick Start below before anything else. It explains every piece of a Relm4 app.

---

## 🆕 The User Wants to Build Something

Listen for the user's goal, then pick the pattern:

| If they want to... | That's this concept | Start here |
|---|---|---|
| Make a button/label/window | **view! macro** — 90% of UI work | `references/view-macro.md` |
| Make a popup / settings dialog / second window | **Child components** with `Controller<T>` | `references/child-components.md` |
| Show a dynamic list (chat messages, todo items, tabs) | **FactoryVecDeque** | `references/factories.md` |
| Call an API / read a file / sleep without freezing | **Commands** or **Workers** | `references/threads-and-async.md` |
| Reuse the same widget pattern in many places | **WidgetTemplate** | `references/widget-templates.md` |
| Make the UI only update when specific data changes | **Tracker** (`#[tracker::track]`) | `references/tracker-and-efficiency.md` |
| Use dropdowns, text inputs, spinbuttons, checkboxes | **GTK4 widgets in view! macro** | `references/gtk-integration.md` |
| Handle command-line arguments | **clap + with_args** | `references/gtk-integration.md` → CLI |
| Use libadwaita / Adwaita widgets | **adw feature** | `references/gtk-integration.md` → Libadwaita |
| See a complete working app | **Full examples** | `references/complete-examples.md` |
| Write a component without macros | **Manual SimpleComponent** | `references/complete-examples.md` → Manual |

---

## 📖 Annotated Quick Start — What Each Piece Does

```rust
// ── Imports ──────────────────────────────────────────────
use gtk::prelude::*;   // Brings in GTK traits like BoxExt, ButtonExt, GtkWindowExt
use relm4::{gtk, ComponentParts, ComponentSender, RelmApp, RelmWidgetExt, SimpleComponent};
//   gtk              — re-exports gtk4 crate
//   SimpleComponent  — the trait for most UI components
//   ComponentSender  — sends messages to update/model/output
//   ComponentParts   — return type holding { model, widgets }
//   RelmWidgetExt    — adds set_class_active, etc.
//   RelmApp          — app entry point

// ── Model: your app's state ──────────────────────────────
struct AppModel { counter: u8 }
// This is THE source of truth. Everything renders from this.

// ── Messages: what can happen in your app ────────────────
#[derive(Debug)]
enum AppMsg { Increment, Decrement }

// ── Component: ties model, messages, and UI together ─────
#[relm4::component]                    // This macro generates Widgets struct + update_view
impl SimpleComponent for AppModel {
    // Init: the data passed in when app.run::<AppModel>(data) is called
    type Init = u8;
    // Input: messages this component can receive
    type Input = AppMsg;
    // Output: messages sent to parent (() for the root app)
    type Output = ();

    // ── view!: declarative UI ────────────────────────
    // Inside this macro:
    //   model     — auto-generated reference to your AppModel
    //   sender    — auto-generated ComponentSender for sending messages
    //   widgets.XXX — access named widgets by their #[name]
    view! {
        gtk::Window {
            set_title: Some("Counter"),
            set_default_width: 300,
            set_default_height: 100,
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 5, set_margin_all: 5,
                gtk::Button {
                    set_label: "Increment",
                    // Shortcut: connect_clicked => Msg sends sender.input(Msg)
                    connect_clicked => AppMsg::Increment,
                },
                gtk::Button::with_label("Decrement") {
                    connect_clicked => AppMsg::Decrement,
                },
                gtk::Label {
                    // #[watch] = re-run this setter every time the model changes
                    #[watch]
                    set_label: &format!("Counter: {}", model.counter),
                    set_margin_all: 5,
                }
            }
        }
    }

    // ── init: create model + generate widgets ─────────
    fn init(counter: Self::Init,      // the u8 passed from app.run::<AppModel>(0)
            root: Self::Root,          // the gtk::Window auto-created from view!
            sender: ComponentSender<Self>)  // channel for sending messages
        -> ComponentParts<Self> {
        let model = AppModel { counter };
        let widgets = view_output!();   // 🔮 This macro generates all widgets from view!
        ComponentParts { model, widgets }  // Return everything to the runtime

    }

    // ── update: handle messages, mutate model ─────────
    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            // self is &mut AppModel — mutate the model directly
            AppMsg::Increment => self.counter = self.counter.wrapping_add(1),
            AppMsg::Decrement => self.counter = self.counter.wrapping_sub(1),
        }
        // After update returns, the runtime calls update_view automatically
        // #[watch] lines in view! will re-run with the new model values
    }
}

fn main() {
    // The app ID is a reverse-domain identifier (like Android package name)
    let app = RelmApp::new("com.example.counter");
    // app.run::<AppModel>(initial_data) — starts the GTK event loop
    app.run::<AppModel>(0);
}
```

Key mental model: **Relm4 runs in a loop.**
1. Wait for a message
2. Call `update()` — you modify `self` (the model)
3. Call `update_view()` — auto-generated, re-runs all `#[watch]` lines
4. Go back to 1

---

## 🧱 Project Setup

```toml
[dependencies]
relm4 = "0.10.1"
# relm4 = { version = "0.10.1", features = ["libadwaita"] }  # for libadwaita

[build-dependencies]       # only needed for resource bundles
glib-build-tools = "0.17.10"
```

### Installing GTK4

```bash
# Ubuntu/Debian
sudo apt install libgtk-4-dev libadwaita-1-dev

# Fedora
sudo dnf install gtk4-devel libadwaita-devel

# macOS (Homebrew)
brew install gtk4 libadwaita

# Windows — use MSYS2, see https://gtk-rs.org/gtk4-rs/git/book/installation.html
```

---

## 🎯 Key Traits — Quick Decision Guide

| Trait | Use When | Macro |
|-------|----------|-------|
| **`SimpleComponent`** | **90% of cases** — normal UI with buttons, labels, inputs | `#[relm4::component]` |
| **`Component`** | You need background tasks without freezing the UI | (manual `impl`) |
| **`AsyncComponent`** | You need `await` inside `init()` or `update()` | `#[relm4::component(async)]` |
| **`Worker`** | CPU-heavy work on a separate thread (no widgets) | `impl Worker for ...` |
| **`FactoryComponent`** | Dynamic list of similar widgets (todo items, tabs) | `#[relm4::factory]` |
| **`WidgetTemplate`** | Reuse the same widget subtree in multiple places | `#[relm4::widget_template]` |

> **Note:** `SimpleComponent` automatically implements `Component` under the hood. But the extra features (`CommandOutput` + `update_cmd`) are only available if you `impl Component` manually.

---

## 🗺️ File Map — How to Read Reference Files Efficiently

> **Reading strategy:** Each file starts with a `> **Read this when:**` block that tells you the exact scenarios it covers. **Read only that first block first.** If it matches the user's problem, read the whole file. If not, try another file. Within a file, section headers have emoji (🧩 🎨 🏗️ 📦 🎨) so you can jump directly. The `view-macro.md` file has a quick-reference table right after its intro — scan that to find the exact syntax you need.

| File | Contains |
|------|----------|
| `references/view-macro.md` | All `view!` syntax: `#[watch]`, `#[track]`, naming widgets, signals (connect_clicked), containers, conditionals, `#[local_ref]`, `#[block_signal]`, `#[transition]`, `#[iterate]`, `#[wrap]`, `additional_fields!`, `pre_view`/`post_view` |
| `references/child-components.md` | `Controller<T>`, builder pattern (`.launch().forward()`), reusable component design, communicating parent↔child |
| `references/factories.md` | `FactoryVecDeque`, `#[relm4::factory]`, `DynamicIndex`, guard pattern, grid positions, async factory |
| `references/threads-and-async.md` | Workers, Commands (`oneshot_command`, `update_cmd`), `AsyncComponent`, `init_loading_widgets`, comparison table |
| `references/widget-templates.md` | `#[relm4::widget_template]`, `#[template]`, `#[template_child]`, nested access with dot-path |
| `references/tracker-and-efficiency.md` | `#[tracker::track]` macro, `#[track]` attribute, conditional updates, `reset()` pattern |
| `references/gtk-integration.md` | Common GTK4 widgets (Entry, ComboBoxText, SpinButton, etc.), libadwaita, CLI with clap, resource bundles, CSS, ecosystem crates, GTK inspector |
| `references/complete-examples.md` | Full runnable: Todo App, reusable Alert dialog, etc. |
