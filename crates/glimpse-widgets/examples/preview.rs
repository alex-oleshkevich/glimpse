use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use clap::{Parser, ValueEnum};
use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};

/// Scoped to the preview's own window. On bare `window` it also paints every tooltip, popover and
/// menu GTK creates, which then render transparent over the checkerboard.
const CHECKERBOARD: &str = "
window.preview {
  background-color: #888888;
  background-image:
    linear-gradient(45deg, #6f6f6f 25%, transparent 25%, transparent 75%, #6f6f6f 75%),
    linear-gradient(45deg, #6f6f6f 25%, transparent 25%, transparent 75%, #6f6f6f 75%);
  background-size: 24px 24px;
  background-position: 0 0, 12px 12px;
}
window.preview > .preview__slot { background-color: transparent; }
";

const SETTLE: Duration = Duration::from_millis(40);

#[derive(Parser)]
#[command(about = "Render one blueprint with the real widgets, and reload it on every save.")]
struct Cli {
    /// Blueprint to render.
    blueprint: PathBuf,

    /// Sample data to put in after the build. Defaults to the blueprint's own name.
    fixture: Option<String>,

    /// Color scheme to render under. The whole token vocabulary flips at once, so a widget is not
    /// checked until it has been seen under both.
    #[arg(long, value_enum, default_value_t = Scheme::System)]
    scheme: Scheme,
}

#[derive(Clone, Copy, ValueEnum)]
enum Scheme {
    System,
    Light,
    Dark,
}

impl From<Scheme> for adw::ColorScheme {
    fn from(scheme: Scheme) -> Self {
        match scheme {
            Scheme::System => adw::ColorScheme::Default,
            Scheme::Light => adw::ColorScheme::ForceLight,
            Scheme::Dark => adw::ColorScheme::ForceDark,
        }
    }
}

fn main() -> glib::ExitCode {
    let cli = Cli::parse();
    let fixture = cli.fixture.filter(|name| !name.is_empty()).or_else(|| {
        cli.blueprint
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
    });

    let app = adw::Application::builder()
        .application_id(format!(
            "me.aresa.WidgetPreview.{}",
            cli.blueprint
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
        ))
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_activate(move |app| {
        activate(app, &cli.blueprint, fixture.as_deref(), cli.scheme.into())
    });
    app.run_with_args::<&str>(&["preview"])
}

fn activate(
    app: &adw::Application,
    blueprint: &Path,
    fixture: Option<&str>,
    scheme: adw::ColorScheme,
) {
    let blueprint = &resolve(blueprint);
    adw::StyleManager::default().set_color_scheme(scheme);
    glimpse_widgets::register_resources().expect("widget resources");
    ensure_types();

    let sheets = vec![
        (resolve(&builtin_css()), provider()),
        (resolve(&theme_css()), provider()),
        (resolve(&blueprint.with_extension("css")), provider()),
    ];
    let checkerboard = provider();
    checkerboard.load_from_string(CHECKERBOARD);

    if let Some(display) = gdk::Display::default() {
        let priorities = [
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            gtk4::STYLE_PROVIDER_PRIORITY_USER,
            gtk4::STYLE_PROVIDER_PRIORITY_USER + 1,
        ];
        for ((_, provider), priority) in sheets.iter().zip(priorities) {
            gtk4::style_context_add_provider_for_display(&display, provider, priority);
        }
        gtk4::style_context_add_provider_for_display(
            &display,
            &checkerboard,
            gtk4::STYLE_PROVIDER_PRIORITY_USER + 2,
        );
    }

    let slot = gtk4::Box::builder()
        .css_classes(["preview__slot"])
        .orientation(gtk4::Orientation::Vertical)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .title(blueprint.file_name().unwrap_or_default().to_string_lossy())
        .child(&slot)
        .css_classes(["preview"])
        .build();

    let keys = gtk4::EventControllerKey::new();
    let closing = window.clone();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape {
            closing.close();
        }
        glib::Propagation::Proceed
    });
    window.add_controller(keys);
    window.present();

    load_styles(&sheets);
    build(&slot, blueprint, fixture);

    let monitors = watch(blueprint, &slot, sheets, fixture.map(str::to_owned));
    unsafe { window.set_data("preview-monitors", monitors) };
}

