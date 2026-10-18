use glimpse_sdk::*;
use serde_json::Value;
use std::cell::Cell;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

enum Msg {
    Last(String),
    Wide(bool),
    Wifi(bool),
    Remember(bool),
    Volume(f64),
    Scale(f64),
    Draft(String),
    Drop(usize),
    Choice(usize),
    Month(i64, i64),
    Date(String),
    Toggle(usize, bool),
    Rotate,
    Reveal(bool),
    Expand(bool),
    Page,
    Increment,
    Notify,
    Copy,
    OpenUri,
    Launch,
    Close,
}

struct Item {
    id: &'static str,
    title: &'static str,
    done: bool,
}

struct App {
    seconds: Arc<AtomicU64>,
    count: Cell<i64>,
    wifi: bool,
    remember: bool,
    volume: f64,
    scale: f64,
    draft: String,
    choice: usize,
    drop: usize,
    date: String,
    month: (i64, i64),
    expanded: bool,
    revealed: bool,
    page: usize,
    wide: bool,
    last: String,
    items: Vec<Item>,
}

impl App {
    fn count(&self) -> i64 {
        self.count.get()
    }

    fn save(&self, ctx: &Ctx) {
        let _ = ctx.save_state(&json!({"count": self.count()}));
    }
}

impl glimpse_sdk::App for App {
    type Msg = Msg;

