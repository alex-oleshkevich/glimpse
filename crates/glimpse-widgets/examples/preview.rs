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
    glimpse_utils::init_translations(None);
    glimpse_widgets::register_resources().expect("widget resources");
    ensure_types();

    let shared = blueprint.with_file_name("_shared.css");
    let sheets = vec![
        (resolve(&builtin_css()), provider()),
        (resolve(&theme_css()), provider()),
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
        ];
        for ((_, provider), priority) in sheets.iter().zip(priorities) {
            gtk4::style_context_add_provider_for_display(&display, provider, priority);
        }
        gtk4::style_context_add_provider_for_display(
            &display,
            &checkerboard,
            gtk4::STYLE_PROVIDER_PRIORITY_USER + 3,
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
        Action, Advisory, Body, Calendar, Choice, ChoiceList, Day, Event, EventList, Fact,
        FactList, Focus, Group, Hour, Indicator, IndicatorSpec, Notification, NotificationItem,
        NotificationList, NotificationStack, NotificationsPopover, NowPlaying, Pager, Player,
        PlayerList, Repeat, Row, Severity, Shape, Slot, SplitRow, TransportAction, WeatherPage,
        WeatherPopover, WorldClock, Ymd, Zone,
    };
    use gtk4::glib;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    const NAV: &str = "nav__";
    const EXPAND: &str = "expander";
    const ACTION: &str = "action__";
    const DEMO: &str = "demo__";

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
            "mpris" => mpris(root),
            "next_event" => next_event(root),
            "weather_popover" => weather_popover(root),
            "pager" => pager(root),
            "notification_states" => notification_images(root),
            "notification_markup" => notification_markup(root),
            "notification_indicator" => notification_indicators(root),
            "notification_list" => notification_list(root),
            "notification_stack" => notification_stack(root),
            "notifications" => notifications(root, filled()),
            "notifications_anonymous" => notifications(root, anonymous()),
            _ => {}
        }
        drawer_nav(root);
        expanders(root);
        actions(root);
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

    /// A paintable is not a property, so the one item that carries an image is filled here.
    ///
    /// The texture is generated wide and already inside the widget's bound, so the board shows an
    /// image that was passed through rather than one that was resampled.
    fn notification_images(root: &gtk4::Widget) {
        let mut filled = 0;
        for item in tagged::<NotificationItem>(root, "image") {
            item.set_image(Some(&banner((196, 108, 62))));
            filled += 1;
        }
        if filled == 0 {
            eprintln!("no $NotificationItem carries demo__image, so no board shows an image");
        }
        for (case, tint) in [("avatar", (74, 138, 96)), ("avatar2", (92, 104, 168))] {
            for item in tagged::<NotificationItem>(root, case) {
                item.set_app_icon(Some(avatar(tint).upcast_ref::<gtk4::gio::Icon>()));
            }
        }
        report_actions(root);
    }

    /// A sender's photo, which arrives as pixels rather than as an icon name. `gdk::Texture`
    /// implements `gio::Icon`, so it goes through the same setter a themed name does.
    fn avatar((r, g, b): (u8, u8, u8)) -> gdk::Texture {
        const SIZE: usize = 128;
        let mut pixels = Vec::with_capacity(SIZE * SIZE * 4);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let blend = (x + y) as f32 / (2 * SIZE) as f32;
                let lift = |channel: u8| {
                    (f32::from(channel) + (255.0 - f32::from(channel)) * 0.45 * blend) as u8
                };
                pixels.extend_from_slice(&[lift(r), lift(g), lift(b), u8::MAX]);
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

    fn banner((r, g, b): (u8, u8, u8)) -> gdk::Texture {
        const WIDTH: usize = 448;
        const HEIGHT: usize = 168;
        let mut pixels = Vec::with_capacity(WIDTH * HEIGHT * 4);
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let blend = (x as f32 / WIDTH as f32 + y as f32 / HEIGHT as f32) / 2.0;
                let shade = |channel: u8| (channel as f32 * (1.0 - 0.55 * blend)) as u8;
                pixels.extend_from_slice(&[shade(r), shade(g), shade(b), u8::MAX]);
            }
        }
        gdk::MemoryTexture::new(
            WIDTH as i32,
            HEIGHT as i32,
            gdk::MemoryFormat::R8g8b8a8,
            &glib::Bytes::from_owned(pixels),
            WIDTH * 4,
        )
        .upcast()
    }

    /// Raw sender input, exactly as it would arrive over the bus. Everything but `raw` goes
    /// through the shipped sanitizer first, which is the point of the board: the widget is handed
    /// only what `sanitize_body` produced, and renders it only if pango accepts it.
    const BODIES: [(&str, &str); 10] = [
        ("telegram", "<b>Alice</b>\nHey there"),
        (
            "link",
            r#"New <a href="https://example.com">message</a> &#9733; from Bob"#,
        ),
        ("nbsp", "Reminder&nbsp;&mdash;&nbsp;standup at 10:00"),
        ("entities", "&mdash; &hellip; &rsquo; &copy; &trade; &euro;"),
        (
            "span",
            r#"<span foreground="red" size="50pt">huge and red</span>"#,
        ),
        ("script", "<script>alert(1)</script>the rest of the body"),
        ("ampersand", "5 < 10 && AT&T said so"),
        ("unbalanced", "<b>bold that never closes"),
        ("bidi", "Lunch\u{202e}gpj.exe"),
        (
            "raw",
            "<b>Alice</b> &nbsp; <a href=\"https://x\">unsanitized</a>",
        ),
    ];

    fn notification_markup(root: &gtk4::Widget) {
        for (case, body) in BODIES {
            for item in tagged::<NotificationItem>(root, case) {
                item.set_tooltip_text(Some(body));
                match case {
                    "raw" => item.set_body_markup(Some(body)),
                    _ => item
                        .set_body_markup(Some(glimpse_utils::markup::sanitize_body(body).as_str())),
                }
            }
        }
    }

    /// A notification is a struct rather than a property, so the list is filled here — and the
    /// buttons mutate the same three, which is what makes `by_key` visible: reorder them and the
    /// rows move rather than being rebuilt in place.
    fn notification_list(root: &gtk4::Widget) {
        let Some(list) = find::<NotificationList>(root) else {
            eprintln!("the board carries no $NotificationList, so there is nothing to fill");
            return;
        };

        let note = |key: &str, app: &str, summary: &str, body: &str, when: &str| Notification {
            key: key.to_owned(),
            app_name: app.to_owned(),
            summary: summary.to_owned(),
            body: Some(Body::Plain(body.to_owned())),
            when: when.to_owned(),
            icon: Some(themed_icon("user-available-symbolic")),
            ..Notification::default()
        };

        let feed = Rc::new(RefCell::new(vec![
            Notification {
                unread: true,
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
                    "marta",
                    "Telegram",
                    "Marta Kaz",
                    "Are we still on for 14:00?",
                    "2m",
                )
            },
            note(
                "incident",
                "PagerDuty",
                "#incidents",
                "glimpsed restarted on host build-03.",
                "26m",
            ),
            note(
                "jonas",
                "Signal",
                "Jonas Weber",
                "Pushed the branch, take a look.",
                "18m",
            ),
        ]));

        list.set_notifications(&feed.borrow());
        list.connect_activated(|_, key| eprintln!("list: {key} opened"));
        list.connect_action_invoked(|_, key, action| eprintln!("list: {key} -> {action}"));
        list.connect_dismissed(glib::clone!(
            #[strong]
            feed,
            move |list, key| {
                feed.borrow_mut().retain(|note| note.key != key);
                list.set_notifications(&feed.borrow());
                eprintln!("list: {key} dismissed");
            }
        ));

        for case in ["reorder", "relabel", "add", "clear"] {
            for button in tagged::<gtk4::Button>(root, case) {
                button.connect_clicked(glib::clone!(
                    #[strong]
                    feed,
                    #[weak]
                    list,
                    move |_| {
                        {
                            let mut feed = feed.borrow_mut();
                            match case {
                                "reorder" => {
                                    let by = 1.min(feed.len());
                                    feed.rotate_left(by);
                                }
                                "relabel" => {
                                    if let Some(first) = feed.first_mut() {
                                        first.summary.push('!');
                                    }
                                }
                                "add" => {
                                    let at = feed.len();
                                    feed.push(note(
                                        &format!("added-{at}"),
                                        "Screenshots",
                                        "Screenshot captured",
                                        "Saved to ~/Pictures/Screenshots",
                                        "now",
                                    ));
                                }
                                _ => feed.clear(),
                            }
                        }
                        list.set_notifications(&feed.borrow());
                    }
                ));
            }
        }
    }

    fn notification_stack(root: &gtk4::Widget) {
        let note = |key: &str, app: &str, summary: &str, body: &str, when: &str| Notification {
            key: key.to_owned(),
            app_name: app.to_owned(),
            summary: summary.to_owned(),
            body: Some(Body::Plain(body.to_owned())),
            when: when.to_owned(),
            icon: Some(themed_icon("user-available-symbolic")),
            ..Notification::default()
        };

        let feed = vec![
            Notification {
                unread: true,
                actions: vec![
                    Action {
                        key: "reply".to_owned(),
                        label: "Reply".to_owned(),
                    },
                    Action {
                        key: "dismiss".to_owned(),
                        label: "Dismiss".to_owned(),
                    },
                ],
                ..note(
                    "marta",
                    "Telegram",
                    "Marta Kaz",
                    "Can you look at the deploy before standup? The tray service is still \
                     restarting on build-03.",
                    "now",
                )
            },
            note(
                "incident",
                "PagerDuty",
                "#incidents",
                "glimpsed restarted on host build-03 after an unhandled panic in the tray service.",
                "2m",
            ),
            note(
                "shot",
                "Screenshots",
                "Screenshot captured",
                "Saved to ~/Pictures/Screenshots",
                "4m",
            ),
            note(
                "backup",
                "Backups",
                "Backup completed",
                "The encrypted archive is ready.",
                "7m",
            ),
        ];

        let mut filled = false;
        for (case, collapsed, items) in [
            ("collapsed", true, feed.as_slice()),
            ("three", false, &feed[..3]),
        ] {
            for stack in tagged::<NotificationStack>(root, case) {
                stack.set_items(items);
                stack.set_collapsed(collapsed);
                stack.connect_activated(|_, key| eprintln!("stack: {key} opened"));
                stack.connect_dismissed(|_, key| eprintln!("stack: {key} dismissed"));
                stack.connect_clear_requested(|_| eprintln!("stack: clear requested"));
                stack
                    .connect_action_invoked(|_, key, action| eprintln!("stack: {key} -> {action}"));
                filled = true;
            }
        }

        if !filled {
            eprintln!("the board carries no $NotificationStack, so there is nothing to fill");
        }
    }

    fn notifications(root: &gtk4::Widget, groups: Vec<Group>) {
        let Some(popover) = find::<NotificationsPopover>(root) else {
            eprintln!("the board carries no $NotificationsPopover, so there is nothing to fill");
            return;
        };

        popover.set_clear_label(Some("Clear all"));
        popover.set_footer(Some("Notification settings"));
        let shown = Rc::new(RefCell::new(groups));
        popover.set_groups(&shown.borrow());

        let push = |popover: &NotificationsPopover, shown: &Rc<RefCell<Vec<Group>>>| {
            popover.set_groups(&shown.borrow());
        };

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

        for case in ["filled", "single", "anonymous", "dense", "empty", "trouble"] {
            for button in tagged::<gtk4::Button>(root, case) {
                button.connect_clicked(glib::clone!(
                    #[weak]
                    popover,
                    #[strong]
                    shown,
                    move |_| {
                        *shown.borrow_mut() = match case {
                            "filled" => filled(),
                            "single" => filled()[..1].to_vec(),
                            "anonymous" => anonymous(),
                            "dense" => dense(),
                            _ => Vec::new(),
                        };
                        popover.set_trouble(match case {
                            "trouble" => Some("org.freedesktop.Notifications is taken."),
                            _ => None,
                        });
                        popover.set_groups(&shown.borrow());
                    }
                ));
            }
        }
    }

    fn anonymous() -> Vec<Group> {
        let mut groups = filled()[..1].to_vec();
        groups[0].app_name.clear();
        groups
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

    fn filled() -> Vec<Group> {
        let note = |key: &str, summary: &str, body: &str, when: &str| Notification {
            key: key.to_owned(),
            summary: summary.to_owned(),
            body: Some(Body::Plain(body.to_owned())),
            when: when.to_owned(),
            icon: Some(themed_icon("user-available-symbolic")),
            ..Notification::default()
        };

        vec![
            Group {
                key: "org.telegram.desktop".to_owned(),
                app_name: "Telegram".to_owned(),
                notifications: vec![
                    Notification {
                        unread: true,
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
                        ..note("marta", "Marta Kaz", "Are we still on for 14:00?", "2m")
                    },
                    note("marta-2", "Marta Kaz", "Never mind, it settled.", "1h"),
                ],
            },
            Group {
                key: "com.pagerduty".to_owned(),
                app_name: "PagerDuty".to_owned(),
                notifications: vec![note(
                    "incident",
                    "#incidents",
                    "glimpsed restarted on host build-03.",
                    "26m",
                )],
            },
        ]
    }

    fn themed_icon(name: &str) -> gtk4::gio::Icon {
        gtk4::gio::ThemedIcon::new(name).upcast()
    }

    fn notification_indicators(root: &gtk4::Widget) {
        let themed = |name: &str| gtk4::gio::ThemedIcon::new(name).upcast::<gtk4::gio::Icon>();
        let bell = || Some(themed("preferences-system-notifications-symbolic"));
        let muted = || Some(themed("notifications-disabled-symbolic"));

        let specs: [(&str, IndicatorSpec); 6] = [
            (
                "idle",
                IndicatorSpec {
                    icon: bell(),
                    tooltip: Some("No new notifications".to_owned()),
                    ..IndicatorSpec::default()
                },
            ),
            (
                "unread",
                IndicatorSpec {
                    icon: bell(),
                    badge: Some("3".to_owned()),
                    tooltip: Some("3 new notifications".to_owned()),
                    ..IndicatorSpec::default()
                },
            ),
            (
                "attention",
                IndicatorSpec {
                    icon: bell(),
                    attention: true,
                    tooltip: Some("New notifications".to_owned()),
                    ..IndicatorSpec::default()
                },
            ),
            (
                "urgent",
                IndicatorSpec {
                    icon: bell(),
                    attention: true,
                    severity: Some(Severity::Error),
                    tooltip: Some("Battery is at 4%".to_owned()),
                    ..IndicatorSpec::default()
                },
            ),
            (
                "dnd",
                IndicatorSpec {
                    icon: muted(),
                    severity: Some(Severity::Info),
                    tooltip: Some("Do not disturb until tomorrow".to_owned()),
                    ..IndicatorSpec::default()
                },
            ),
            (
                "dnd_pending",
                IndicatorSpec {
                    icon: muted(),
                    badge: Some("12".to_owned()),
                    severity: Some(Severity::Info),
                    tooltip: Some("12 waiting, do not disturb is on".to_owned()),
                    ..IndicatorSpec::default()
                },
            ),
        ];

        for (case, spec) in &specs {
            for indicator in tagged::<Indicator>(root, case) {
                indicator.apply(spec);
            }
        }
    }

    /// Every `$NotificationItem` action in a board carries an `action__` class, so the shared
    /// `actions` fixture already reports it. This adds the item's own signals, so dismissing one
    /// in the board actually removes it.
    fn report_actions(root: &gtk4::Widget) {
        for item in collect::<NotificationItem>(root) {
            let name = item.summary().unwrap_or_default();
            item.connect_activated({
                let name = name.clone();
                move |_| eprintln!("notification: {name} opened")
            });
            item.connect_dismissed({
                let name = name.clone();
                move |item| {
                    eprintln!("notification: {name} dismissed");
                    item.set_visible(false);
                }
            });
        }
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
            row.connect_clicked(move |_| revealer.set_reveal_child(!revealer.reveals_child()));
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
        Calendar, CalendarPopover, ChoiceList, ClockRow, EventList, EventRow, FactList,
        ForecastDay, ForecastHour, ForecastList, ForecastStrip, Hero, Indicator, IndicatorGroup,
        KeyboardPopover, Notice, NotificationItem, NotificationList, NotificationStack,
        NotificationsPopover, NowPlaying, Pager, Panel, Placeholder, PlayerList, PlayerRow,
        PopoverShell, RangeBar, Readout, Row, Scrubber, Section, SplitRow, Transport,
        WeatherPopover, WorldClock,
    };

    for widget in [
        Calendar::static_type(),
        CalendarPopover::static_type(),
        ChoiceList::static_type(),
        ClockRow::static_type(),
        EventRow::static_type(),
        FactList::static_type(),
        ForecastDay::static_type(),
        ForecastHour::static_type(),
        ForecastList::static_type(),
        ForecastStrip::static_type(),
        Notice::static_type(),
        NotificationItem::static_type(),
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
        EventList::static_type(),
        Section::static_type(),
        WeatherPopover::static_type(),
        SplitRow::static_type(),
        WorldClock::static_type(),
        Hero::static_type(),
        PopoverShell::static_type(),
        Pager::static_type(),
        Panel::static_type(),
        Indicator::static_type(),
        IndicatorGroup::static_type(),
        KeyboardPopover::static_type(),
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
