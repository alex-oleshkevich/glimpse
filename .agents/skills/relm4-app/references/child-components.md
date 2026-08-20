# Child Components

> **Read this when:** You need a popup dialog, a settings window, a header bar that updates the main app, or you want to split your UI into reusable, independent modules. Also read this if you see errors about dropped `Controller`s or panics on `sender.output()`.

Components are independent parts of your app that communicate in a parent-child hierarchy. Each component stores child components as `Controller<T>` in its model.

---

## Parent Model Pattern — Storing Controllers

```rust
use relm4::Controller;

struct AppModel {
    mode: AppMode,
    header: Controller<HeaderModel>,   // child component controller
    dialog: Controller<DialogModel>,
}
```

## Builder Pattern: Launch + Forward

In `init()`, construct each child component using the builder pattern:

```rust
fn init(params: Self::Init, root: Self::Root, sender: ComponentSender<Self>)
    -> ComponentParts<Self>
{
    // Builder chain:
    //   1. builder()    — create the builder
    //   2. .transient_for(&root) — optional GTK window relationship
    //   3. .launch(init_data)     — start the component
    //   4. .forward(sender, map)  — transform Output → parent Input

    let header: Controller<HeaderModel> = HeaderModel::builder()
        .launch(())
        .forward(sender.input_sender(), |msg| match msg {
            HeaderOutput::View => AppMsg::SetMode(AppMode::View),
            HeaderOutput::Edit => AppMsg::SetMode(AppMode::Edit),
        });

    let dialog = DialogModel::builder()
        .transient_for(&root)     // dialog's parent window
        .launch(true)
        .forward(sender.input_sender(), |msg| match msg {
            DialogOutput::Close => AppMsg::Close,
        });

    let model = AppModel { mode: params, header, dialog };
    let widgets = view_output!();
    ComponentParts { model, widgets }
}
```

### Key Builder Methods

| Method | Purpose |
|--------|---------|
| `.launch(init_data)` | Start the component with its `Init` parameter |
| `.forward(sender, closure)` | Route the component's `Output` to parent's `Input` via a mapping closure |
| `.detach()` | Start without forwarding outputs (no parent listener) |
| `.transient_for(&root)` | Set parent window for dialogs (calls `set_transient_for`) |

---

## Communicating with Child Components

### Parent → Child: Send Input Messages

```rust
// Via the Controller's sender:
fn update(&mut self, msg: AppMsg, _sender: ComponentSender<Self>) {
    match msg {
        AppMsg::CloseRequest => {
            self.dialog.sender().send(DialogInput::Show).unwrap();
            // or use .emit() for a shorthand:
            self.dialog.emit(DialogInput::Show);
        }
        AppMsg::Close => {
            relm4::main_application().quit();
        }
    }
}
```

### Child → Parent: Output Messages

The child component defines an `Output` type and sends via `sender.output(...)`:

```rust
// Child component's update or signal handler:
sender.output(DialogOutput::Close).unwrap();
```

The parent maps this to its own message type in the `forward` closure:

```rust
.forward(sender.input_sender(), |msg| match msg {
    DialogOutput::Close => AppMsg::Close,
})
```

### Accessing Child Widgets

```rust
// Get the child's root widget:
model.header.widget()  // returns &gtk::HeaderBar

// Use it in the parent's view!:
view! {
    gtk::Window {
        set_titlebar: Some(model.header.widget()),
        // ...
    }
}
```

---

## Reusable Component Pattern

Design components for reuse by making `Init` a settings struct, using `#[doc(hidden)]` for internal messages, and exposing a clean `Input`/`Output` API.

### Settings Struct as `Init`

```rust
pub struct AlertSettings {
    pub text: String,
    pub secondary_text: Option<String>,
    pub is_modal: bool,
    pub destructive_accept: bool,
    pub confirm_label: String,
    pub cancel_label: String,
    pub option_label: Option<String>,
}
```

### Internal vs External Messages

```rust
#[derive(Debug)]
pub enum AlertMsg {
    /// Message sent by parent to show the dialog
    Show,

    #[doc(hidden)]  // hides from docs — internal use only
    Response(gtk::ResponseType),
}
```

### Clean Output Type

```rust
#[derive(Debug)]
pub enum AlertResponse {
    Confirm,
    Cancel,
    Option,
}
```

### Making Widgets Public

Use `#[relm4::component(pub)]` to make the generated `Widgets` struct public:

```rust
#[relm4::component(pub)]
impl SimpleComponent for Alert {
    type Widgets = AlertWidgets;   // explicit type name
    type Init = AlertSettings;
    type Input = AlertMsg;
    type Output = AlertResponse;

    view! { ... }
    // ...
}
```

### Post-Init Modifications Using Named Widgets

```rust
fn init(settings: AlertSettings, root: Self::Root, sender: ComponentSender<Self>)
    -> ComponentParts<Self>
{
    let model = Alert { settings, is_active: false };
    let widgets = view_output!();

    // Manual post-init widget modifications
    if let Some(option_label) = &model.settings.option_label {
        widgets.dialog.add_button(option_label, gtk::ResponseType::Other(0));
    }

    ComponentParts { model, widgets }
}
```

---

## Common Pitfalls

### Dropping Controllers

Dropping a `Controller` drops the component runtime. Sending messages to it afterward will panic:

```rust
// ❌ BAD — controller must be stored
let dialog = DialogModel::builder().launch(true).detach();
// dialog dropped here → receiver gone

// ✅ GOOD — store in model
self.dialog = DialogModel::builder().launch(true).forward(...);
```

### Sending Output After `detach()`

If you call `.detach()` instead of `.forward()`, there's no output receiver. `sender.output()` will return an error. Use `unwrap_or(())` if ignoring is acceptable.

### Message Recursion

If `update_view` triggers a signal that sends the same message, you get an infinite loop. Use `#[block_signal]` (see `view-macro.md`) to prevent re-entrant signals.

### Forward vs Detach

```rust
// Forward — output messages are routed to parent input
let child = MyComp::builder()
    .launch(())
    .forward(parent_sender, |out| ParentMsg::from(out));

// Detach — no output routing (child outputs are discarded)
let child = MyComp::builder()
    .launch(())
    .detach();

// Detach worker — for Worker trait components
let worker = AsyncHandler::builder()
    .detach_worker(())
    .forward(sender.input_sender(), identity);
```
