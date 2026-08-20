# Complete, Runnable Examples

> **Read this when:** You want a complete, working app to start from or adapt. Each example is self-contained and compilable.

---

## 1. Todo App — Full CRUD with Patterns

This example demonstrates: `#[watch]`, `connect_changed`, `connect_clicked` with sender, `for` iteration, conditional CSS classes.

```rust
use gtk::prelude::*;
use relm4::{gtk, ComponentParts, ComponentSender, RelmApp, RelmWidgetExt, SimpleComponent};

#[derive(Debug, Clone)]
struct Todo {
    id: u32,
    text: String,
    completed: bool,
}

#[derive(Debug)]
enum AppMsg {
    AddTodo,
    ToggleTodo(u32),
    DeleteTodo(u32),
    TextInputChanged(String),
}

struct AppModel {
    todos: Vec<Todo>,
    text_input: String,
    next_id: u32,
}

#[relm4::component]
impl SimpleComponent for AppModel {
    type Init = ();
    type Input = AppMsg;
    type Output = ();

    view! {
        gtk::Window {
            set_title: Some("Todo App"),
            set_default_width: 400,
            set_default_height: 300,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_margin_all: 10,
                set_spacing: 10,

                // Input section
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 5,

                    gtk::Entry {
                        set_placeholder_text: Some("New todo..."),
                        set_hexpand: true,
                        connect_changed[sender] => move |entry| {
                            sender.input(AppMsg::TextInputChanged(entry.text().to_string()));
                        },
                    },

                    gtk::Button {
                        set_label: "Add",
                        connect_clicked => AppMsg::AddTodo,
                    },
                },

                // Todo list
                gtk::ScrolledWindow {
                    set_vexpand: true,

                    gtk::ListBox {
                        for (index, todo) in model.todos.iter().enumerate() {
                            gtk::ListBoxRow {
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 10,
                                    set_margin_all: 5,

                                    gtk::CheckButton {
                                        #[watch]
                                        set_active: todo.completed,
                                        connect_toggled[sender, index] => move |_| {
                                            sender.input(AppMsg::ToggleTodo(index as u32));
                                        },
                                    },

                                    gtk::Label {
                                        #[watch]
                                        set_label: &todo.text,
                                        set_hexpand: true,
                                        add_css_class?: if todo.completed { Some("dim-label") } else { None },
                                    },

                                    gtk::Button {
                                        set_label: "Delete",
                                        connect_clicked[sender, index] => move |_| {
                                            sender.input(AppMsg::DeleteTodo(index as u32));
                                        },
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn init(_: Self::Init, root: Self::Root, sender: ComponentSender<Self>)
        -> ComponentParts<Self>
    {
        let model = AppModel {
            todos: vec![],
            text_input: String::new(),
            next_id: 0,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            AppMsg::AddTodo => {
                if !self.text_input.trim().is_empty() {
                    self.todos.push(Todo {
                        id: self.next_id,
                        text: self.text_input.clone(),
                        completed: false,
                    });
                    self.next_id += 1;
                    self.text_input.clear();
                }
            }
            AppMsg::ToggleTodo(index) => {
                if let Some(todo) = self.todos.get_mut(index as usize) {
                    todo.completed = !todo.completed;
                }
            }
            AppMsg::DeleteTodo(index) => {
                if (index as usize) < self.todos.len() {
                    self.todos.remove(index as usize);
                }
            }
            AppMsg::TextInputChanged(text) => {
                self.text_input = text;
            }
        }
    }
}

fn main() {
    let app = RelmApp::new("com.example.todo");
    app.run::<AppModel>(());
}
```

---

## 2. Reusable Alert Dialog Component

Demonstrates: `#[relm4::component(pub)]`, settings as `Init`, `#[doc(hidden)]` internal messages, `Controller`, builder with `.transient_for()`, `.forward()`, named widgets, manual post-init.

### The Component

```rust
use gtk::prelude::*;
use relm4::prelude::*;
use relm4::Controller;

/// Configuration for the alert dialog
pub struct AlertSettings {
    pub text: String,
    pub secondary_text: Option<String>,
    pub is_modal: bool,
    pub destructive_accept: bool,
    pub confirm_label: String,
    pub cancel_label: String,
    pub option_label: Option<String>,
}

pub struct Alert {
    settings: AlertSettings,
    is_active: bool,
}

/// Messages sent to the alert
#[derive(Debug)]
pub enum AlertMsg {
    Show,
    #[doc(hidden)]
    Response(gtk::ResponseType),
}

/// User action result
#[derive(Debug)]
pub enum AlertResponse {
    Confirm,
    Cancel,
    Option,
}

#[relm4::component(pub)]
impl SimpleComponent for Alert {
    type Widgets = AlertWidgets;
    type Init = AlertSettings;
    type Input = AlertMsg;
    type Output = AlertResponse;

    view! {
        #[name = "dialog"]
        gtk::MessageDialog {
            set_message_type: gtk::MessageType::Question,
            #[watch]
            set_visible: model.is_active,
            connect_response[sender] => move |_, response| {
                sender.input(AlertMsg::Response(response));
            },

            set_text: Some(&model.settings.text),
            set_secondary_text: model.settings.secondary_text.as_deref(),
            set_modal: model.settings.is_modal,
            add_button: (&model.settings.confirm_label, gtk::ResponseType::Accept),
            add_button: (&model.settings.cancel_label, gtk::ResponseType::Cancel),
        }
    }

    fn init(settings: AlertSettings, root: Self::Root, sender: ComponentSender<Self>)
        -> ComponentParts<Self>
    {
        let model = Alert { settings, is_active: false };
        let widgets = view_output!();

        // Post-init: third option button
        if let Some(label) = &model.settings.option_label {
            widgets.dialog.add_button(label, gtk::ResponseType::Other(0));
        }

        // Post-init: destructive styling on accept button
        if model.settings.destructive_accept {
            let btn = widgets.dialog.widget_for_response(gtk::ResponseType::Accept)
                .expect("No accept button");
            btn.add_css_class("destructive-action");
        }

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: AlertMsg, sender: ComponentSender<Self>) {
        match msg {
            AlertMsg::Show => { self.is_active = true; }
            AlertMsg::Response(ty) => {
                self.is_active = false;
                sender.output(match ty {
                    gtk::ResponseType::Accept => AlertResponse::Confirm,
                    gtk::ResponseType::Other(_) => AlertResponse::Option,
                    _ => AlertResponse::Cancel,
                }).unwrap();
            }
        }
    }
}
```

