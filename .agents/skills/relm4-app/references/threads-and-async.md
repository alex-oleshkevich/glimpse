# Threads and Async — Keeping the UI Responsive

> **Read this when:** The UI freezes when you click a button, you need to call an API, read a file, do a heavy computation, or spawn any background work that takes longer than ~10ms. Also read this for choosing between Workers, Commands, and AsyncComponent.

When operations take more than a few milliseconds, the UI freezes. Relm4 provides three strategies to avoid this.

## Comparison Table

| Strategy | Sync | Async | Non-blocking | `!Send` | Best For |
|----------|------|-------|-------------|---------|----------|
| **Worker** | ✅ | ❌ | ❌ | ❌ | CPU-bound tasks, one at a time, on separate thread |
| **Commands** | ✅ | ✅ | ✅ | ❌ | Parallel background tasks, both sync & async |
| **AsyncComponent** | ❌ | ✅ | ❌ | ✅ | `await` in init/update with LoadingWidgets |

---

## 1. Workers — Widgetless Components on a Separate Thread

`Worker` runs `update` on a different thread, so it won't block the UI.

### Worker Definition

```rust
use relm4::{Worker, WorkerController};
use std::time::Duration;

#[derive(Debug)]
enum AsyncHandlerMsg { DelayedIncrement, DelayedDecrement }

struct AsyncHandler;

impl Worker for AsyncHandler {
    type Init = ();
    type Input = AsyncHandlerMsg;
    type Output = AppMsg;

    fn init(_init: Self::Init, _sender: ComponentSender<Self>) -> Self { Self }

    fn update(&mut self, msg: AsyncHandlerMsg, sender: ComponentSender<Self>) {
        std::thread::sleep(Duration::from_secs(1));
        match msg {
            AsyncHandlerMsg::DelayedIncrement => sender.output(AppMsg::Increment),
            AsyncHandlerMsg::DelayedDecrement => sender.output(AppMsg::Decrement),
        }.unwrap()
    }
}
```

### Storing and Using a Worker

```rust
struct AppModel {
    counter: u8,
    worker: WorkerController<AsyncHandler>,
}

// In init():
let model = AppModel {
    counter: 0,
    worker: AsyncHandler::builder()
        .detach_worker(())
        .forward(sender.input_sender(), identity),
};

// In view! — send messages to worker:
connect_clicked[sender = model.worker.sender().clone()] => move |_| {
    sender.send(AsyncHandlerMsg::DelayedIncrement).unwrap();
},
```

---

## 2. Commands — Background Tasks with `Component` Trait

Commands require implementing `Component` (not `SimpleComponent`) because they need `CommandOutput` + `update_cmd`.

> **Note:** `SimpleComponent` has a blanket implementation of `Component`. But since `SimpleComponent` doesn't define `CommandOutput`, you must implement `Component` manually to use commands.

### Complete Minimal Example

```rust
use relm4::{gtk, Component, ComponentParts, ComponentSender, RelmApp, RelmWidgetExt};
use std::time::Duration;

struct AppModel {
    result: String,
}

#[derive(Debug)]
enum AppMsg {
    StartTask,
}

#[derive(Debug)]
enum CmdOutput {
    Finished(String),
}

impl Component for AppModel {
    type CommandOutput = CmdOutput;  // ← extra type for background task results
    type Init = ();
    type Input = AppMsg;
    type Output = ();
    type Root = gtk::Window;
    type Widgets = AppWidgets;

    fn init_root() -> Self::Root {
        gtk::Window::new()
    }

    fn init(_: Self::Init, root: Self::Root, sender: ComponentSender<Self>)
        -> ComponentParts<Self>
    {
        let model = AppModel { result: String::new() };
        let vbox = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(5).build();
        let btn = gtk::Button::with_label("Start");
        let label = gtk::Label::new(None);
        btn.connect_clicked(relm4::gtk::glib::clone!(#[strong] sender,
            move |_| { sender.input(AppMsg::StartTask); }
        ));
        vbox.append(&btn);
        vbox.append(&label);
        root.set_child(Some(&vbox));
        let widgets = AppWidgets { label };
        ComponentParts { model, widgets }
    }

    // Note the extra `_root: &Self::Root` param — Component::update signature
    fn update(&mut self, _msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        // Start an async background task via oneshot_command
        sender.oneshot_command(async {
            tokio::time::sleep(Duration::from_secs(2)).await;
            CmdOutput::Finished("Done!".to_string())
        });
    }

    // Handle the command's result
    fn update_cmd(&mut self, msg: Self::CommandOutput, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            CmdOutput::Finished(text) => self.result = text,
        }
    }
}
```

