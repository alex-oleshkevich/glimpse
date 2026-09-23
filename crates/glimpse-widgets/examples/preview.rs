use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::AdwDialogExt;
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
const DARK_SHEET: &str = "dark.css";

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
    glimpse_utils::init_translations(None);
    glimpse_widgets::register_resources().expect("widget resources");
    ensure_types();

    let shared = blueprint.with_file_name("_shared.css");
    let sheets = vec![
        (resolve(&builtin_css()), provider()),
        (resolve(&theme_css()), provider()),
        (resolve(&theme_dark_css()), provider()),
        (resolve(&shared), provider()),
        (resolve(&blueprint.with_extension("css")), provider()),
    ];
    let checkerboard = provider();
    checkerboard.load_from_string(CHECKERBOARD);

    if let Some(display) = gdk::Display::default() {
        let priorities = [
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            gtk4::STYLE_PROVIDER_PRIORITY_USER,
            gtk4::STYLE_PROVIDER_PRIORITY_USER + 1,
            gtk4::STYLE_PROVIDER_PRIORITY_USER + 2,
            gtk4::STYLE_PROVIDER_PRIORITY_USER + 3,
        ];
        for ((_, provider), priority) in sheets.iter().zip(priorities) {
            gtk4::style_context_add_provider_for_display(&display, provider, priority);
        }
        gtk4::style_context_add_provider_for_display(
            &display,
            &checkerboard,
            gtk4::STYLE_PROVIDER_PRIORITY_USER + 4,
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

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(blueprint.file_name().unwrap_or_default().to_string_lossy())
        .content(&slot)
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
    build(&slot, blueprint, fixture, &sheets);

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
    provider.set_prefers_color_scheme(if adw::StyleManager::default().is_dark() {
        gtk4::InterfaceColorScheme::Dark
    } else {
        gtk4::InterfaceColorScheme::Light
    });
    provider.connect_parsing_error(|_, section, error| {
        eprintln!("stylesheet {}: {error}", section.to_str());
    });
    provider
}

fn load_styles(sheets: &[(PathBuf, gtk4::CssProvider)]) {
    let dark = adw::StyleManager::default().is_dark();
    for (path, provider) in sheets {
        if !dark && path.file_name().is_some_and(|name| name == DARK_SHEET) {
            provider.load_from_string("");
            continue;
        }
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
                build(&slot, &blueprint, fixture.as_deref(), sheets.as_ref());
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
    use adw::prelude::AdwDialogExt;
    use gtk4::gdk;
    use gtk4::prelude::*;

    use glimpse_widgets::{
        Action, Advisory, BatteryChargeLimit, BatteryDevice, BatteryPopover, Body,
        BrightnessPopover, Calendar, Choice, ChoiceList, Day, Display, DisplayList, DisplayLogical,
        DisplayMode, DisplayPopover, Event, EventList, Fact, FactList, Focus, Group, Hero, Hour,
        Indicator, IndicatorSpec, InhibitorEntry, InhibitorList, InhibitorSource, InhibitorTargets,
        NightLight, Notification, NotificationsPopover, NowPlaying, Pager, Player, PlayerList,
        PrintingDetail, PrintingJob, PrintingPopover, PrintingPrinter, PrivacyPopover,
        PrivacyUsage, Repeat, Row, Severity, Shape, Slot, SourceList, SplitRow, TransportAction,
        TrayChip, TrayStrip, Urgency, WeatherPage, WeatherPopover, WorldClock, Ymd, Zone,
    };
    use gtk4::glib;
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::time::Duration;

    const NAV: &str = "nav__";
    const EXPAND: &str = "expander";
    const DIM_HOST: &str = "dim-host";
    const DIMMED: &str = "dimmed";
    const OPENED: &str = "open";
    const ACTION: &str = "action__";
    const DEMO: &str = "demo__";
    const OPEN: &str = "open-on-map";
    const DIALOG: &str = "dialog__";
    const AFTER: &str = "after__";
    const BUSY: &str = "busy";
    const ICON: &str = "icon__";
    const OVERLAY: &str = "overlay__";
    const SEVERITY: &str = "severity__";
    const ATTENTION: &str = "state__attention";
    const NOTICE: &str = "state__notice";

    pub fn apply(
        name: &str,
        root: &gtk4::Widget,
        sheets: &[(PathBuf, gtk4::CssProvider)],
        builder: &gtk4::Builder,
    ) {
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
            "mpris" => mpris(root),
            "source_list_states" => source_list_states(root),
            "display_list_states" => display_list_states(root),
            "battery" => battery_popover(root),
            "brightness_popover_full" => brightness_popover_states(root),
            "display_popover_full" => display_popover_states(root),
            "next_event" => next_event(root),
            "weather_popover" => weather_popover(root),
            "notifications" => notifications(root, notification_catalog()),
            "tray" => tray(root),
            "tray_states" => tray_states(root),
            "inhibitor_list_states" => inhibitor_list_states(root),
            "printing_states" => printing_popover_states(root),
            "privacy_states" => privacy_popover_states(root),
            _ => {}
        }
        drawer_nav(root);
        expanders(root);
        actions(root);
        opened_menus(root);
        dialogs(root, builder);
        after(root);
        pager(root);
        busy(root);
        indicators(root);
        scheme_toggle(root, sheets);
    }

    fn dialogs(root: &gtk4::Widget, builder: &gtk4::Builder) {
        for widget in collect::<gtk4::Widget>(root) {
            let Some(id) = widget
                .css_classes()
                .iter()
                .find_map(|class| class.as_str().strip_prefix(DIALOG).map(str::to_owned))
            else {
                continue;
            };
            let Some(dialog) = builder.object::<adw::Dialog>(&id) else {
                eprintln!("{DIALOG}{id} names no AdwDialog in this file; that control is dead");
                continue;
            };
            let Some(button) = widget.downcast_ref::<gtk4::Button>() else {
                eprintln!(
                    "{DIALOG}{id} is on a {}, which nothing can click",
                    widget.type_()
                );
                continue;
            };
            if button.has_css_class(OPEN) {
                let shown = dialog.clone();
                button.connect_map(move |button| shown.present(Some(button)));
            }
            button.connect_clicked(move |button| dialog.present(Some(button)));
        }
    }

    fn after(root: &gtk4::Widget) {
        let Some((_, stack)) = page_stack(root) else {
            if collect::<gtk4::Widget>(root)
                .iter()
                .any(|widget| widget.css_classes().iter().any(|c| c.starts_with(AFTER)))
            {
                eprintln!("no Gtk.Revealer holds a Gtk.Stack; every {AFTER} page is dead");
            }
            return;
        };

        let mut rules: Vec<(String, u64, String)> = Vec::new();
        let mut child = stack.first_child();
        while let Some(page) = child {
            child = page.next_sibling();
            let Some(rule) = page
                .css_classes()
                .iter()
                .find_map(|class| class.as_str().strip_prefix(AFTER).map(str::to_owned))
            else {
                continue;
            };
            let Some((delay, target)) = rule.split_once("__") else {
                eprintln!("{AFTER}{rule} is not <ms>__<page>; that page never advances");
                continue;
            };
            let Ok(delay) = delay.parse::<u64>() else {
                eprintln!("{AFTER}{rule} does not begin with a number of milliseconds");
                continue;
            };
            if stack.child_by_name(target).is_none() {
                eprintln!("{AFTER}{rule} names no page in the stack; that page never advances");
                continue;
            }
            let Some(name) = stack.page(&page).name() else {
                eprintln!("{AFTER}{rule} is on a stack page with no name; it can never be shown");
                continue;
            };
            rules.push((name.into(), delay, target.to_owned()));
        }

        if rules.is_empty() {
            return;
        }

        let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let rules = Rc::new(rules);
        let arm = {
            let pending = pending.clone();
            let rules = rules.clone();
            move |stack: &gtk4::Stack| {
                if let Some(armed) = pending.borrow_mut().take() {
                    armed.remove();
                }
                if !stack.is_mapped() {
                    return;
                }
                let Some(showing) = stack.visible_child_name() else {
                    return;
                };
                let Some((_, delay, target)) =
                    rules.iter().find(|(name, _, _)| name == showing.as_str())
                else {
                    return;
                };
                let stack = stack.clone();
                let target = target.clone();
                let fired = pending.clone();
                let armed =
                    glib::timeout_add_local_once(Duration::from_millis(*delay), move || {
                        fired.borrow_mut().take();
                        stack.set_visible_child_name(&target);
                    });
                *pending.borrow_mut() = Some(armed);
            }
        };

        let on_map = arm.clone();
        stack.connect_map(move |stack| on_map(stack));
        stack.connect_unmap(move |_| {
            if let Some(armed) = pending.borrow_mut().take() {
                armed.remove();
            }
        });
        stack.connect_visible_child_name_notify(move |stack| arm(stack));
    }

    fn scheme_toggle(root: &gtk4::Widget, sheets: &[(PathBuf, gtk4::CssProvider)]) {
        for button in tagged::<gtk4::Button>(root, "scheme") {
            let manager = adw::StyleManager::default();
            button.set_label(if manager.is_dark() {
                "Light theme"
            } else {
                "Dark theme"
            });
            let sheets = sheets.to_vec();
            button.connect_clicked(move |button| {
                let manager = adw::StyleManager::default();
                let dark = !manager.is_dark();
                manager.set_color_scheme(if dark {
                    adw::ColorScheme::ForceDark
                } else {
                    adw::ColorScheme::ForceLight
                });
                let scheme = if dark {
                    gtk4::InterfaceColorScheme::Dark
                } else {
                    gtk4::InterfaceColorScheme::Light
                };
                for (_, provider) in &sheets {
                    provider.set_prefers_color_scheme(scheme);
                }
                super::load_styles(&sheets);
                button.set_label(if dark { "Light theme" } else { "Dark theme" });
            });
        }
    }

    struct Forecast {
        label: &'static str,
        icon_name: &'static str,
        condition: &'static str,
        precipitation: Option<u32>,
        low: f64,
        high: f64,
    }

    const fn forecast(
        label: &'static str,
        icon_name: &'static str,
        condition: &'static str,
        precipitation: Option<u32>,
        low: f64,
        high: f64,
    ) -> Forecast {
        Forecast {
            label,
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
            "weather-showers-symbolic",
            "Light rain",
            Some(60),
            12.0,
            18.0,
        ),
        forecast(
            "Tomorrow",
            "weather-overcast-symbolic",
            "Overcast",
            Some(20),
            11.0,
            20.0,
        ),
        forecast(
            "Sunday",
            "weather-clear-symbolic",
            "Clear",
            None,
            10.0,
            23.0,
        ),
        forecast(
            "Monday",
            "weather-clear-symbolic",
            "Clear",
            None,
            12.0,
            25.0,
        ),
        forecast(
            "Tuesday",
            "weather-few-clouds-symbolic",
            "Sunny spells",
            Some(10),
            14.0,
            26.0,
        ),
        forecast(
            "Wednesday",
            "weather-showers-symbolic",
            "Showers",
            Some(70),
            13.0,
            21.0,
        ),
        forecast(
            "Thursday",
            "weather-storm-symbolic",
            "Thunderstorms",
            Some(80),
            12.0,
            19.0,
        ),
        forecast(
            "Friday",
            "weather-overcast-symbolic",
            "Overcast",
            Some(30),
            10.0,
            17.0,
        ),
        forecast(
            "Saturday",
            "weather-clear-symbolic",
            "Clear",
            None,
            8.0,
            16.0,
        ),
        forecast("Sunday", "weather-fog-symbolic", "Fog", Some(20), 7.0, 15.0),
    ];

    /// The real `$WeatherPopover`, filled through the setters the applet uses, so the preview
    /// shows the shipped widget rather than a hand-copied arrangement of its parts.
    fn weather_popover(root: &gtk4::Widget) {
        let Some(popover) = find::<WeatherPopover>(root) else {
            return;
        };

        popover.set_heading(
            "weather-showers-symbolic",
            "Vilnius",
            Some("Light rain · feels like 16°"),
        );
        popover.set_reading(Some(("18", "°")));

        let hour = |label: &str, icon_name: &str, temperature| Hour {
            label: label.to_owned(),
            icon_name: icon_name.to_owned(),
            temperature,
            now: false,
        };
        popover.set_hours(&[
            hour("16:00", "weather-showers-symbolic", 17.0),
            hour("17:00", "weather-showers-scattered-symbolic", 17.0),
            hour("18:00", "weather-few-clouds-symbolic", 16.0),
            hour("19:00", "weather-clear-symbolic", 15.0),
        ]);

        popover.set_days(
            &DAYS
                .iter()
                .skip(1)
                .map(|day| Day {
                    label: day.label.to_owned(),
                    icon_name: day.icon_name.to_owned(),
                    precipitation: day.precipitation,
                    low: day.low,
                    high: day.high,
                })
                .collect::<Vec<_>>(),
        );

        popover.set_nowcast(Some(&Advisory {
            severity: Severity::Info,
            icon_name: "weather-showers-symbolic".to_owned(),
            title: "Rain starting in 25 minutes".to_owned(),
            subtitle: Some("Light rain".to_owned()),
            page: None,
        }));

        popover.set_alerts(&[
            Advisory {
                severity: Severity::Error,
                icon_name: "dialog-warning-symbolic".to_owned(),
                title: "Thunderstorm warning until 21:00".to_owned(),
                subtitle: Some("LHMT".to_owned()),
                page: Some(glimpse_widgets::alert_page(0)),
            },
            Advisory {
                severity: Severity::Warning,
                icon_name: "dialog-warning-symbolic".to_owned(),
                title: "Strong wind advisory".to_owned(),
                subtitle: Some("LHMT".to_owned()),
                page: None,
            },
        ]);

        let mut pages: Vec<WeatherPage> = DAYS
            .iter()
            .skip(1)
            .enumerate()
            .map(|(index, day)| WeatherPage {
                key: glimpse_widgets::day_page(index as u32),
                title: day.label.to_owned(),
                description: Some(day.condition.to_owned()),
                facts: vec![
                    Fact::new("High", format!("{}°", day.high)),
                    Fact::new("Low", format!("{}°", day.low)),
                    Fact::new("Sunrise", "05:59"),
                    Fact::new("Sunset", "20:08"),
                    Fact::new("Day length", "14 h 9 min"),
                ],
            })
            .collect();

        pages.push(WeatherPage {
            key: glimpse_widgets::alert_page(0),
            title: "Thunderstorm warning until 21:00".to_owned(),
            description: Some("Hail and gusts to 25 m/s are possible.".to_owned()),
            facts: vec![
                Fact::new("Issued", "14:05"),
                Fact::new("Expires", "21:00"),
                Fact::new("Source", "LHMT"),
            ],
        });

        popover.set_pages(&pages);
        popover.set_footer(Some("Weather settings"));
    }

    struct Song {
        title: &'static str,
        artist: &'static str,
        album: &'static str,
        duration: f64,
    }

    const fn song(
        title: &'static str,
        artist: &'static str,
        album: &'static str,
        duration: f64,
    ) -> Song {
        Song {
            title,
            artist,
            album,
            duration,
        }
    }

    struct Source {
        name: &'static str,
        icon_name: &'static str,
        seekable: bool,
        art: (u8, u8, u8),
        songs: &'static [Song],
    }

    const SOURCES: [Source; 4] = [
        Source {
            name: "Spotify",
            icon_name: "audio-x-generic-symbolic",
            seekable: true,
            art: (196, 108, 62),
            songs: &[
                song(
                    "Dayvan Cowboy",
                    "Boards of Canada",
                    "The Campfire Headphase",
                    285.0,
                ),
                song(
                    "Roygbiv",
                    "Boards of Canada",
                    "Music Has the Right to Children",
                    149.0,
                ),
                song(
                    "Everything You Do Is a Balloon",
                    "Boards of Canada",
                    "Hi Scores",
                    397.0,
                ),
            ],
        },
        Source {
            name: "Firefox",
            icon_name: "web-browser-symbolic",
            seekable: true,
            art: (74, 108, 168),
            songs: &[
                song(
                    "How the Chip Shortage Ends",
                    "Odd Lots",
                    "Episode 412",
                    2731.0,
                ),
                song("The Housing Trap", "Odd Lots", "Episode 411", 2504.0),
            ],
        },
        Source {
            name: "VLC",
            icon_name: "video-x-generic-symbolic",
            seekable: true,
            art: (92, 92, 104),
            songs: &[song("The Wire — S03E04", "", "Hamsterdam", 3320.0)],
        },
        Source {
            name: "Amberol",
            icon_name: "audio-headphones-symbolic",
            seekable: false,
            art: (64, 128, 118),
            songs: &[song(
                "Sleep Walk",
                "Santo & Johnny",
                "Santo & Johnny",
                142.0,
            )],
        },
    ];

    /// A rewind past this many seconds restarts the track instead of stepping back a track, which
    /// is what every player does and what makes the button feel right rather than jumpy.
    const RESTART_AFTER: f64 = 3.0;

    struct Entry {
        source: usize,
        track: usize,
        position: f64,
        playing: bool,
    }

    impl Entry {
        fn source(&self) -> &'static Source {
            &SOURCES[self.source]
        }

        fn song(&self) -> &'static Song {
            &self.source().songs[self.track]
        }

        fn step(&mut self, forward: bool) {
            let count = self.source().songs.len();
            match forward {
                true => self.track = (self.track + 1) % count,
                false if self.position > RESTART_AFTER => {}
                false => self.track = (self.track + count - 1) % count,
            }
            self.position = 0.0;
        }
    }

    struct Media {
        entries: Vec<Entry>,
        covers: Vec<gdk::Texture>,
        shuffle: bool,
        repeat: Repeat,
    }

    impl Media {
        fn new() -> Self {
            let entry = |source, position, playing| Entry {
                source,
                track: 0,
                position,
                playing,
            };
            Self {
                entries: vec![
                    entry(0, 167.0, true),
                    entry(1, 0.0, false),
                    entry(2, 0.0, true),
                    entry(3, 0.0, false),
                ],
                covers: SOURCES.iter().map(|source| artwork(source.art)).collect(),
                shuffle: false,
                repeat: Repeat::Off,
            }
        }
    }

    fn cycle(repeat: Repeat) -> Repeat {
        match repeat {
            Repeat::Off => Repeat::Playlist,
            Repeat::Playlist => Repeat::Track,
            Repeat::Track => Repeat::Off,
        }
    }

    const OUTPUTS: [(&str, &str, &str); 3] = [
        ("WH-1000XM5", "", "audio-headphones-symbolic"),
        ("Built-in speakers", "", "audio-speakers-symbolic"),
        ("Living room", "", "video-display-symbolic"),
    ];

    fn mpris(root: &gtk4::Widget) {
        let (Some(player), Some(list)) = (find::<NowPlaying>(root), find::<PlayerList>(root))
        else {
            return;
        };

        if let Some(outputs) = find::<ChoiceList>(root) {
            outputs.set_choices(
                &OUTPUTS
                    .iter()
                    .map(|(label, detail, icon_name)| Choice {
                        label: (*label).to_owned(),
                        detail: (*detail).to_owned(),
                        icon_name: (*icon_name).to_owned(),
                    })
                    .collect::<Vec<_>>(),
            );
            outputs.set_selected(Some(0));
        }

        let media = Rc::new(RefCell::new(Media::new()));
        show(&media.borrow(), &player, &list);

        player.transport().connect_action(glib::clone!(
            #[strong]
            media,
            #[weak]
            player,
            #[weak]
            list,
            move |_, action| {
                {
                    let mut media = media.borrow_mut();
                    match action {
                        TransportAction::PlayPause => {
                            media.entries[0].playing = !media.entries[0].playing;
                        }
                        TransportAction::Next => media.entries[0].step(true),
                        TransportAction::Previous => media.entries[0].step(false),
                        TransportAction::Shuffle => media.shuffle = !media.shuffle,
                        TransportAction::Repeat => media.repeat = cycle(media.repeat),
                    }
                }
                show(&media.borrow(), &player, &list);
            }
        ));

        player.scrubber().connect_seek(glib::clone!(
            #[strong]
            media,
            move |_, seconds| media.borrow_mut().entries[0].position = seconds
        ));

        list.connect_activated(glib::clone!(
            #[strong]
            media,
            #[weak]
            player,
            #[weak]
            list,
            move |_, key| {
                let Some(index) = other(&media.borrow(), &key) else {
                    return;
                };
                media.borrow_mut().entries.swap(0, index);
                show(&media.borrow(), &player, &list);
            }
        ));

        list.connect_toggled(glib::clone!(
            #[strong]
            media,
            #[weak]
            player,
            #[weak]
            list,
            move |_, key| {
                let Some(index) = other(&media.borrow(), &key) else {
                    return;
                };
                {
                    let mut media = media.borrow_mut();
                    let entry = &mut media.entries[index];
                    entry.playing = !entry.playing;
                }
                show(&media.borrow(), &player, &list);
            }
        ));

        glib::timeout_add_local(
            Duration::from_secs(1),
            glib::clone!(
                #[strong]
                media,
                #[weak]
                player,
                #[weak]
                list,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || {
                    {
                        let mut media = media.borrow_mut();
                        if !media.entries[0].playing {
                            return glib::ControlFlow::Continue;
                        }
                        let repeat = media.repeat;
                        let entry = &mut media.entries[0];
                        entry.position += 1.0;
                        if entry.position >= entry.song().duration {
                            match repeat {
                                Repeat::Track => entry.position = 0.0,
                                _ => entry.step(true),
                            }
                        }
                    }
                    show(&media.borrow(), &player, &list);
                    glib::ControlFlow::Continue
                }
            ),
        );
    }

    fn source_list_states(root: &gtk4::Widget) {
        let source =
            |key: &str, name: &str, value: f64, maximum: f64, floor: f64| glimpse_widgets::Source {
                key: key.to_owned(),
                name: name.to_owned(),
                value,
                maximum,
                floor,
            };

        for list in tagged::<SourceList>(root, "one") {
            list.set_sources(&[source("built-in", "Built-in", 65.0, 100.0, 0.0)]);
        }
        for list in tagged::<SourceList>(root, "three") {
            list.set_sources(&[
                source("built-in", "Built-in", 40.0, 100.0, 0.0),
                source("dp-1", "DP-1", 60.0, 100.0, 0.0),
                source("dp-2", "DP-2", 80.0, 100.0, 5.0),
            ]);
        }
    }

    fn display_list_states(root: &gtk4::Widget) {
        let mode = DisplayMode {
            width: 1920,
            height: 1080,
            refresh_mhz: 60_000,
        };
        let logical = |scale: f64| DisplayLogical { x: 0, y: 0, scale };

        let built_in = Display {
            connector: "eDP-1".to_owned(),
            label: "Built-in display".to_owned(),
            current_mode: Some(mode.clone()),
            logical: Some(logical(2.0)),
            enabled: true,
            ..Display::default()
        };
        let external = Display {
            connector: "DP-1".to_owned(),
            label: "DELL U2720Q".to_owned(),
            make: Some("Dell Inc.".to_owned()),
            model: Some("U2720Q".to_owned()),
            serial: Some("8QK1P93".to_owned()),
            current_mode: Some(mode.clone()),
            logical: Some(logical(1.25)),
            enabled: true,
        };

        for list in tagged::<DisplayList>(root, "one") {
            list.set_displays(std::slice::from_ref(&built_in));
        }
        for list in tagged::<DisplayList>(root, "two") {
            list.set_displays(&[built_in.clone(), external.clone()]);
        }
        for list in tagged::<DisplayList>(root, "disabled") {
            let mut disabled = external.clone();
            disabled.enabled = false;
            list.set_displays(&[built_in.clone(), disabled]);
        }
        for list in tagged::<DisplayList>(root, "no_power") {
            list.set_displays(&[built_in.clone(), external.clone()]);
            list.set_output_power(false);
        }
    }

    fn battery_popover(root: &gtk4::Widget) {
        let Some(popover) = find::<BatteryPopover>(root) else {
            return;
        };
        popover.set_heading(
            Some("battery-full-charged-symbolic"),
            Some("Fully charged"),
            Some(100),
        );
        popover.set_profiles(
            &[
                Choice {
                    label: "Power saver".to_owned(),
                    detail: "Longer battery life, slower response".to_owned(),
                    icon_name: "power-profile-power-saver-symbolic".to_owned(),
                },
                Choice {
                    label: "Balanced".to_owned(),
                    detail: "The default trade-off".to_owned(),
                    icon_name: "power-profile-balanced-symbolic".to_owned(),
                },
                Choice {
                    label: "Performance".to_owned(),
                    detail: String::new(),
                    icon_name: "power-profile-performance-symbolic".to_owned(),
                },
            ],
            Some(1),
        );
        popover.set_devices(&[BatteryDevice {
            name: "MX Master 3S".to_owned(),
            subtitle: "Mouse".to_owned(),
            icon_name: "input-mouse-symbolic".to_owned(),
            value: "41%".to_owned(),
        }]);
        popover.set_details(
            &[
                Fact::new("Charge", "100%"),
                Fact::new("Energy", "87.0 / 87.0 Wh"),
                Fact::new("Health", "97%"),
                Fact::new("Voltage", "17.2 V"),
                Fact::new("Technology", "Li-ion"),
                Fact::new("Model", "A32-K55"),
                Fact::new("Vendor", "ASUS"),
            ],
            Some(&BatteryChargeLimit {
                enabled: false,
                subtitle: "Stops at 80% to slow wear".to_owned(),
            }),
        );
    }

    fn brightness_popover_states(root: &gtk4::Widget) {
        let level = |key: &str, name: &str, value: f64| glimpse_widgets::Source {
            key: key.to_owned(),
            name: name.to_owned(),
            value,
            maximum: 100.0,
            floor: 0.0,
        };

        for popover in tagged::<BrightnessPopover>(root, "none") {
            popover.set_sources(&[]);
        }
        for popover in tagged::<BrightnessPopover>(root, "one") {
            popover.set_sources(&[level("built-in", "Built-in", 65.0)]);
        }
        for popover in tagged::<BrightnessPopover>(root, "three") {
            popover.set_sources(&[
                level("built-in", "Built-in", 40.0),
                level("dp-1", "DP-1", 60.0),
                level("dp-2", "DP-2", 80.0),
            ]);
        }
        for popover in tagged::<BrightnessPopover>(root, "unavailable") {
            popover.set_sources(&[level("built-in", "Built-in", 55.0)]);
            popover.set_night_light(None);
        }
        for popover in tagged::<BrightnessPopover>(root, "off") {
            popover.set_sources(&[level("built-in", "Built-in", 55.0)]);
            popover.set_night_light(Some(&NightLight {
                enabled: false,
                temperature: 6500,
            }));
        }
        for popover in tagged::<BrightnessPopover>(root, "on") {
            popover.set_sources(&[level("built-in", "Built-in", 55.0)]);
            popover.set_night_light(Some(&NightLight {
                enabled: true,
                temperature: 4200,
            }));
        }
    }

    fn display_popover_states(root: &gtk4::Widget) {
        let mode = DisplayMode {
            width: 1920,
            height: 1080,
            refresh_mhz: 60_000,
        };
        let logical = |scale: f64| DisplayLogical { x: 0, y: 0, scale };

        let built_in = Display {
            connector: "eDP-1".to_owned(),
            label: "Built-in display".to_owned(),
            current_mode: Some(mode.clone()),
            logical: Some(logical(2.0)),
            enabled: true,
            ..Display::default()
        };
        let external = Display {
            connector: "DP-1".to_owned(),
            label: "DELL U2720Q".to_owned(),
            make: Some("Dell Inc.".to_owned()),
            model: Some("U2720Q".to_owned()),
            serial: Some("8QK1P93".to_owned()),
            current_mode: Some(mode.clone()),
            logical: Some(logical(1.25)),
            enabled: true,
        };

        for popover in tagged::<DisplayPopover>(root, "one") {
            popover.set_output_power(true);
            popover.set_displays(std::slice::from_ref(&built_in));
        }
        for popover in tagged::<DisplayPopover>(root, "two") {
            popover.set_output_power(true);
            popover.set_displays(&[built_in.clone(), external.clone()]);
        }
        for popover in tagged::<DisplayPopover>(root, "disabled") {
            popover.set_output_power(true);
            let mut disabled = external.clone();
            disabled.enabled = false;
            popover.set_displays(&[built_in.clone(), disabled]);
        }
        for popover in tagged::<DisplayPopover>(root, "no_power") {
            popover.set_output_power(false);
            popover.set_displays(&[built_in.clone(), external.clone()]);
        }
    }

    /// Where the row carrying `key` sits in `entries`. Rows cover `entries[1..]`, and the list
    /// reports which player was clicked rather than which position, so the fixture looks it up.
    fn other(media: &Media, key: &str) -> Option<usize> {
        media
            .entries
            .iter()
            .skip(1)
            .position(|entry| entry.source().name == key)
            .map(|index| index + 1)
    }

    fn show(media: &Media, player: &NowPlaying, list: &PlayerList) {
        let current = &media.entries[0];
        let song = current.song();
        let source = current.source();

        player.set_source(Some(source.name));
        player.set_icon_name(Some(source.icon_name));
        player.set_title(Some(song.title));
        player.set_artist(Some(song.artist));
        player.set_album(Some(song.album));
        player.set_art(Some(&media.covers[current.source]));

        let scrubber = player.scrubber();
        scrubber.set_duration(song.duration);
        scrubber.set_position(current.position);
        scrubber.set_seekable(source.seekable);

        let transport = player.transport();
        transport.set_playing(current.playing);
        transport.set_can_next(source.songs.len() > 1);
        transport.set_can_shuffle(true);
        transport.set_can_repeat(true);
        transport.set_shuffle(media.shuffle);
        transport.set_repeat(media.repeat);

        let players: Vec<Player> = media.entries[1..]
            .iter()
            .map(|entry| Player {
                key: entry.source().name.to_owned(),
                name: entry.source().name.to_owned(),
                icon_name: entry.source().icon_name.to_owned(),
                title: entry.song().title.to_owned(),
                artist: entry.song().artist.to_owned(),
                playing: entry.playing,
            })
            .collect();
        list.set_players(&players);
    }

    /// Nothing in a preview can reach a real `mpris:artUrl`, so the cover is generated: a diagonal
    /// blend between the source's colour and a darkened version of it. It exists to prove the
    /// rounded clip and the fallback swap, not to look like a record sleeve.
    fn artwork((r, g, b): (u8, u8, u8)) -> gdk::Texture {
        const SIZE: usize = 192;
        let mut pixels = Vec::with_capacity(SIZE * SIZE * 4);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let blend = (x + y) as f32 / (2 * SIZE) as f32;
                let shade = |channel: u8| (channel as f32 * (1.0 - 0.55 * blend)) as u8;
                pixels.extend_from_slice(&[shade(r), shade(g), shade(b), u8::MAX]);
            }
        }
        gdk::MemoryTexture::new(
            SIZE as i32,
            SIZE as i32,
            gdk::MemoryFormat::R8g8b8a8,
            &glib::Bytes::from_owned(pixels),
            SIZE * 4,
        )
        .upcast()
    }

    fn notifications(root: &gtk4::Widget, groups: Vec<Group>) {
        let Some(popover) = find::<NotificationsPopover>(root) else {
            eprintln!("the board carries no $NotificationsPopover, so there is nothing to fill");
            return;
        };

        popover.set_clear_label(Some("Clear all"));
        popover.set_footer(Some("Notification settings"));
        let shown = Rc::new(RefCell::new(groups));

        let push = |popover: &NotificationsPopover, shown: &Rc<RefCell<Vec<Group>>>| {
            let groups = shown.borrow().clone();
            popover.set_groups(&groups);
        };
        push(&popover, &shown);

        popover.connect_activated(|_, key| eprintln!("popover: {key} opened"));
        popover.connect_action_invoked(|_, key, action| eprintln!("popover: {key} -> {action}"));
        popover.connect_dismissed(glib::clone!(
            #[strong]
            shown,
            move |popover, key| {
                for group in shown.borrow_mut().iter_mut() {
                    group.notifications.retain(|note| note.key != key);
                }
                shown
                    .borrow_mut()
                    .retain(|group| !group.notifications.is_empty());
                push(popover, &shown);
                eprintln!("popover: {key} dismissed");
            }
        ));
        popover.connect_clear_all(glib::clone!(
            #[strong]
            shown,
            move |popover| {
                shown.borrow_mut().clear();
                push(popover, &shown);
                eprintln!("popover: clear all");
            }
        ));
        popover.connect_clear_group(glib::clone!(
            #[strong]
            shown,
            move |popover, key| {
                shown.borrow_mut().retain(|group| group.key != key);
                push(popover, &shown);
                eprintln!("popover: clear group {key}");
            }
        ));
        popover.connect_footer_activated(|_| eprintln!("popover: settings"));
        popover.connect_dnd_toggled(|_, silenced| eprintln!("popover: do not disturb {silenced}"));

        let next = Rc::new(Cell::new(1_u64));
        for button in tagged::<gtk4::Button>(root, "add") {
            button.connect_clicked(glib::clone!(
                #[weak]
                popover,
                #[strong]
                shown,
                #[strong]
                next,
                move |_| {
                    let id = next.get();
                    next.set(id + 1);
                    let notification = Notification {
                        key: format!("preview-{id}"),
                        app_name: "Preview".to_owned(),
                        summary: format!("Added notification {id}"),
                        body: Some(Body::Plain(
                            "This item was added with the preview action row.".to_owned(),
                        )),
                        when: "now".to_owned(),
                        icon: Some(themed_icon("dialog-information-symbolic")),
                        unread: true,
                        ..Notification::default()
                    };
                    {
                        let mut groups = shown.borrow_mut();
                        if let Some(group) = groups.iter_mut().find(|group| group.key == "preview")
                        {
                            group.notifications.insert(0, notification);
                        } else {
                            groups.insert(
                                0,
                                Group {
                                    key: "preview".to_owned(),
                                    app_name: "Preview".to_owned(),
                                    notifications: vec![notification],
                                },
                            );
                        }
                    }
                    popover.set_trouble(None);
                    push(&popover, &shown);
                }
            ));
        }
        for button in tagged::<gtk4::Button>(root, "remove") {
            button.connect_clicked(glib::clone!(
                #[weak]
                popover,
                #[strong]
                shown,
                move |_| {
                    {
                        let mut groups = shown.borrow_mut();
                        if let Some(group) = groups
                            .iter_mut()
                            .find(|group| !group.notifications.is_empty())
                        {
                            group.notifications.remove(0);
                        }
                        groups.retain(|group| !group.notifications.is_empty());
                    }
                    popover.set_trouble(None);
                    push(&popover, &shown);
                }
            ));
        }

        for case in [
            "variants",
            "single",
            "anonymous",
            "markup",
            "dense",
            "empty",
            "trouble",
        ] {
            for button in tagged::<gtk4::Button>(root, case) {
                button.connect_clicked(glib::clone!(
                    #[weak]
                    popover,
                    #[strong]
                    shown,
                    move |_| {
                        *shown.borrow_mut() = match case {
                            "variants" => notification_catalog(),
                            "single" => notification_catalog()[..1].to_vec(),
                            "anonymous" => anonymous(),
                            "markup" => markup_catalog(),
                            "dense" => dense(),
                            _ => Vec::new(),
                        };
                        popover.set_trouble(match case {
                            "trouble" => Some("org.freedesktop.Notifications is taken."),
                            _ => None,
                        });
                        push(&popover, &shown);
                    }
                ));
            }
        }
    }

    fn anonymous() -> Vec<Group> {
        vec![Group {
            key: "anonymous".to_owned(),
            app_name: String::new(),
            notifications: vec![Notification {
                key: "anonymous".to_owned(),
                summary: "Sender without an application name".to_owned(),
                body: Some(Body::Plain(
                    "The card keeps its content aligned when identity metadata is absent."
                        .to_owned(),
                )),
                when: "now".to_owned(),
                ..Notification::default()
            }],
        }]
    }

    fn dense() -> Vec<Group> {
        const APPS: [(&str, &str); 6] = [
            ("org.telegram.desktop", "Telegram"),
            ("com.pagerduty", "PagerDuty"),
            ("org.mozilla.thunderbird", "Thunderbird"),
            ("com.slack", "Slack"),
            ("org.gnome.Software", "Software"),
            ("me.aresa.glimpse", "glimpse"),
        ];

        APPS.iter()
            .map(|(key, app)| Group {
                key: (*key).to_owned(),
                app_name: (*app).to_owned(),
                notifications: (0..8)
                    .map(|index| Notification {
                        key: format!("{key}-{index}"),
                        summary: format!("{app} message {}", index + 1),
                        body: Some(Body::Plain(format!(
                            "Notification number {} from this application today.",
                            index + 1
                        ))),
                        when: format!("{}m", (index + 1) * 3),
                        icon: Some(themed_icon("user-available-symbolic")),
                        unread: index < 2,
                        ..Notification::default()
                    })
                    .collect(),
            })
            .collect()
    }

    fn notification_catalog() -> Vec<Group> {
        let note = |key: &str, app: &str, summary: &str, body: &str, when: &str| Notification {
            key: key.to_owned(),
            app_name: app.to_owned(),
            summary: summary.to_owned(),
            body: Some(Body::Plain(body.to_owned())),
            when: when.to_owned(),
            ..Notification::default()
        };

        vec![
            Group {
                key: "org.signal.Signal".to_owned(),
                app_name: "Signal".to_owned(),
                notifications: vec![Notification {
                    unread: true,
                    ..note(
                        "app-name",
                        "Signal",
                        "Application name",
                        "This notification has an application name without an icon or image.",
                        "now",
                    )
                }],
            },
            Group {
                key: "org.telegram.desktop".to_owned(),
                app_name: "Telegram".to_owned(),
                notifications: vec![Notification {
                    icon: Some(themed_icon("user-available-symbolic")),
                    unread: true,
                    activatable: true,
                    actions: vec![
                        Action {
                            key: "reply".to_owned(),
                            label: "Reply".to_owned(),
                        },
                        Action {
                            key: "mute".to_owned(),
                            label: "Mute".to_owned(),
                        },
                    ],
                    ..note(
                        "app-icon",
                        "Telegram",
                        "Marta Kaz",
                        "This notification has an application icon and name.",
                        "2m",
                    )
                }],
            },
            Group {
                key: "org.gnome.Screenshot".to_owned(),
                app_name: "Screenshots".to_owned(),
                notifications: vec![Notification {
                    image: Some(artwork((196, 108, 62))),
                    activatable: true,
                    actions: vec![
                        Action {
                            key: "open".to_owned(),
                            label: "Open".to_owned(),
                        },
                        Action {
                            key: "copy".to_owned(),
                            label: "Copy path".to_owned(),
                        },
                    ],
                    ..note(
                        "screenshot",
                        "Screenshots",
                        "Screenshot captured",
                        "Saved to ~/Pictures/Screenshots",
                        "12m",
                    )
                }],
            },
            Group {
                key: "org.gnome.Software".to_owned(),
                app_name: "Software".to_owned(),
                notifications: vec![Notification {
                    progress: Some(0.68),
                    ..note(
                        "software-progress",
                        "Software",
                        "Installing updates",
                        "Downloading 14 of 21 packages",
                        "4m",
                    )
                }],
            },
            Group {
                key: "org.gnome.SettingsDaemon.Power".to_owned(),
                app_name: "Power".to_owned(),
                notifications: vec![Notification {
                    urgency: Urgency::Critical,
                    unread: true,
                    ..note(
                        "battery-critical",
                        "Power",
                        "Battery critically low",
                        "Connect the charger to avoid losing your work.",
                        "1m",
                    )
                }],
            },
            Group {
                key: "org.gnome.Calendar".to_owned(),
                app_name: "Calendar".to_owned(),
                notifications: vec![Notification {
                    key: "app-icon".to_owned(),
                    app_name: "Calendar".to_owned(),
                    summary: "Application icon".to_owned(),
                    body: Some(Body::Plain(
                        "This notification has an application name and themed icon.".to_owned(),
                    )),
                    when: "5m".to_owned(),
                    icon: Some(themed_icon("x-office-calendar-symbolic")),
                    ..Notification::default()
                }],
            },
            Group {
                key: "org.mozilla.Thunderbird".to_owned(),
                app_name: "Thunderbird".to_owned(),
                notifications: (1..=4)
                    .map(|index| {
                        note(
                            &format!("mail-{index}"),
                            "Thunderbird",
                            &format!("Message {index}"),
                            "A grouped notification in the collapsed stack.",
                            &format!("{}m", index * 3),
                        )
                    })
                    .collect(),
            },
        ]
    }

    const NOTIFICATION_BODIES: [(&str, &str); 9] = [
        ("Bold text", "<b>Alice</b>\nHey there"),
        (
            "Link and entity",
            r#"New <a href="https://example.com">message</a> &#9733; from Bob"#,
        ),
        (
            "Non-breaking space",
            "Reminder&nbsp;&mdash;&nbsp;standup at 10:00",
        ),
        (
            "Typographic entities",
            "&mdash; &hellip; &rsquo; &copy; &trade; &euro;",
        ),
        (
            "Unsupported span",
            r#"<span foreground="red" size="50pt">huge and red</span>"#,
        ),
        (
            "Script tag",
            "<script>alert(1)</script>the rest of the body",
        ),
        ("Bare ampersand", "5 < 10 && AT&T said so"),
        ("Unbalanced tag", "<b>bold that never closes"),
        ("Bidi override", "Lunch\u{202e}gpj.exe"),
    ];

    fn markup_catalog() -> Vec<Group> {
        vec![Group {
            key: "markup".to_owned(),
            app_name: "Markup sanitizer".to_owned(),
            notifications: NOTIFICATION_BODIES
                .iter()
                .enumerate()
                .map(|(index, (summary, raw))| Notification {
                    key: format!("markup-{index}"),
                    app_name: "Markup sanitizer".to_owned(),
                    summary: (*summary).to_owned(),
                    body: Some(Body::Markup(glimpse_utils::markup::sanitize_body(raw))),
                    when: format!("{}m", index + 1),
                    icon: Some(themed_icon("format-text-rich-symbolic")),
                    ..Notification::default()
                })
                .collect(),
        }]
    }

    fn themed_icon(name: &str) -> gtk4::gio::Icon {
        gtk4::gio::ThemedIcon::new(name).upcast()
    }

    fn tagged<T: IsA<gtk4::Widget>>(root: &gtk4::Widget, case: &str) -> Vec<T> {
        let wanted = format!("{DEMO}{case}");
        collect::<T>(root)
            .into_iter()
            .filter(|widget| widget.as_ref().has_css_class(&wanted))
            .collect()
    }

    fn page_stack(root: &gtk4::Widget) -> Option<(gtk4::Revealer, gtk4::Stack)> {
        collect::<gtk4::Revealer>(root)
            .into_iter()
            .find_map(|revealer| {
                let stack = revealer
                    .child()
                    .and_then(|child| find::<gtk4::Stack>(&child))?;
                Some((revealer, stack))
            })
    }

    fn drawer_nav(root: &gtk4::Widget) {
        let Some((drawer, stack)) = page_stack(root) else {
            if collect::<gtk4::Button>(root)
                .iter()
                .any(|row| row.css_classes().iter().any(|c| c.starts_with(NAV)))
            {
                eprintln!("no Gtk.Revealer holds a Gtk.Stack; every {NAV} row is dead");
            }
            return;
        };

        let rows: Rc<Vec<(gtk4::Widget, String)>> = Rc::new(
            collect::<gtk4::Widget>(root)
                .into_iter()
                .filter(|widget| widget.is::<gtk4::Button>() || widget.is::<SplitRow>())
                .filter_map(|row| {
                    let page = row
                        .css_classes()
                        .iter()
                        .find_map(|class| class.as_str().strip_prefix(NAV).map(str::to_owned))?;
                    if stack.child_by_name(&page).is_none() {
                        eprintln!("{NAV}{page} names no page in the stack; that row is dead");
                        return None;
                    }
                    Some((row, page))
                })
                .collect(),
        );

        for (index, (row, page)) in rows.iter().enumerate() {
            let all = Rc::clone(&rows);
            let page = page.clone();
            let drawer = drawer.clone();
            let stack = stack.clone();
            let show = move || {
                let showing = drawer.reveals_child()
                    && stack.visible_child_name().as_deref() == Some(page.as_str());
                for (other, _) in all.iter() {
                    set_selected(other, false);
                }
                if showing {
                    drawer.set_reveal_child(false);
                    return;
                }
                stack.set_visible_child_name(&page);
                set_selected(&all[index].0, true);
                drawer.set_reveal_child(true);
            };

            match row.downcast_ref::<SplitRow>() {
                Some(split) => {
                    split.connect_details(move |_| show());
                }
                None => {
                    let button = row.downcast_ref::<gtk4::Button>().expect("filtered above");
                    button.connect_clicked(move |_| show());
                }
            }
        }
    }

    fn actions(root: &gtk4::Widget) {
        for widget in collect::<gtk4::Widget>(root) {
            let Some(name) = widget
                .css_classes()
                .iter()
                .find_map(|class| class.as_str().strip_prefix(ACTION).map(str::to_owned))
            else {
                continue;
            };

            if let Some(split) = widget.downcast_ref::<SplitRow>() {
                split.connect_activated(move |_| eprintln!("action: {name}"));
            } else if let Some(button) = widget.downcast_ref::<gtk4::Button>() {
                button.connect_clicked(move |_| eprintln!("action: {name}"));
            } else {
                eprintln!(
                    "{ACTION}{name} sits on a {}, which nothing clicks",
                    widget.type_().name()
                );
            }
        }
    }

    fn busy(root: &gtk4::Widget) {
        for widget in collect::<gtk4::Widget>(root) {
            if !widget.has_css_class(BUSY) {
                continue;
            }
            match widget.downcast_ref::<SplitRow>() {
                Some(split) => split.row().set_busy(true),
                None => eprintln!(
                    ".{BUSY} sits on a {}; a $Row takes busy and a Gtk.Spinner takes spinning \
                     straight from the blueprint",
                    widget.type_().name()
                ),
            }
        }
    }

    fn indicators(root: &gtk4::Widget) {
        for indicator in collect::<Indicator>(root) {
            let classes = indicator.css_classes();
            let named = |prefix: &str| {
                classes
                    .iter()
                    .find_map(|class| class.as_str().strip_prefix(prefix).map(str::to_owned))
            };
            let flagged = |flag: &str| classes.iter().any(|class| class.as_str() == flag);

            let icon = named(ICON);
            let overlay = named(OVERLAY);
            let severity = named(SEVERITY);
            let attention = flagged(ATTENTION);
            let notice = flagged(NOTICE);

            if icon.is_none() && overlay.is_none() && severity.is_none() && !attention && !notice {
                continue;
            }

            match icon {
                Some(icon) => indicator.set_icon(Some(&themed_icon(&icon))),
                None if overlay.is_none() && !attention && !notice => {
                    eprintln!("an $Indicator carries no {ICON} class and nothing else to draw")
                }
                None => {}
            }

            if let Some(overlay) = overlay {
                indicator.set_overlay(Some(&themed_icon(&overlay)));
            }

            match severity.as_deref() {
                Some("info") => indicator.set_severity(Some(Severity::Info)),
                Some("warning") => indicator.set_severity(Some(Severity::Warning)),
                Some("error") => indicator.set_severity(Some(Severity::Error)),
                Some(other) => eprintln!("{SEVERITY}{other} is not a severity"),
                None => {}
            }

            indicator.set_attention(attention);
            indicator.set_notice(notice);
        }
    }

    fn tray_states(root: &gtk4::Widget) {
        let chip = |key: &str, icon: &str, spec: IndicatorSpec| TrayChip {
            key: key.to_owned(),
            spec: IndicatorSpec {
                icon: Some(themed_icon(icon)),
                tooltip: Some(key.to_owned()),
                ..spec
            },
        };

        for strip in collect::<TrayStrip>(root) {
            let case = strip
                .css_classes()
                .iter()
                .find_map(|class| class.as_str().strip_prefix(DEMO).map(str::to_owned))
                .unwrap_or_default();
            let items = vec![
                chip(
                    "plain",
                    "folder-publicshare-symbolic",
                    IndicatorSpec::default(),
                ),
                chip(
                    "label",
                    "network-transmit-receive-symbolic",
                    IndicatorSpec {
                        label: Some("1.2 MB/s".to_owned()),
                        ..Default::default()
                    },
                ),
                chip(
                    "badge",
                    "mail-unread-symbolic",
                    IndicatorSpec {
                        badge: Some("12".to_owned()),
                        ..Default::default()
                    },
                ),
                chip(
                    "attention",
                    "chat-message-new-symbolic",
                    IndicatorSpec {
                        attention: true,
                        ..Default::default()
                    },
                ),
                chip(
                    "attention and badge",
                    "chat-message-new-symbolic",
                    IndicatorSpec {
                        badge: Some("99+".to_owned()),
                        attention: true,
                        ..Default::default()
                    },
                ),
                chip(
                    "overlay",
                    "folder-publicshare-symbolic",
                    IndicatorSpec {
                        overlay: Some(themed_icon("emblem-synchronizing-symbolic")),
                        ..Default::default()
                    },
                ),
                chip(
                    "warning",
                    "dialog-warning-symbolic",
                    IndicatorSpec {
                        severity: Some(Severity::Warning),
                        ..Default::default()
                    },
                ),
                chip(
                    "error",
                    "dialog-error-symbolic",
                    IndicatorSpec {
                        severity: Some(Severity::Error),
                        ..Default::default()
                    },
                ),
                chip(
                    "missing icon",
                    "no-such-icon-symbolic",
                    IndicatorSpec::default(),
                ),
            ];

            strip.set_overflow_tooltip(Some("Show the rest"));
            match case.as_str() {
                "overflow" => strip.set_max_visible(4),
                _ => strip.set_max_visible(0),
            }
            strip.set_items(&items);
        }
    }

    fn inhibitor_list_states(root: &gtk4::Widget) {
        let entry = |id: u64,
                     source: InhibitorSource,
                     label: &str,
                     status: &str,
                     targets: InhibitorTargets,
                     can_release: bool| InhibitorEntry {
            id,
            source,
            label: label.to_owned(),
            status: status.to_owned(),
            targets,
            can_release,
        };

        let zoom = entry(
            1,
            InhibitorSource::ScreenSaver,
            "Zoom",
            "screen sharing · 2m ago",
            InhibitorTargets {
                idle: true,
                suspend: true,
                ..InhibitorTargets::default()
            },
            true,
        );

        for list in collect::<InhibitorList>(root) {
            let case = list
                .css_classes()
                .iter()
                .find_map(|class| class.as_str().strip_prefix(DEMO).map(str::to_owned))
                .unwrap_or_default();

            match case.as_str() {
                "empty" => list.set_inhibitors(&[]),
                "one" => list.set_inhibitors(std::slice::from_ref(&zoom)),
                "mix" => list.set_inhibitors(&[
                    zoom.clone(),
                    entry(
                        2,
                        InhibitorSource::Portal,
                        "Steam (Flatpak)",
                        "playing a video · (Flatpak via portal)",
                        InhibitorTargets {
                            idle: true,
                            shutdown: true,
                            ..InhibitorTargets::default()
                        },
                        false,
                    ),
                    entry(
                        3,
                        InhibitorSource::Login1,
                        "packagekitd",
                        "installing updates · (systemd-inhibit · pid 1183)",
                        InhibitorTargets {
                            shutdown: true,
                            power_key: true,
                            suspend_key: true,
                            ..InhibitorTargets::default()
                        },
                        true,
                    ),
                    entry(
                        4,
                        InhibitorSource::ManualHold,
                        "Keep awake",
                        "manual hold · until 16:24",
                        InhibitorTargets {
                            idle: true,
                            suspend: true,
                            ..InhibitorTargets::default()
                        },
                        true,
                    ),
                ]),
                _ => {}
            }
        }
    }

    fn printing_popover_states(root: &gtk4::Widget) {
        for popover in collect::<PrintingPopover>(root) {
            let Some(case) = popover
                .css_classes()
                .iter()
                .find_map(|class| class.as_str().strip_prefix(DEMO).map(str::to_owned))
            else {
                eprintln!("a $PrintingPopover carries no {DEMO} class, so it stays empty");
                continue;
            };

            match case.as_str() {
                "empty" => {
                    popover.set_jobs(&[]);
                    popover.set_printers(&[]);
                }
                "busy" => {
                    popover.set_jobs(&[
                        PrintingJob {
                            id: "1".into(),
                            name: "specs-004-panel-final-review-draft.pdf".into(),
                            printer: "HP LaserJet 400".into(),
                            status: "Printing".into(),
                            progress: Some((3, 12)),
                            busy: true,
                            cancellable: true,
                            pausable: true,
                            resumable: false,
                        },
                        PrintingJob {
                            id: "2".into(),
                            name: "boarding-pass.pdf".into(),
                            printer: "Kitchen".into(),
                            status: "Queued".into(),
                            progress: None,
                            busy: false,
                            cancellable: true,
                            pausable: false,
                            resumable: false,
                        },
                        PrintingJob {
                            id: "3".into(),
                            name: "invoice-final.pdf".into(),
                            printer: "HP LaserJet 400".into(),
                            status: "Held".into(),
                            progress: None,
                            busy: false,
                            cancellable: true,
                            pausable: false,
                            resumable: true,
                        },
                    ]);
                    popover.set_printers(&[
                        PrintingPrinter {
                            id: "laserjet".into(),
                            name: "HP LaserJet 400".into(),
                            status: "Default · paper jam".into(),
                            network: false,
                            details: vec![
                                PrintingDetail {
                                    label: "Location".into(),
                                    value: "Study".into(),
                                },
                                PrintingDetail {
                                    label: "Problem".into(),
                                    value: "Paper jam in tray 2".into(),
                                },
                                PrintingDetail {
                                    label: "Paper loaded".into(),
                                    value: "A4, Letter".into(),
                                },
                            ],
                        },
                        PrintingPrinter {
                            id: "kitchen".into(),
                            name: "Kitchen".into(),
                            status: "IPP Everywhere · idle".into(),
                            network: true,
                            details: vec![
                                PrintingDetail {
                                    label: "Location".into(),
                                    value: "Kitchen".into(),
                                },
                                PrintingDetail {
                                    label: "Prints".into(),
                                    value: "Black & white, single-sided".into(),
                                },
                                PrintingDetail {
                                    label: "Resolution".into(),
                                    value: "600 dpi".into(),
                                },
                            ],
                        },
                    ]);
                }
                _ => {
                    eprintln!("{DEMO}{case} names no printing case");
                    popover.set_jobs(&[]);
                    popover.set_printers(&[]);
                }
            }
        }
    }

    fn privacy_popover_states(root: &gtk4::Widget) {
        for popover in collect::<PrivacyPopover>(root) {
            let Some(case) = popover
                .css_classes()
                .iter()
                .find_map(|class| class.as_str().strip_prefix(DEMO).map(str::to_owned))
            else {
                eprintln!("a $PrivacyPopover carries no {DEMO} class, so it stays empty");
                continue;
            };

            match case.as_str() {
                "empty" => popover.set_usages(&[]),
                "in_use" => {
                    popover.set_usages(&[
                        PrivacyUsage {
                            id: "camera".into(),
                            icon: "camera-web-symbolic".into(),
                            title: "Camera".into(),
                            detail: Some("Zoom · since 14:02".into()),
                            stoppable: false,
                        },
                        PrivacyUsage {
                            id: "microphone".into(),
                            icon: "audio-input-microphone-symbolic".into(),
                            title: "Microphone".into(),
                            detail: Some("Zoom · since 14:02".into()),
                            stoppable: false,
                        },
                        PrivacyUsage {
                            id: "screen".into(),
                            icon: "video-display-symbolic".into(),
                            title: "Screen".into(),
                            detail: Some("OBS Studio · sharing DP-1 since 13:41".into()),
                            stoppable: true,
                        },
                        PrivacyUsage {
                            id: "location".into(),
                            icon: "find-location-symbolic".into(),
                            title: "Location".into(),
                            detail: None,
                            stoppable: false,
                        },
                    ]);
                    popover.set_screen_shared(Some("OBS Studio · sharing DP-1 since 13:41"));
                }
                _ => {
                    eprintln!("{DEMO}{case} names no privacy case");
                    popover.set_usages(&[]);
                }
            }
        }
    }

    fn tray(root: &gtk4::Widget) {
        let group = gio::SimpleActionGroup::new();

        for name in [
            "activate",
            "open-browser",
            "open-folder",
            "open-file",
            "open-inbox",
            "compose",
            "mark-read",
            "conflicts",
            "reconnect",
            "disconnect",
            "settings",
            "about",
            "quit",
        ] {
            let action = gio::SimpleAction::new(name, None);
            action.connect_activate(move |_, _| eprintln!("action: tray.{name}"));
            group.add_action(&action);
        }

        for (name, on) in [("pause", false), ("notify", true), ("autostart", true)] {
            let action = gio::SimpleAction::new_stateful(name, None, &on.to_variant());
            action.connect_activate(move |action, _| {
                let next = !action
                    .state()
                    .and_then(|on| on.get::<bool>())
                    .unwrap_or(false);
                action.set_state(&next.to_variant());
                eprintln!("action: tray.{name} = {next}");
            });
            group.add_action(&action);
        }

        for (name, initial) in [("limit", "none"), ("quality", "high")] {
            let action = gio::SimpleAction::new_stateful(
                name,
                Some(&String::static_variant_type()),
                &initial.to_variant(),
            );
            action.connect_activate(move |action, target| {
                let Some(target) = target else { return };
                action.set_state(target);
                eprintln!("action: tray.{name} = {}", target.str().unwrap_or_default());
            });
            group.add_action(&action);
        }

        let logout = gio::SimpleAction::new("logout", None);
        logout.set_enabled(false);
        group.add_action(&logout);

        root.insert_action_group("tray", Some(&group));

        if std::env::var_os("GLIMPSE_PREVIEW_POPUP").is_some()
            && let Some(button) = find::<gtk4::MenuButton>(root)
        {
            glib::timeout_add_local(Duration::from_millis(500), move || {
                if button.popover().is_some_and(|menu| !menu.is_visible()) {
                    button.popup();
                }
                glib::ControlFlow::Continue
            });
        }
    }

    fn pager(root: &gtk4::Widget) {
        for pager in collect::<Pager>(root) {
            let Some(case) = pager
                .css_classes()
                .iter()
                .find_map(|class| class.as_str().strip_prefix(DEMO).map(str::to_owned))
            else {
                eprintln!("a $Pager carries no {DEMO} class, so it stays empty");
                continue;
            };

            let (shape, slots, windows) = pager_case(&case);
            pager.set_shape(shape);
            pager.set_slots(&slots);

            pager.connect_pressed({
                let case = case.clone();
                move |_| eprintln!("pager: {case} opens the popover")
            });

            let state = Rc::new(RefCell::new(slots));
            pager.connect_stepped(move |pager, horizontal, forward| {
                let way = if forward { "next" } else { "previous" };
                let (strip, other) = match windows {
                    true => ("window", "workspace"),
                    false => ("workspace", "window"),
                };

                if horizontal {
                    eprintln!("pager: {case} focuses the {way} {other}");
                    return;
                }

                let mut slots = state.borrow_mut();
                if slots.is_empty() {
                    return;
                }
                advance(&mut slots, forward);
                pager.set_slots(&slots);
                eprintln!("pager: {case} focuses the {way} {strip}");
            });
        }
    }

    fn advance(slots: &mut [Slot], forward: bool) {
        let count = slots.len();
        let at = slots
            .iter()
            .position(|slot| slot.focus != Focus::None)
            .unwrap_or(0);
        let next = match forward {
            true => (at + 1) % count,
            false => (at + count - 1) % count,
        };
        if next != at {
            slots[next].focus = slots[at].focus;
            slots[at].focus = Focus::None;
        }
    }

    fn workspace(id: u64, label: &str, tooltip: &str) -> Slot {
        Slot {
            id,
            label: label.to_owned(),
            tooltip: tooltip.to_owned(),
            ..Slot::default()
        }
    }

    fn pager_case(case: &str) -> (Shape, Vec<Slot>, bool) {
        let mut slots = vec![
            workspace(1, "1", "Workspace 1 · Browsing"),
            workspace(2, "2", "Workspace 2 · glimpse"),
            workspace(3, "3", "Workspace 3 · Notes"),
            workspace(4, "4", "Workspace 4 · empty"),
        ];
        for slot in slots.iter_mut().take(3) {
            slot.occupied = true;
        }
        slots[1].focus = Focus::Here;

        match case {
            "workspaces" => (Shape::Dots, slots, false),
            "elsewhere" => {
                slots[1].focus = Focus::Elsewhere;
                (Shape::Dots, slots, false)
            }
            "urgent" => {
                slots[2].urgent = true;
                (Shape::Dots, slots, false)
            }
            "windows" => {
                let mut windows = vec![
                    workspace(11, "1", "Alacritty · just verify"),
                    workspace(12, "2", "Zed · preview.rs — glimpse"),
                    workspace(13, "3", "Nautilus · widget_examples"),
                ];
                for window in windows.iter_mut() {
                    window.occupied = true;
                }
                windows[1].focus = Focus::Here;
                (Shape::Dots, windows, true)
            }
            "labels" => (Shape::Labels, slots, false),
            "named" => {
                let named = ["Browsing", "glimpse", "Notes", "4"];
                for (slot, name) in slots.iter_mut().zip(named) {
                    slot.label = name.to_owned();
                }
                slots[2].urgent = true;
                (Shape::Labels, slots, false)
            }
            "empty" => (Shape::Dots, Vec::new(), false),
            _ => {
                eprintln!("{DEMO}{case} names no pager case");
                (Shape::Dots, Vec::new(), false)
            }
        }
    }

    fn set_selected(widget: &gtk4::Widget, selected: bool) {
        if let Some(row) = widget.downcast_ref::<Row>() {
            row.set_selected(selected);
        }
        if let Some(split) = widget.downcast_ref::<SplitRow>() {
            split.set_selected(selected);
        }
    }

    fn expanders(root: &gtk4::Widget) {
        for row in collect::<gtk4::Button>(root) {
            if !row.has_css_class(EXPAND) {
                continue;
            }
            let Some(revealer) = row.next_sibling().and_downcast::<gtk4::Revealer>() else {
                eprintln!("a .{EXPAND} row has no Gtk.Revealer after it; it expands nothing");
                continue;
            };
            row.connect_clicked(move |row| {
                revealer.set_reveal_child(!revealer.reveals_child());
                if let Some(host) = dim_host(row.upcast_ref()) {
                    recede(&host);
                }
            });
        }
    }

    /// The nearest ancestor that asked for its rows to recede behind an open expander.
    ///
    /// Opt-in, because `.expander` also reveals an audio stream's volume slider, and dimming a
    /// whole popover around a slider would be wrong.
    fn dim_host(widget: &gtk4::Widget) -> Option<gtk4::Widget> {
        let mut node = widget.parent();
        while let Some(current) = node {
            if current.has_css_class(DIM_HOST) {
                return Some(current);
            }
            node = current.parent();
        }
        None
    }

    /// Everything outside an open detail dims, the row that opened it does not.
    ///
    /// Derived from what is revealed rather than remembered, so two expanders open at once leave
    /// both details lit and closing one does not undim the other's neighbours.
    fn recede(host: &gtk4::Widget) {
        let open: Vec<gtk4::Revealer> = collect::<gtk4::Revealer>(host)
            .into_iter()
            .filter(|revealer| revealer.reveals_child())
            .collect();

        for widget in collect::<gtk4::Widget>(host) {
            if !(widget.is::<Row>() || widget.is::<SplitRow>() || widget.is::<Hero>()) {
                continue;
            }
            let inside = open
                .iter()
                .any(|revealer| widget.is_ancestor(revealer.upcast_ref::<gtk4::Widget>()));
            let opener = widget
                .next_sibling()
                .and_downcast::<gtk4::Revealer>()
                .is_some_and(|revealer| revealer.reveals_child());

            match !open.is_empty() && !inside && !opener {
                true => widget.add_css_class(DIMMED),
                false => widget.remove_css_class(DIMMED),
            }
            match opener {
                true => widget.add_css_class(OPENED),
                false => widget.remove_css_class(OPENED),
            }
        }
    }

    /// A `Gtk.MenuButton` carrying `.open-on-map` shows its menu once the board is on screen.
    ///
    /// A blueprint cannot do this: `active: true` is applied by `Builder` before the button is
    /// realized, and `GtkMenuButton` drops it, so the popup is never even created — no
    /// `xdg_positioner` reaches the compositor. Without this a board can only show a menu if
    /// someone clicks it, and `just click` cannot reliably reach a preview window
    /// (`glimpse-cd67`), which is what made a real `Gtk.PopoverMenu` look broken here.
    ///
    /// The popup is deferred to an idle after the first frame because its anchor rectangle is the
    /// button's *allocation*: popping up before layout anchors it to a zero-sized rectangle, and a
    /// compositor legitimately squeezes that popup to nothing.
    fn opened_menus(root: &gtk4::Widget) {
        for button in collect::<gtk4::MenuButton>(root) {
            if !button.has_css_class(OPEN) {
                continue;
            }
            if button.popover().is_none() {
                eprintln!("a .{OPEN} menu button has no menu to open");
                continue;
            }
            // Opened when the window becomes *active*, not merely mapped: a compositor dismisses
            // a popup belonging to an unfocused window with `xdg_popup.popup_done` the moment it
            // appears, and a preview opens on its own workspace, which is usually not the focused
            // one. Waiting for focus is what makes the menu stay up long enough to read.
            //
            // Hooked from `map`, because a fixture runs while the tree is still being built and
            // the button has no root window to ask about focus yet.
            button.connect_map(|button| {
                let Some(window) = button.root().and_downcast::<gtk4::Window>() else {
                    return;
                };
                let shown = button.clone();
                let opened = std::cell::Cell::new(false);
                let open = move |window: &gtk4::Window| {
                    if !window.is_active() || opened.replace(true) || !shown.is_mapped() {
                        return;
                    }
                    shown.popup();
                };
                open(&window);
                window.connect_is_active_notify(move |window| open(window));
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
                glimpse_widgets::drawer::toggle(drawer);
            }
        });
    }

    fn next_event(root: &gtk4::Widget) {
        if let Some(facts) = find::<FactList>(root) {
            facts.set_facts(&[
                Fact::new("Calendar", "Work"),
                Fact::new("Location", "Meeting room Kaunas"),
                Fact::new("Organizer", "Marta Kazlauskienė"),
                Fact::new("Guests", "8 · 5 accepted"),
                Fact::new("Repeats", "Every Wednesday"),
                Fact::new("Reminder", "10 minutes before"),
            ]);
        }

        let Some(events) = find::<EventList>(root) else {
            return;
        };
        let color = |hex: &str| hex.parse::<gdk::RGBA>().unwrap_or(gdk::RGBA::BLUE);
        let work = color("#3584e4");
        let home = color("#2ec27e");
        let event = |summary: &str, detail: &str, when: &str, color| Event {
            summary: summary.to_owned(),
            detail: detail.to_owned(),
            when: when.to_owned(),
            color: Some(color),
        };

        events.set_events(&[
            event("Sprint retro", "Meeting room Kaunas", "16:00", work),
            event("1:1 with Marta", "Google Meet", "17:00", work),
            event("Pick up the parcel", "Antakalnio g. 18", "18:30", home),
        ]);
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
        BatteryPopover, BrightnessPopover, Calendar, CalendarPopover, ChoiceList, ClipboardList,
        ClipboardPopover, ClockRow, ColorList, ColorPickerPopover, DisplayList, DisplayPopover,
        EventList, EventRow, FactList, Fader, ForecastDay, ForecastHour, ForecastList,
        ForecastStrip, Hero, Indicator, IndicatorGroup, InhibitorList, KeyboardPopover, Notice,
        NotificationCard, NotificationHeader, NotificationImageBody, NotificationList,
        NotificationStack, NotificationTextBody, NotificationsPopover, NowPlaying, Pager, Panel,
        Placeholder, PlayerList, PlayerRow, PopoverShell, PrintingPopover, PrivacyPopover,
        RangeBar, Readout, Row, Scrubber, Section, SessionPopover, SourceList, SplitRow, Swatch,
        SwitchRow, TooltipCard, Transport, TrayStrip, WeatherPopover, WorkspaceNamePopover,
        WorldClock,
    };

    for widget in [
        BatteryPopover::static_type(),
        BrightnessPopover::static_type(),
        Calendar::static_type(),
        CalendarPopover::static_type(),
        ChoiceList::static_type(),
        ClockRow::static_type(),
        ColorList::static_type(),
        ColorPickerPopover::static_type(),
        Swatch::static_type(),
        DisplayList::static_type(),
        DisplayPopover::static_type(),
        SourceList::static_type(),
        EventRow::static_type(),
        FactList::static_type(),
        ForecastDay::static_type(),
        ForecastHour::static_type(),
        ForecastList::static_type(),
        ForecastStrip::static_type(),
        Notice::static_type(),
        NotificationCard::static_type(),
        NotificationHeader::static_type(),
        NotificationImageBody::static_type(),
        NotificationTextBody::static_type(),
        NotificationList::static_type(),
        NotificationStack::static_type(),
        NotificationsPopover::static_type(),
        NowPlaying::static_type(),
        PlayerList::static_type(),
        PlayerRow::static_type(),
        Scrubber::static_type(),
        Transport::static_type(),
        RangeBar::static_type(),
        Readout::static_type(),
        Fader::static_type(),
        EventList::static_type(),
        Section::static_type(),
        SessionPopover::static_type(),
        WeatherPopover::static_type(),
        SplitRow::static_type(),
        SwitchRow::static_type(),
        WorldClock::static_type(),
        Hero::static_type(),
        PopoverShell::static_type(),
        Pager::static_type(),
        Panel::static_type(),
        Indicator::static_type(),
        IndicatorGroup::static_type(),
        TrayStrip::static_type(),
        TooltipCard::static_type(),
        KeyboardPopover::static_type(),
        Placeholder::static_type(),
        Row::static_type(),
        InhibitorList::static_type(),
        ClipboardList::static_type(),
        ClipboardPopover::static_type(),
        PrintingPopover::static_type(),
        PrivacyPopover::static_type(),
        WorkspaceNamePopover::static_type(),
    ] {
        let _ = widget;
    }
}

