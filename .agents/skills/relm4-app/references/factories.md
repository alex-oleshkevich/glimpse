# Factories — Dynamic Widget Lists

> **Read this when:** You have a list of items that grows/shrinks at runtime (chat messages, todo items, tabs, any dynamic collection). Each item has its own widgets and can send messages independently.

Factories generate widgets from data collections. Use `FactoryVecDeque` instead of `Vec` when you need to display a dynamic list of similar widgets.

---

## Defining a Factory Component

Each element in the factory is a `FactoryComponent` trait implementation with `#[relm4::factory]`:

```rust
use relm4::factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque};

#[derive(Debug)]
struct Counter {
    value: u8,
}

#[derive(Debug)]
enum CounterMsg { Increment, Decrement }

#[derive(Debug)]
enum CounterOutput {
    MoveUp(DynamicIndex),
    MoveDown(DynamicIndex),
}

#[relm4::factory]
impl FactoryComponent for Counter {
    type Init = u8;
    type Input = CounterMsg;
    type Output = CounterOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;   // the container that holds all items

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 10,

            #[name(label)]
            gtk::Label {
                #[watch]
                set_label: &self.value.to_string(),
                set_width_chars: 3,
            },

            gtk::Button {
                set_label: "+",
                connect_clicked => CounterMsg::Increment,
            },
            gtk::Button {
                set_label: "-",
                connect_clicked => CounterMsg::Decrement,
            },
            gtk::Button {
                set_label: "Up",
                connect_clicked[sender, index] => move |_| {
                    sender.output(CounterOutput::MoveUp(index.clone())).unwrap();
                }
            },
            gtk::Button {
                set_label: "Down",
                connect_clicked[sender, index] => move |_| {
                    sender.output(CounterOutput::MoveDown(index.clone())).unwrap();
                }
            },
        }
    }

    fn init_model(value: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self { value }
    }

    fn update(&mut self, msg: Self::Input, _sender: FactorySender<Self>) {
        match msg {
            CounterMsg::Increment => self.value = self.value.wrapping_add(1),
            CounterMsg::Decrement => self.value = self.value.wrapping_sub(1),
        }
    }
}
```

### Key Differences from SimpleComponent

| Aspect | SimpleComponent | FactoryComponent |
|--------|----------------|------------------|
| Model access | `model.field` | `self.field` |
| Init function | `init()` | `init_model()` + macro-generated `init_widgets()` |
| View update | `update_view()` with `#[watch]` on `model.field` | `#[watch]` on `self.field` — update logic is part of the generated view |
| Parent type | N/A | `type ParentWidget = gtk::Box` |
| Root widget | Must be a `Window` for main component | `#[root]` on any widget |
| Builder launch | `Component::builder()` | `FactoryVecDeque::builder()` |
| Public | `#[relm4::component(pub)]` | `#[relm4::factory(pub)]` |

### Async Factory

Factories support async init and update too, similar to `AsyncComponent`:

```rust
#[relm4::factory(async)]
impl AsyncFactoryComponent for Counter {
    // ... same types as FactoryComponent

    async fn init_model(value: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        tokio::time::sleep(Duration::from_secs(1)).await;
        Self { value }
    }

    async fn update(&mut self, msg: Self::Input, _sender: FactorySender<Self>) {
        tokio::time::sleep(Duration::from_secs(1)).await;
        // ...
    }
}

---

## Using FactoryVecDeque in the Parent

### Model

```rust
struct App {
    created_widgets: u8,
    counters: FactoryVecDeque<Counter>,
}
```

### Initialization

```rust
fn init(counter: Self::Init, root: Self::Root, sender: ComponentSender<Self>)
    -> ComponentParts<Self>
{
    let counters = FactoryVecDeque::builder()
        .launch(gtk::Box::default())        // container widget
        .forward(sender.input_sender(), |output| match output {
            CounterOutput::MoveUp(index) => AppMsg::MoveUp(index),
            CounterOutput::MoveDown(index) => AppMsg::MoveDown(index),
            CounterOutput::SendFront(index) => AppMsg::SendFront(index),
        });

    let model = App { created_widgets: counter, counters };

    // Get a reference to the factory's container for use in view!
    let counter_box = model.counters.widget();
    let widgets = view_output!();

    ComponentParts { model, widgets }
}
```

### View — Referencing the Factory Container

```rust
view! {
    gtk::Window {
        gtk::Box {
            #[local_ref]
            counter_box -> gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 5,
            }
        }
    }
}
```

---

## Modifying the Factory — The Guard Pattern

All mutations go through a **guard** (RAII). The factory watches the guard's lifetime — when it drops, the UI is automatically updated with minimal diffs.

```rust
fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
    match msg {
        AppMsg::AddCounter => {
            // Acquire guard → make changes → guard drops → UI updates
            self.counters.guard().push_back(self.created_widgets);
            self.created_widgets = self.created_widgets.wrapping_add(1);
        }
        AppMsg::RemoveCounter => {
            self.counters.guard().pop_back();
        }
        AppMsg::SendFront(index) => {
            self.counters.guard().move_front(index.current_index());
        }
        AppMsg::MoveDown(index) => {
            let i = index.current_index();
            if i + 1 < self.counters.len() {
                self.counters.guard().move_to(i, i + 1);
            }
        }
        AppMsg::MoveUp(index) => {
            let i = index.current_index();
            if i != 0 {
                self.counters.guard().move_to(i, i - 1);
            }
        }
    }
}
```

### Guard Methods

| Method | Description |
|--------|-------------|
| `push_back(value)` | Append element |
| `push_front(value)` | Prepend element |
| `pop_back()` | Remove last element |
| `pop_front()` | Remove first element |
| `insert(index, value)` | Insert at position |
| `remove(index)` | Remove at position |
| `move_to(from, to)` | Move element |
| `move_front(index)` | Move element to front |
| `clear()` | Remove all elements |
| `set(data)` | Replace entire content |
| `len()` | Current length |

---

## DynamicIndex — Stable Identity

`DynamicIndex` tracks an element's position even when items are moved. Always use `DynamicIndex` (not `usize`) for identifying factory items, because indices change when items are rearranged.

```rust
// Access the current usize position:
index.current_index()

// Clone and pass to parent for later use:
sender.output(CounterOutput::MoveUp(index.clone())).unwrap();
```

---

## Position Functions (for Grid Factories)

For non-linear layouts like `gtk::Grid`, implement the `position` function to map each item's index to a grid position:

```rust
fn position(&self, index: &usize) -> GridPosition {
    let idx = *index as i32;

    // 3 items per row:
    GridPosition {
        column: idx % 3,
        row: idx / 3,
        width: 1,
        height: 1,
    }
}
```

```rust
// Chess board pattern:
fn position(&self, index: &usize) -> GridPosition {
    let idx = *index as i32;
    let row = idx / 5;
    let column = (idx % 5) * 2 + row % 2;
    GridPosition { column, row, width: 1, height: 1 }
}
```