### Sync Command (CPU-bound, non-async)

Use `spawn_oneshot_command` with a closure instead of `oneshot_command` with a future:

```rust
fn update(&mut self, _msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
    sender.spawn_oneshot_command(|| {
        let result = expensive_computation();
        CmdOutput::Finished(result)
    });
}
```

### Multi-message Commands

Use `.command()` to send multiple messages during execution:

```rust
sender.command(async |out| {
    out.send(Msg::Progress(10)).await;
    do_work().await;
    out.send(Msg::Progress(100)).await;
});
```

### Multi-message Commands

```rust
sender.command(async |out| {
    out.send(Msg::Progress(10)).await;
    do_work().await;
    out.send(Msg::Progress(100)).await;
});
```

### Runtime Configuration

```rust
fn main() {
    relm4::RELM_THREADS = 4;    // More async threads
    // ...
}
```

---

## 3. AsyncComponent — Await in Init/Update

Use `async` trait with `#[relm4::component(async)]`.

### Component Definition

```rust
#[relm4::component(async)]
impl AsyncComponent for App {
    type Init = u8;
    type Input = Msg;
    type Output = ();
    type CommandOutput = ();

    view! {
        gtk::Window {
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 5, set_margin_all: 5,

                gtk::Button { set_label: "Increment", connect_clicked => Msg::Increment, },
                gtk::Button { set_label: "Decrement", connect_clicked => Msg::Decrement, },
                gtk::Label {
                    #[watch]
                    set_label: &format!("Counter: {}", model.counter),
                }
            }
        }
    }

    fn init_loading_widgets(root: Self::Root) -> Option<LoadingWidgets> {
        view! {
            #[local]
            root {
                set_title: Some("Loading..."),
                set_default_size: (300, 100),
                #[name(spinner)]
                gtk::Spinner { start: (), set_halign: gtk::Align::Center }
            }
        }
        Some(LoadingWidgets::new(root, spinner))
    }

    async fn init(counter: Self::Init, root: Self::Root, sender: AsyncComponentSender<Self>)
        -> AsyncComponentParts<Self>
    {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let model = App { counter };
        let widgets = view_output!();
        AsyncComponentParts { model, widgets }
    }

    async fn update(&mut self, msg: Self::Input, _sender: AsyncComponentSender<Self>, _root: &Self::Root) {
        tokio::time::sleep(Duration::from_secs(1)).await;
        match msg {
            Msg::Increment => self.counter = self.counter.wrapping_add(1),
            Msg::Decrement => self.counter = self.counter.wrapping_sub(1),
        }
    }
}

fn main() {
    let app = RelmApp::new("com.example.async");
    app.run_async::<App>(0);
}
```

### Key Types for AsyncComponent

| Regular | Async Equivalent |
|---------|-----------------|
| `ComponentSender<T>` | `AsyncComponentSender<T>` |
| `ComponentParts<T>` | `AsyncComponentParts<T>` |
| `app.run::<T>(init)` | `app.run_async::<T>(init)` |
| `#[relm4::component]` | `#[relm4::component(async)]` |
| `fn init(...)` | `async fn init(...)` |
| `fn update(...)` | `async fn update(...)` |