fn build(
    slot: &gtk4::Box,
    blueprint: &Path,
    fixture: Option<&str>,
    sheets: &[(PathBuf, gtk4::CssProvider)],
) {
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

    if let Some(showing) = unsafe { slot.steal_data::<adw::Dialog>("preview-dialog") } {
        showing.close();
    }

    let parentless: Vec<gtk4::Widget> = builder
        .objects()
        .into_iter()
        .filter_map(|object| object.downcast::<gtk4::Widget>().ok())
        .filter(|widget| widget.parent().is_none())
        .collect();

    let root = parentless
        .iter()
        .find(|widget| !widget.is::<adw::Dialog>())
        .or_else(|| parentless.first())
        .cloned();

    match root {
        Some(root) => {
            if let Some(fixture) = fixture {
                fixtures::apply(fixture, &root, sheets, &builder);
            }
            match root.downcast::<adw::Dialog>() {
                Ok(dialog) => {
                    dialog.present(Some(slot));
                    unsafe { slot.set_data("preview-dialog", dialog) };
                }
                Err(widget) => {
                    widget.set_halign(gtk4::Align::Center);
                    widget.set_valign(gtk4::Align::Center);
                    slot.append(&widget);
                }
            }
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

fn theme_dark_css() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/themes/adwaita/dark.css")
}

fn theme_css() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/themes/adwaita/panel.css")
}