    fn start(&mut self, ctx: &Ctx) {
        if let Some(saved) = ctx.load_state::<Value>()
            && let Some(count) = saved["count"].as_i64()
        {
            self.count.set(count);
        }

        let waker = ctx.waker();
        let seconds = Arc::clone(&self.seconds);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(1));
                seconds.fetch_add(1, Ordering::Relaxed);
                waker.wake();
            }
        });
    }

    fn update(&mut self, ctx: &Ctx, msg: Msg) {
        match msg {
            Msg::Last(value) => self.last = value,
            Msg::Wide(value) => self.wide = value,
            Msg::Wifi(value) => self.wifi = value,
            Msg::Remember(value) => self.remember = value,
            Msg::Volume(value) => self.volume = value,
            Msg::Scale(value) => self.scale = value,
            Msg::Draft(value) => self.draft = value,
            Msg::Drop(value) => self.drop = value,
            Msg::Choice(value) => self.choice = value,
            Msg::Month(year, month) => self.month = (year, month),
            Msg::Date(value) => self.date = value,
            Msg::Toggle(index, done) => {
                if let Some(item) = self.items.get_mut(index) {
                    item.done = done;
                }
            }
            Msg::Rotate => {
                self.items.rotate_left(1);
                self.last = "rotate".to_owned();
            }
            Msg::Reveal(value) => self.revealed = value,
            Msg::Expand(value) => self.expanded = value,
            Msg::Page => self.page = (self.page + 1) % 3,
            Msg::Increment => {
                self.count.set(self.count.get() + 1);
                self.save(ctx);
                self.last = "increment".to_owned();
            }
            Msg::Notify => ctx.notify(
                "Kitchen sink",
                Some("Hello from Rust"),
                Some("applications-graphics-symbolic"),
                None,
            ),
            Msg::Copy => ctx.copy(if self.draft.is_empty() {
                "kitchen sink"
            } else {
                &self.draft
            }),
            Msg::OpenUri => ctx.open_uri("https://example.org"),
            Msg::Launch => ctx.launch("org.gnome.Nautilus"),
            Msg::Close => ctx.close_popover(),
        }
    }

    fn build(&self, ctx: &Ctx) -> Vec<Widget<Msg>> {
        let secs = self.seconds.load(Ordering::Relaxed);
        let count = self.count();
        let title = ctx.options["title"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("Kitchen sink")
            .to_owned();
        let open = self.items.iter().filter(|item| !item.done).count();
        let vertical = ctx
            .placement
            .as_ref()
            .is_some_and(|p| p["orientation"] == "vertical");
        let history: Vec<Option<f64>> = (0..24)
            .map(|i| Some(((secs + i * 7) % 10) as f64 / 10.0))
            .collect();
        let rows: Vec<Widget<Msg>> = self
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                toggle_row()
                    .key(item.id)
                    .title(item.title)
                    .control(ToggleControl::Check)
                    .active(item.done)
                    .on_toggle(move |value| Msg::Toggle(index, value))
                    .into()
            })
            .chain(std::iter::once(
                action_row()
                    .title("Rotate list")
                    .subtitle("Keyed rows move, state follows")
                    .on_activate(|| Msg::Rotate)
                    .into(),
            ))
            .collect();

        let ind = indicator()
            .icon("applications-graphics-symbolic")
            .tooltip(title.clone())
            .badge(open.to_string())
            .attention(count > 10);
        let ind = if count % 2 == 1 {
            ind.dot("#e01b24")
        } else {
            ind
        };

        vec![
            ind.text(if vertical { "" } else { "KS" })
                .on_press(|button| Msg::Last(format!("press {button}")))
                .on_scroll(|dx, dy| Msg::Last(format!("scroll {dx:.1} {dy:.1}")))
                .into(),
            popover()
                .icon("applications-graphics-symbolic")
                .title(title)
                .subtitle(format!("{secs}s uptime"))
                .size(if self.wide {
                    PopoverSize::Wide
                } else {
                    PopoverSize::Regular
                })
                .child(addon().child(switch().active(self.wide).on_toggle(Msg::Wide)))
                .child(
                    notice()
                        .icon("dialog-information-symbolic")
                        .title("Last event")
                        .subtitle(self.last.clone())
                        .severity(Severity::Info)
                        .on_activate(|| Msg::Last("notice".to_owned())),
                )
                .child(
                    section()
                        .title("State and timers")
                        .count(count.to_string())
                        .child(
                            row()
                                .icon("timer-symbolic")
                                .title("Seconds")
                                .value(secs.to_string())
                                .child(readout().value(secs.to_string()).unit("s")),
                        )
                        .child(
                            action_row()
                                .icon("list-add-symbolic")
                                .title("Increment persisted counter")
                                .subtitle("Saved to the state dir")
                                .value(count.to_string())
                                .on_activate(|| Msg::Increment),
                        )
                        .child(row().title("Applet").value(ctx.name.clone()))
                        .child(
                            row()
                                .title("State dir")
                                .value(if ctx.state_dir.is_empty() {
                                    "unset"
                                } else {
                                    "set"
                                })
                                .child(spinner()),
                        ),
                )
                .child(
                    section()
                        .title("Controls")
                        .child(
                            toggle_row()
                                .icon("network-wireless-symbolic")
                                .title("Wi-Fi")
                                .active(self.wifi)
                                .on_toggle(Msg::Wifi),
                        )
                        .child(
                            toggle_row()
                                .title("Remember")
                                .control(ToggleControl::Check)
                                .active(self.remember)
                                .on_toggle(Msg::Remember),
                        )
                        .child(
                            slider_row()
                                .title("Volume")
                                .value(self.volume)
                                .min(0.0)
                                .max(1.0)
                                .step(0.05)
                                .on_move(|value| Msg::Last(format!("move {value:.2}")))
                                .on_change(Msg::Volume)
                                .child(
                                    start().child(
                                        button()
                                            .icon("audio-volume-medium-symbolic")
                                            .class_name(ClassName::Flat)
                                            .on_click(|| Msg::Last("mute".to_owned())),
                                    ),
                                ),
                        )
                        .child(
                            scale()
                                .value(self.scale)
                                .min(0.0)
                                .max(100.0)
                                .step(1.0)
                                .on_change(Msg::Scale),
                        )
                        .child(
                            entry()
                                .placeholder("Type, then Enter")
                                .value(self.draft.clone())
                                .on_change(Msg::Draft)
                                .on_submit(|value| Msg::Last(format!("submit {value}"))),
                        )
                        .child(
                            drop_down()
                                .options(["Small", "Medium", "Large"])
                                .selected(Some(self.drop as i64))
                                .on_select(Msg::Drop),
                        )
                        .child(
                            choice_list()
                                .choices([
                                    Choice::new("First").detail("detail one"),
                                    Choice::new("Second").icon("emblem-ok-symbolic"),
                                    Choice::new("Third").warning(true),
                                ])
                                .selected(Some(self.choice as i64))
                                .on_select(Msg::Choice),
                        )
                        .child(
                            bx().orientation(Orientation::Horizontal)
                                .spacing(6)
                                .child(
                                    toggle_button()
                                        .label("Toggle")
                                        .active(self.wifi)
                                        .on_toggle(Msg::Wifi),
                                )
                                .child(
                                    check_button()
                                        .label("Check")
                                        .active(self.remember)
                                        .on_toggle(Msg::Remember),
                                )
                                .child(
                                    button()
                                        .text("Plain")
                                        .on_click(|| Msg::Last("button".to_owned())),
                                ),
                        )
                        .child(
                            calendar()
                                .year(self.month.0)
                                .month(self.month.1)
                                .selected(self.date.clone())
                                .today("2026-10-15")
                                .events([CalendarEvent::new("2026-10-15").colors([Tone::Accent])])
                                .on_month(Msg::Month)
                                .on_select(Msg::Date),
                        ),
                )
                .child(
                    section()
                        .title("Keyed list")
                        .count(open.to_string())
                        .children(rows),
                )
                .child(
                    section()
                        .title("Data")
                        .child(
                            meter_row()
                                .title("CPU")
                                .level(Some(0.3))
                                .value("30%")
                                .child(image().icon("computer-symbolic")),
                        )
                        .child(
                            meter_row()
                                .title("Memory")
                                .level(Some(0.8))
                                .tone(RowTone::Warning)
                                .value("80%"),
                        )
                        .child(
                            range_bar()
                                .min(0.0)
                                .max(100.0)
                                .low(20.0)
                                .high(80.0)
                                .now(Some((secs % 100) as f64)),
                        )
                        .child(
                            bars()
                                .values(history.clone())
                                .maximum(Some(1.0))
                                .highlight_last(Some(true))
                                .label("History"),
                        )
                        .child(
                            sparkline()
                                .values(history)
                                .fill(true)
                                .show_last(true)
                                .label("Trend"),
                        )
                        .child(fact_list().facts([
                            Fact::new("Counter", count.to_string()),
                            Fact::new("Date", self.date.clone()),
                        ]))
                        .child(
                            action_row()
                                .title("Event dots")
                                .on_activate(|| Msg::Last("dots".to_owned()))
                                .child(
                                    start().child(
                                        dots()
                                            .colors([Tone::Accent, Tone::Success, Tone::Warning])
                                            .size(10)
                                            .max(3),
                                    ),
                                ),
                        )
                        .child(
                            action_row()
                                .title("Color swatch")
                                .on_activate(|| Msg::Last("swatch".to_owned()))
                                .child(start().child(swatch().color("#3584e4").tooltip("Blue"))),
                        )
                        .child(
                            container()
                                .height(Some(44))
                                .background(Paint::Blue)
                                .color(Paint::Light)
                                .align(Align::Center)
                                .child(label().text("Container with a label")),
                        )
                        .child(separator())
                        .child(
                            label()
                                .text("A plain label")
                                .class_name(ClassName::DimLabel),
                        ),
                )
                .child(
                    section()
                        .title("Layout")
                        .child(
                            toggle_row()
                                .title("Reveal more")
                                .active(self.revealed)
                                .on_toggle(Msg::Reveal),
                        )
                        .child(
                            revealer()
                                .revealed(self.revealed)
                                .transition(Transition::SlideDown)
                                .child(row().title("Revealed row").subtitle("Inside a revealer")),
                        )
                        .child(
                            action_row()
                                .title("Next stack page")
                                .value(self.page.to_string())
                                .on_activate(|| Msg::Page),
                        )
                        .child(
                            stack()
                                .visible(self.page as i64)
                                .child(label().text("Page one"))
                                .child(label().text("Page two"))
                                .child(label().text("Page three")),
                        )
                        .child(
                            expander_row()
                                .icon("bluetooth-symbolic")
                                .title("Expander")
                                .open(self.expanded)
                                .on_toggle(Msg::Expand)
                                .child(end().child(image().icon("object-select-symbolic")))
                                .child(
                                    action_row()
                                        .title("Drawer action")
                                        .on_activate(|| Msg::Last("drawer".to_owned())),
                                )
                                .child(meter_row().title("Battery").level(Some(0.6)).value("60%"))
                                .child(
                                    toggle_row()
                                        .title("Drawer toggle")
                                        .active(self.wifi)
                                        .on_toggle(Msg::Wifi),
                                ),
                        ),
                )
                .child(
                    section()
                        .title("Host requests")
                        .child(action_row().title("Notify").on_activate(|| Msg::Notify))
                        .child(action_row().title("Copy draft").on_activate(|| Msg::Copy))
                        .child(
                            action_row()
                                .title("Open example.org")
                                .on_activate(|| Msg::OpenUri),
                        )
                        .child(
                            action_row()
                                .title("Launch files")
                                .on_activate(|| Msg::Launch),
                        )
                        .child(
                            action_row()
                                .title("Close popover")
                                .tone(RowTone::Destructive)
                                .on_activate(|| Msg::Close),
                        ),
                )
                .into(),
        ]
    }
}

fn main() -> ExitCode {
    run(App {
        seconds: Arc::new(AtomicU64::new(0)),
        count: Cell::new(0),
        wifi: true,
        remember: false,
        volume: 0.5,
        scale: 30.0,
        draft: String::new(),
        choice: 0,
        drop: 1,
        date: "2026-10-15".to_owned(),
        month: (2026, 10),
        expanded: false,
        revealed: false,
        page: 0,
        wide: false,
        last: "none".to_owned(),
        items: vec![
            Item {
                id: "a",
                title: "Alpha",
                done: false,
            },
            Item {
                id: "b",
                title: "Bravo",
                done: true,
            },
            Item {
                id: "c",
                title: "Charlie",
                done: false,
            },
        ],
    })
}
