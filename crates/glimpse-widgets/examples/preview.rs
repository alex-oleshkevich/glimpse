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
        Calendar, Choice, ChoiceList, Day, Event, EventList, Fact, FactList, Focus, ForecastList,
        ForecastStrip, Hour, NowPlaying, Pager, Placeholder, Player, PlayerList, Repeat, Row,
        Section, Shape, Slot, SplitRow, TransportAction, WorldClock, Ymd, Zone,
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
            "weather" => weather(root),
            "pager" => pager(root),
            _ => {}
        }
        drawer_nav(root);
        expanders(root);
        actions(root);
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

        let Some((drawer, stack)) = page_stack(root) else {
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
            move |_, index| {
                media.borrow_mut().entries.swap(0, index as usize + 1);
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
            move |_, index| {
                {
                    let mut media = media.borrow_mut();
                    let entry = &mut media.entries[index as usize + 1];
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

            pager.connect_activated({
                let case = case.clone();
                move |_, id| eprintln!("pager: {case} activates slot {id}")
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
            "numbers" => (Shape::Numbers, slots, false),
            "named" => {
                let named = ["Browsing", "glimpse", "Notes", "4"];
                for (slot, name) in slots.iter_mut().zip(named) {
                    slot.label = name.to_owned();
                }
                slots[2].urgent = true;
                (Shape::Numbers, slots, false)
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
        Calendar, ChoiceList, ClockRow, EventList, EventRow, FactList, ForecastDay, ForecastHour,
        ForecastList, ForecastStrip, Hero, Indicator, IndicatorGroup, Notice, NowPlaying, Pager,
        Panel, Placeholder, PlayerList, PlayerRow, PopoverShell, RangeBar, Readout, Row, Scrubber,
        Section, SplitRow, Transport, WorldClock,
    };

    for widget in [
        Calendar::static_type(),
        ChoiceList::static_type(),
        ClockRow::static_type(),
        EventRow::static_type(),
        FactList::static_type(),
        ForecastDay::static_type(),
        ForecastHour::static_type(),
        ForecastList::static_type(),
        ForecastStrip::static_type(),
        Notice::static_type(),
        NowPlaying::static_type(),
        PlayerList::static_type(),
        PlayerRow::static_type(),
        Scrubber::static_type(),
        Transport::static_type(),
        RangeBar::static_type(),
        Readout::static_type(),
        EventList::static_type(),
        Section::static_type(),
        SplitRow::static_type(),
        WorldClock::static_type(),
        Hero::static_type(),
        PopoverShell::static_type(),
        Pager::static_type(),
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