/// A watched path must be spelled the way the file monitor spells it back. A relative argument or
/// a `..` component compares unequal to the absolute, resolved path GIO reports, and every event is
/// then discarded — the watch arms, stays silent, and looks exactly like a tool that does not
/// reload.
fn resolve(path: &Path) -> PathBuf {
    std::fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn provider() -> gtk4::CssProvider {
    let provider = gtk4::CssProvider::new();
    provider.connect_parsing_error(|_, section, error| {
        eprintln!("stylesheet {}: {error}", section.to_str());
    });
    provider
}

fn load_styles(sheets: &[(PathBuf, gtk4::CssProvider)]) {
    for (path, provider) in sheets {
        match std::fs::read_to_string(path) {
            Ok(css) => provider.load_from_string(&css),
            Err(_) if !path.exists() => provider.load_from_string(""),
            Err(error) => eprintln!("{}: {error}", path.display()),
        }
    }
}

/// Watches each file's **directory**, not the file, and treats a rename onto the path as a change.
///
/// An editor that saves by writing a temporary file and renaming it over the original destroys the
/// inode a file monitor holds, and GIO then reports the write on a two-second timer. Watching the
/// directory sees the rename immediately — but reports it as `RENAMED`, whose `file` argument is
/// the *temporary* path and whose `other_file` is the one that was asked for, so matching only the
/// first argument silently ignores every such save.
fn watch(
    blueprint: &Path,
    slot: &gtk4::Box,
    sheets: Vec<(PathBuf, gtk4::CssProvider)>,
    fixture: Option<String>,
) -> Vec<gio::FileMonitor> {
    let sheets = Rc::new(sheets);
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let mut monitors = Vec::new();

    let watched: Vec<PathBuf> = [blueprint.to_path_buf()]
        .into_iter()
        .chain(sheets.iter().map(|(path, _)| path.clone()))
        .collect();

    for target in watched {
        let Some(directory) = target.parent() else {
            continue;
        };
        let Ok(monitor) = gio::File::for_path(directory)
            .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
        else {
            continue;
        };

        let slot = slot.clone();
        let blueprint = blueprint.to_path_buf();
        let sheets = sheets.clone();
        let fixture = fixture.clone();
        let pending = pending.clone();
        monitor.connect_changed(move |_, file, renamed_to, _| {
            let touched = [Some(file.clone()), renamed_to.cloned()]
                .into_iter()
                .flatten()
                .any(|file| file.path().is_some_and(|path| path == target));
            if !touched {
                return;
            }
            if let Some(source) = pending.borrow_mut().take() {
                source.remove();
            }
            let (slot, blueprint, sheets) = (slot.clone(), blueprint.clone(), sheets.clone());
            let fixture = fixture.clone();
            let fired = pending.clone();
            let source = glib::timeout_add_local_once(SETTLE, move || {
                fired.borrow_mut().take();
                load_styles(sheets.as_ref());
                build(&slot, &blueprint, fixture.as_deref());
            });
            pending.replace(Some(source));
        });
        monitors.push(monitor);
    }
    monitors
}

/// Some widgets cannot be filled from a `.blp` because their data is not a property — a calendar's
/// events are a colour list per day. A named fixture puts sample data in after the build, so a
/// states sheet stays a blueprint rather than becoming a second program.
mod fixtures {
    use gtk4::gdk;
    use gtk4::prelude::*;

    use glimpse_widgets::{
        Calendar, Day, Event, EventList, Fact, FactList, ForecastList, ForecastStrip, Hour,
        Placeholder, Row, Section, WorldClock, Ymd, Zone,
    };
    use std::rc::Rc;

    const NAV: &str = "nav__";

    pub fn apply(name: &str, root: &gtk4::Widget) {
        match name {
            "calendar" => {
                if let Some(calendar) = find::<Calendar>(root) {
                    calendar_events(&calendar);
                }
            }
            "agenda" => {
                if let Some(calendar) = find::<Calendar>(root) {
                    calendar_events(&calendar);
                }
                if let Some(events) = find::<EventList>(root) {
                    agenda(&events, find::<gtk4::Revealer>(root));
                }
                if let Some(clocks) = find::<WorldClock>(root) {
                    world_clock(&clocks);
                }
            }
            "network" | "bluetooth" => drawer_nav(root),
            "weather" => {
                weather(root);
                drawer_nav(root);
            }
            _ => {}
        }
    }

    struct Forecast {
        label: &'static str,
        date: &'static str,
        icon_name: &'static str,
        condition: &'static str,
        precipitation: Option<u32>,
        low: f64,
        high: f64,
    }

    const fn forecast(
        label: &'static str,
        date: &'static str,
        icon_name: &'static str,
        condition: &'static str,
        precipitation: Option<u32>,
        low: f64,
        high: f64,
    ) -> Forecast {
        Forecast {
            label,
            date,
            icon_name,
            condition,
            precipitation,
            low,
            high,
        }
    }

    const DAYS: [Forecast; 10] = [
        forecast(
            "Today",
            "Fri 1 Sep",
            "weather-showers-symbolic",
            "Light rain",
            Some(60),
            12.0,
            18.0,
        ),
        forecast(
            "Tomorrow",
            "Sat 2 Sep",
            "weather-overcast-symbolic",
            "Overcast",
            Some(20),
            11.0,
            20.0,
        ),
        forecast(
            "Sunday",
            "Sun 3 Sep",
            "weather-clear-symbolic",
            "Clear",
            None,
            10.0,
            23.0,
        ),
        forecast(
            "Monday",
            "Mon 4 Sep",
            "weather-clear-symbolic",
            "Clear",
            None,
            12.0,
            25.0,
        ),
        forecast(
            "Tuesday",
            "Tue 5 Sep",
            "weather-few-clouds-symbolic",
            "Sunny spells",
            Some(10),
            14.0,
            26.0,
        ),
        forecast(
            "Wednesday",
            "Wed 6 Sep",
            "weather-showers-symbolic",
            "Showers",
            Some(70),
            13.0,
            21.0,
        ),
        forecast(
            "Thursday",
            "Thu 7 Sep",
            "weather-storm-symbolic",
            "Thunderstorms",
            Some(80),
            12.0,
            19.0,
        ),
        forecast(
            "Friday",
            "Fri 8 Sep",
            "weather-overcast-symbolic",
            "Overcast",
            Some(30),
            10.0,
            17.0,
        ),
        forecast(
            "Saturday",
            "Sat 9 Sep",
            "weather-clear-symbolic",
            "Clear",
            None,
            8.0,
            16.0,
        ),
        forecast(
            "Sunday",
            "Sun 10 Sep",
            "weather-fog-symbolic",
            "Fog",
            Some(20),
            7.0,
            15.0,
        ),
    ];

    const DETAILS: [(&str, &str); 12] = [
        ("Feels like", "16°"),
        ("Humidity", "78%"),
        ("Wind", "14 km/h NW"),
        ("Gusts", "28 km/h"),
        ("UV index", "2 · Low"),
        ("Air quality", "32 · Good"),
        ("Pressure", "1008 hPa"),
        ("Visibility", "9 km"),
        ("Dew point", "14°"),
        ("Sunrise", "06:21"),
        ("Sunset", "20:14"),
        ("Day length", "13 h 53 m"),
    ];

    fn weather(root: &gtk4::Widget) {
        if let Some(strip) = find::<ForecastStrip>(root) {
            let hour = |label: &str, icon_name: &str, temperature, now| Hour {
                label: label.to_owned(),
                icon_name: icon_name.to_owned(),
                temperature,
                now,
            };
            strip.set_hours(&[
                hour("Now", "weather-showers-symbolic", 18.0, true),
                hour("16:00", "weather-showers-symbolic", 17.0, false),
                hour("17:00", "weather-few-clouds-symbolic", 17.0, false),
                hour("18:00", "weather-clear-symbolic", 16.0, false),
            ]);
        }

        let Some(list) = find::<ForecastList>(root) else {
            return;
        };
        list.set_days(
            &DAYS
                .iter()
                .map(|day| Day {
                    label: day.label.to_owned(),
                    icon_name: day.icon_name.to_owned(),
                    precipitation: day.precipitation,
                    low: day.low,
                    high: day.high,
                })
                .collect::<Vec<_>>(),
        );

        let Some(drawer) = find::<gtk4::Revealer>(root) else {
            return;
        };
        let Some(stack) = drawer.child().and_then(|child| find::<gtk4::Stack>(&child)) else {
            return;
        };

        stack.add_named(
            &page("Right now", Some("Light rain"), None, &DETAILS),
            Some("details"),
        );
        stack.add_named(
            &page(
                "Thunderstorm warning",
                Some("yellow"),
                Some(alert_placeholder()),
                &[
                    ("Issued", "14:05"),
                    ("Expires", "21:00"),
                    ("Source", "LHMT"),
                ],
            ),
            Some("alert"),
        );
        for (index, day) in DAYS.iter().enumerate() {
            let facts = [
                ("Condition", day.condition.to_owned()),
                ("High", format!("{}°", day.high)),
                ("Low", format!("{}°", day.low)),
                (
                    "Chance of rain",
                    format!("{}%", day.precipitation.unwrap_or_default()),
                ),
                ("Wind", "14 km/h NW".to_owned()),
                ("Humidity", "78%".to_owned()),
                ("Sunrise", "06:21".to_owned()),
                ("Sunset", "20:14".to_owned()),
            ];
            let facts: Vec<(&str, &str)> = facts
                .iter()
                .map(|(label, value)| (*label, value.as_str()))
                .collect();
            stack.add_named(
                &page(day.label, Some(day.date), None, &facts),
                Some(&format!("day{index}")),
            );
        }
        stack.set_visible_child_name("details");

        list.connect_activated(glib::clone!(
            #[weak]
            drawer,
            #[weak]
            stack,
            move |_, index| {
                stack.set_visible_child_name(&format!("day{index}"));
                drawer.set_reveal_child(true);
            }
        ));
    }

    fn alert_placeholder() -> gtk4::Widget {
        let placeholder = Placeholder::new();
        placeholder.set_icon_name(Some("weather-storm-symbolic"));
        placeholder.set_title(Some("Thunderstorms until 21:00"));
        placeholder.set_description(Some(
            "Frequent lightning and gusts to 80 km/h are expected. Avoid open water.",
        ));
        placeholder.set_error(true);
        placeholder.upcast()
    }

    fn page(
        title: &str,
        count: Option<&str>,
        lead: Option<gtk4::Widget>,
        facts: &[(&str, &str)],
    ) -> gtk4::Widget {
        let body = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .valign(gtk4::Align::Start)
            .build();
        if let Some(lead) = lead {
            body.append(&lead);
        }
        let list = FactList::new();
        list.set_facts(
            &facts
                .iter()
                .map(|(label, value)| Fact::new(*label, *value))
                .collect::<Vec<_>>(),
        );
        body.append(&list);

        let section = Section::new();
        section.set_title(Some(title));
        section.set_count(count);
        section.set_content(Some(&body));
        section.upcast()
    }

    fn drawer_nav(root: &gtk4::Widget) {
        let Some(drawer) = find::<gtk4::Revealer>(root) else {
            return;
        };
        let Some(stack) = drawer.child().and_then(|child| find::<gtk4::Stack>(&child)) else {
            return;
        };

        let rows: Rc<Vec<(gtk4::Button, String)>> = Rc::new(
            collect::<gtk4::Button>(root)
                .into_iter()
                .filter_map(|row| {
                    let page = row
                        .css_classes()
                        .iter()
                        .find_map(|class| class.as_str().strip_prefix(NAV).map(str::to_owned))?;
                    stack.child_by_name(&page).map(|_| (row, page))
                })
                .collect(),
        );

        for (index, (row, page)) in rows.iter().enumerate() {
            let drawer = drawer.clone();
            let stack = stack.clone();
            let all = Rc::clone(&rows);
            let page = page.clone();
            row.connect_clicked(move |_| {
                let showing = drawer.reveals_child()
                    && stack.visible_child_name().as_deref() == Some(page.as_str());
                for (other, _) in all.iter() {
                    if let Some(row) = other.downcast_ref::<Row>() {
                        row.set_selected(false);
                    }
                }
                if showing {
                    drawer.set_reveal_child(false);
                    return;
                }
                stack.set_visible_child_name(&page);
                if let Some(row) = all[index].0.downcast_ref::<Row>() {
                    row.set_selected(true);
                }
                drawer.set_reveal_child(true);
            });
        }
    }

    fn collect<T: IsA<gtk4::Widget>>(widget: &gtk4::Widget) -> Vec<T> {
        let mut found = Vec::new();
        if let Ok(this) = widget.clone().downcast::<T>() {
            found.push(this);
        }
        let mut child = widget.first_child();
        while let Some(node) = child {
            found.extend(collect::<T>(&node));
            child = node.next_sibling();
        }
        found
    }

    fn agenda(events: &EventList, drawer: Option<gtk4::Revealer>) {
        let color = |hex: &str| hex.parse::<gdk::RGBA>().unwrap_or(gdk::RGBA::BLUE);
        let work = color("#3584e4");
        let home = color("#2ec27e");
        let birthday = color("#e01b24");

        let event = |summary: &str, detail: &str, when: &str, color| Event {
            summary: summary.to_owned(),
            detail: detail.to_owned(),
            when: when.to_owned(),
            color: Some(color),
        };

        let today = [
            event("Company all-hands", "All day", "—", work),
            event("Team standup", "Daily · Google Meet", "09:30", work),
            event(
                "Vilnius ↔ Berlin design review with the platform group",
                "Meeting room Kaunas",
                "14:00",
                work,
            ),
            event("Marta's birthday", "All day", "—", birthday),
            event("Pick up the parcel", "Antakalnio g. 18", "18:00", home),
        ];

        events.set_max_rows(4);
        events.set_events(&today);

        if let Some(drawer) = drawer.as_ref()
            && let Some(child) = drawer.child()
            && let Some(all) = find::<EventList>(&child)
        {
            all.set_events(&today);
        }
        events.connect_overflow(move |_| {
            if let Some(drawer) = drawer.as_ref() {
                drawer.set_reveal_child(!drawer.reveals_child());
            }
        });
    }

    fn world_clock(clocks: &WorldClock) {
        let zone = |label: &str, timezone: &str, note: &str, icon_name: &str| Zone {
            label: label.to_owned(),
            timezone: timezone.to_owned(),
            note: note.to_owned(),
            icon_name: icon_name.to_owned(),
        };
        clocks.set_zones(&[
            zone(
                "Vilnius",
                "Europe/Vilnius",
                "18° · Light rain",
                "weather-showers-scattered-symbolic",
            ),
            zone("Berlin", "Europe/Berlin", "", ""),
            zone("San Francisco", "America/Los_Angeles", "", ""),
            zone(
                "Auckland",
                "Pacific/Auckland",
                "9° · Clear",
                "weather-clear-night-symbolic",
            ),
        ]);
    }

    fn calendar_events(calendar: &Calendar) {
        let today = calendar.today();
        let color = |hex: &str| hex.parse::<gdk::RGBA>().unwrap_or(gdk::RGBA::BLUE);
        let work = color("#3584e4");
        let home = color("#2ec27e");
        let birthday = color("#e01b24");

        let day = |day: u32| Ymd::new(today.year, today.month, day);
        calendar.set_events(&[
            (day(4), vec![work]),
            (day(9), vec![work, home]),
            (day(11), vec![work, home, birthday]),
            (day(17), vec![home, birthday, work, home]),
            (today, vec![work, birthday]),
        ]);
        calendar.select(day(17));
    }

    fn find<T: IsA<gtk4::Widget>>(widget: &gtk4::Widget) -> Option<T> {
        if let Ok(found) = widget.clone().downcast::<T>() {
            return Some(found);
        }
        let mut child = widget.first_child();
        while let Some(node) = child {
            if let Some(found) = find::<T>(&node) {
                return Some(found);
            }
            child = node.next_sibling();
        }
        None
    }
}

fn ensure_types() {
    use glimpse_widgets::{
        Calendar, ClockRow, EventList, EventRow, FactList, ForecastDay, ForecastHour, ForecastList,
        ForecastStrip, Hero, Indicator, IndicatorGroup, Notice, Panel, Placeholder, PopoverShell,
        RangeBar, Readout, Row, Section, WorldClock,
    };

    for widget in [
        Calendar::static_type(),
        ClockRow::static_type(),
        EventRow::static_type(),
        FactList::static_type(),
        ForecastDay::static_type(),
        ForecastHour::static_type(),
        ForecastList::static_type(),
        ForecastStrip::static_type(),
        Notice::static_type(),
        RangeBar::static_type(),
        Readout::static_type(),
        EventList::static_type(),
        Section::static_type(),
        WorldClock::static_type(),
        Hero::static_type(),
        PopoverShell::static_type(),
        Panel::static_type(),
        Indicator::static_type(),
        IndicatorGroup::static_type(),
        Placeholder::static_type(),
        Row::static_type(),
    ] {
        let _ = widget;
    }
}

fn build(slot: &gtk4::Box, blueprint: &Path, fixture: Option<&str>) {
    while let Some(child) = slot.first_child() {
        slot.remove(&child);
    }

    let ui = match compile(blueprint) {
        Ok(ui) => ui,
        Err(message) => {
            slot.append(&error_label(&message));
            return;
        }
    };

    let builder = gtk4::Builder::new();
    if let Err(error) = builder.add_from_file(&ui) {
        slot.append(&error_label(error.message()));
        return;
    }

    let widget = builder
        .objects()
        .into_iter()
        .filter_map(|object| object.downcast::<gtk4::Widget>().ok())
        .find(|widget| widget.parent().is_none());

    match widget {
        Some(widget) => {
            if let Some(fixture) = fixture {
                fixtures::apply(fixture, &widget);
            }
            widget.set_halign(gtk4::Align::Center);
            widget.set_valign(gtk4::Align::Center);
            slot.append(&widget);
        }
        None => slot.append(&error_label(
            "nothing in this file builds a widget; an example is a top-level object, not a template",
        )),
    }
}

fn compile(blueprint: &Path) -> Result<PathBuf, String> {
    let ui = std::env::temp_dir().join("glimpse-preview.ui");
    let output = std::process::Command::new("blueprint-compiler")
        .args(["compile", "--output"])
        .args([ui.as_os_str(), blueprint.as_os_str()])
        .output()
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        return Ok(ui);
    }
    let message = if output.stderr.is_empty() {
        output.stdout
    } else {
        output.stderr
    };
    Err(String::from_utf8_lossy(&message).trim().to_string())
}

fn error_label(message: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(message));
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.add_css_class("error");
    label
}

fn builtin_css() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("styles/glimpse.css")
}

fn theme_css() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/themes/adwaita/panel.css")
}