### Using the Alert

```rust
struct App {
    counter: u8,
    dialog: Controller<Alert>,
}

#[derive(Debug)]
enum AppMsg {
    Increment,
    CloseRequest,
    Close,
    Ignore,
}

#[relm4::component]
impl SimpleComponent for App {
    type Input = AppMsg;
    type Output = ();
    type Init = ();

    view! {
        main_window = gtk::ApplicationWindow {
            set_title: Some("Alert Demo"),
            connect_close_request[sender] => move |_| {
                sender.input(AppMsg::CloseRequest);
                gtk::glib::Propagation::Stop
            },
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 5, set_margin_all: 5,

                gtk::Button {
                    set_label: "Increment",
                    connect_clicked[sender] => move |_| {
                        sender.input(AppMsg::Increment);
                    },
                },
                gtk::Button {
                    set_label: "Close",
                    connect_clicked[sender] => move |_| {
                        sender.input(AppMsg::CloseRequest);
                    },
                },
                gtk::Label {
                    #[watch]
                    set_label: &format!("Counter: {}", model.counter),
                },
            },
        }
    }

    fn init(_: (), root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let model = App {
            counter: 0,
            dialog: Alert::builder()
                .transient_for(&root)
                .launch(AlertSettings {
                    text: String::from("Close without saving?"),
                    secondary_text: Some(String::from("You have unsaved changes")),
                    confirm_label: String::from("Close"),
                    cancel_label: String::from("Cancel"),
                    option_label: Some(String::from("Save")),
                    is_modal: true,
                    destructive_accept: true,
                })
                .forward(sender.input_sender(), |resp| match resp {
                    AlertResponse::Confirm => AppMsg::Close,
                    AlertResponse::Cancel => AppMsg::Ignore,
                    AlertResponse::Option => AppMsg::Close,  // save then close
                }),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: AppMsg, _sender: ComponentSender<Self>) {
        match msg {
            AppMsg::Increment => self.counter = self.counter.wrapping_add(1),
            AppMsg::CloseRequest => { self.dialog.emit(AlertMsg::Show); }
            AppMsg::Close => { relm4::main_application().quit(); }
            AppMsg::Ignore => {}
        }
    }
}

fn main() {
    let app = RelmApp::new("com.example.alert");
    app.run::<App>(());
}
```

---

## 3. Factory — Dynamic Counter List

> See `references/factories.md` for the full code including `FactoryVecDeque`, `DynamicIndex`, guard pattern, `#[local_ref]`.

---

## 4. Tracker — Efficient Icon Switcher

> See `references/tracker-and-efficiency.md` for the full code including `#[tracker::track]`, `#[track]` attribute, CSS class toggling.

---

## 5. Minimal AsyncComponent with Loading Spinner

> See `references/threads-and-async.md` for the full code including `init_loading_widgets`, `run_async`.

---

## 6. Minimal Manual Component (No Macro)

For cases where you don't use the `view!` macro:

```rust
use gtk::glib::clone;
use gtk::prelude::*;
use relm4::{gtk, ComponentParts, ComponentSender, RelmApp, RelmWidgetExt, SimpleComponent};

struct AppModel { counter: u8 }

struct AppWidgets { label: gtk::Label }

#[derive(Debug)]
enum AppMsg { Increment, Decrement }

impl SimpleComponent for AppModel {
    type Input = AppMsg;
    type Output = ();
    type Init = u8;
    type Root = gtk::Window;
    type Widgets = AppWidgets;

    fn init_root() -> Self::Root {
        gtk::Window::builder()
            .title("Manual App")
            .default_width(300)
            .default_height(100)
            .build()
    }

    fn init(counter: Self::Init, window: Self::Root, sender: ComponentSender<Self>)
        -> ComponentParts<Self>
    {
        let model = AppModel { counter };

        let vbox = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(5).build();
        let inc_btn = gtk::Button::with_label("Increment");
        let dec_btn = gtk::Button::with_label("Decrement");
        let label = gtk::Label::new(Some(&format!("Counter: {}", model.counter)));
        label.set_margin_all(5);

        window.set_child(Some(&vbox));
        vbox.set_margin_all(5);
        vbox.append(&inc_btn);
        vbox.append(&dec_btn);
        vbox.append(&label);

        inc_btn.connect_clicked(clone!(#[strong] sender, move |_| {
            sender.input(AppMsg::Increment);
        }));
        dec_btn.connect_clicked(clone!(#[strong] sender, move |_| {
            sender.input(AppMsg::Decrement);
        }));

        let widgets = AppWidgets { label };
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: AppMsg, _sender: ComponentSender<Self>) {
        match msg {
            AppMsg::Increment => self.counter = self.counter.wrapping_add(1),
            AppMsg::Decrement => self.counter = self.counter.wrapping_sub(1),
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets.label.set_label(&format!("Counter: {}", self.counter));
    }
}

fn main() {
    RelmApp::new("com.example.manual").run::<AppModel>(0);
}
```
