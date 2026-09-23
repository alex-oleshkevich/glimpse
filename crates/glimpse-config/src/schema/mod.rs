mod appearance;
mod applets;
mod bluetooth;
mod brightness;
mod calendar;
mod clipboard;
mod color_picker;
mod geolocation;
mod idle;
mod keyboard;
mod lock;
mod monitors;
mod mpris;
mod network;
mod night_light;
mod notifications;
mod panels;
mod places;
mod power;
mod printing;
mod regional;
mod removable;
mod system_monitor;
mod wallpaper;
mod weather;

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use appearance::{Appearance, BlurSurface, ColorScheme};
pub use applets::{
    Applet, Battery as BatteryAppletConfig, BatteryIndicatorStyle,
    Bluetooth as BluetoothAppletConfig, Brightness as BrightnessAppletConfig,
    Chip as SystemMonitorChip, Clipboard as ClipboardAppletConfig, Clock as ClockConfig,
    Command as CommandAppletConfig, Common as AppletCommon, FirstDay, Kind as AppletKind,
    Mpris as MprisAppletConfig, NextEvent as NextEventConfig, NotificationIndicatorStyle,
    Notifications as NotificationsAppletConfig, Pager as PagerConfig, PagerMode, PagerScope,
    PagerShape, Place as WeatherPlace, Places as PlacesAppletConfig,
    Printing as PrintingAppletConfig, Privacy as PrivacyAppletConfig,
    Removable as RemovableAppletConfig, SystemMonitor as SystemMonitorAppletConfig,
    Timezone as ClockTimezone, Tray as TrayAppletConfig, Weather as WeatherAppletConfig,
    resolve_applet,
};
pub use bluetooth::Bluetooth;
pub use brightness::Brightness;
pub use calendar::{Calendar, Source as CalendarSource, SourceKind as CalendarSourceKind};
pub use clipboard::Clipboard;
pub use color_picker::{ColorFormat, ColorPicker};
pub use geolocation::Geolocation;
pub use idle::{Idle, Listener as IdleListener, Profile as IdleProfile, Profiles as IdleProfiles};
pub use keyboard::{Keyboard, Remember};
pub use lock::{
    Background as LockBackground, Lock, Privacy as LockPrivacy, Session as LockSession,
    SessionAction as LockSessionAction,
};
pub use monitors::Monitors;
pub use mpris::Mpris as MprisConfig;
pub use network::Network as NetworkSettings;
pub use night_light::{CLOCK, NightLight, Schedule, parse_clock};
pub use notifications::{NotificationEdge, Notifications};
pub use panels::{Margin, Panel, Position};
pub use places::Places;
pub use power::Power;
pub use printing::Printing;
pub use regional::{HourFormat, Regional, Units as RegionalUnits};
pub use removable::Removable;
pub use system_monitor::SystemMonitor;
pub use wallpaper::{Backdrop, BackdropOutput, Fit, Transition, Wallpaper, WallpaperOutput};
pub use weather::{Provider as WeatherProvider, Weather as WeatherConfig};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    pub appearance: Appearance,
    pub regional: Regional,
    pub monitors: Monitors,
    pub geolocation: Geolocation,
    pub bluetooth: Bluetooth,
    pub printing: Printing,
    pub network: NetworkSettings,
    pub night_light: NightLight,
    pub brightness: Brightness,
    pub idle: Idle,
    pub power: Power,
    pub keyboard: Keyboard,
    pub calendar: Calendar,
    pub clipboard: Clipboard,
    pub color_picker: ColorPicker,
    pub weather: WeatherConfig,
    pub mpris: MprisConfig,
    pub notifications: Notifications,
    pub wallpaper: Wallpaper,
    pub lock: Lock,
    pub places: Places,
    pub removable: Removable,
    pub system_monitor: SystemMonitor,
    pub panels: Vec<Panel>,
    #[serde(deserialize_with = "applets::deserialize")]
    #[schemars(schema_with = "applets::schema")]
    pub applets: BTreeMap<String, Applet>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            regional: Regional::default(),
            monitors: Monitors::default(),
            geolocation: Geolocation::default(),
            bluetooth: Bluetooth::default(),
            printing: Printing::default(),
            network: NetworkSettings::default(),
            night_light: NightLight::default(),
            brightness: Brightness::default(),
            idle: Idle::default(),
            power: Power::default(),
            keyboard: Keyboard::default(),
            calendar: Calendar::default(),
            clipboard: Clipboard::default(),
            color_picker: ColorPicker::default(),
            weather: WeatherConfig::default(),
            mpris: MprisConfig::default(),
            notifications: Notifications::default(),
            wallpaper: Wallpaper::default(),
            lock: Lock::default(),
            places: Places::default(),
            removable: Removable::default(),
            system_monitor: SystemMonitor::default(),
            panels: vec![Panel::default()],
            applets: BTreeMap::new(),
        }
    }
}

/// Every `Kind` actually placed on some panel's `left`/`center`/`right` zone, resolved the same
/// way a panel resolves a zone entry (see `resolve_applet`). The single source of truth for
/// "is anything actually consuming this applet" — a service's demand gating and the panel's own
/// zone resolution must not diverge.
pub fn placed_kinds(config: &Config) -> impl Iterator<Item = AppletKind> + '_ {
    config
        .panels
        .iter()
        .flat_map(|panel| panel.left.iter().chain(&panel.center).chain(&panel.right))
        .filter_map(|name| resolve_applet(name, &config.applets).map(|applet| applet.kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placed_kinds_follows_zone_placement_not_table_presence() {
        let default = Config::default();
        assert!(
            !placed_kinds(&default).any(|kind| matches!(kind, AppletKind::SystemMonitor(_))),
            "the default document places nothing named system-monitor"
        );

        let named_in_a_zone_with_no_table: Config =
            toml::from_str("[[panels]]\nright = [\"system-monitor\"]\n")
                .expect("a bare name resolves through Applet::from_name");
        assert!(
            placed_kinds(&named_in_a_zone_with_no_table)
                .any(|kind| matches!(kind, AppletKind::SystemMonitor(_))),
            "a name placed in a zone with no table entry must still resolve, the common case"
        );

        let table_with_no_placement: Config = toml::from_str("[applets.system-monitor]\n")
            .expect("a table with no zone naming it still loads");
        assert!(
            !placed_kinds(&table_with_no_placement)
                .any(|kind| matches!(kind, AppletKind::SystemMonitor(_))),
            "a table nobody places is not demand"
        );

        let renamed_and_placed: Config = toml::from_str(
            "[applets.sm]\nextends = \"system-monitor\"\n\n[[panels]]\nright = [\"sm\"]\n",
        )
        .expect("a renamed table placed in a zone loads");
        assert!(
            placed_kinds(&renamed_and_placed)
                .any(|kind| matches!(kind, AppletKind::SystemMonitor(_))),
            "a renamed table placed in a zone must still resolve to its kind"
        );
    }

    #[test]
    fn the_reference_file_matches_the_compiled_in_defaults() {
        let checked_in = include_str!("../../../../data/config.default.toml");

        assert_eq!(checked_in, crate::default_document());
    }

    #[test]
    fn the_commented_reference_file_matches_the_compiled_in_renderer() {
        let checked_in = include_str!("../../../../data/config.commented.toml");

        assert_eq!(checked_in, crate::commented_document());
    }

    #[test]
    fn the_json_schema_matches_the_compiled_in_types() {
        let checked_in = include_str!("../../../../data/config.schema.json");

        assert_eq!(checked_in, crate::json_schema_document());
    }

    #[test]
    fn the_schema_accepts_both_fixed_coordinate_spellings() {
        let schema: serde_json::Value =
            serde_json::from_str(&crate::json_schema_document()).expect("the schema is JSON");
        let tags = schema["$defs"]["Place"]["oneOf"]
            .as_array()
            .expect("place variants")
            .iter()
            .filter_map(|branch| {
                branch
                    .pointer("/properties/at/const")
                    .and_then(serde_json::Value::as_str)
            })
            .collect::<Vec<_>>();

        assert!(tags.contains(&"latlon"));
        assert!(tags.contains(&"coordinates"));
    }

    #[test]
    fn every_key_and_enum_value_is_kebab_case() {
        let schema: serde_json::Value =
            serde_json::from_str(&crate::json_schema_document()).expect("the schema is JSON");
        let mut offenders = Vec::new();
        collect_underscored(&schema, &mut offenders);

        assert!(
            offenders.is_empty(),
            "the document is kebab-case; add `rename_all` to the type owning {offenders:?}"
        );
    }

    fn collect_underscored(node: &serde_json::Value, offenders: &mut Vec<String>) {
        match node {
            serde_json::Value::Object(map) => {
                if let Some(properties) = map.get("properties").and_then(|node| node.as_object()) {
                    offenders.extend(properties.keys().filter(|key| key.contains('_')).cloned());
                }
                if let Some(values) = map.get("enum").and_then(|node| node.as_array()) {
                    offenders.extend(
                        values
                            .iter()
                            .filter_map(|value| value.as_str())
                            .filter(|value| value.contains('_'))
                            .map(str::to_owned),
                    );
                }
                for value in map.values() {
                    collect_underscored(value, offenders);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    collect_underscored(item, offenders);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn config_default_round_trips_losslessly() {
        let mut config = Config::default();
        config.printing.server_url = Some("http://localhost:631/".to_owned());
        config.printing.poll_idle = 45;
        config.applets.insert(
            "printing".to_owned(),
            Applet::from(AppletKind::Printing(applets::Printing::default())),
        );

        let written = toml::to_string(&config).expect("the document serializes");
        let again: Config = toml::from_str(&written).expect("what was written parses again");

        assert_eq!(config, again);
    }

    #[test]
    fn night_light_still_accepts_manual_for_schedule() {
        let parsed: Config =
            toml::from_str("[night-light]\nschedule = \"manual\"\n").expect("the alias parses");

        assert_eq!(parsed.night_light.schedule, Schedule::Schedule);
    }

    #[test]
    fn the_table_name_selects_the_applet_when_extends_is_absent() {
        let parsed: Config = toml::from_str("[applets.clock]\n").expect("the key names the kind");

        assert_eq!(
            parsed.applets["clock"],
            Applet::from(AppletKind::Clock(applets::Clock::default()))
        );
    }

    #[test]
    fn an_applet_table_with_no_settings_takes_the_defaults() {
        let parsed: Config = toml::from_str("[applets.clock]\n").expect("settings are optional");

        assert_eq!(
            parsed.applets["clock"],
            Applet::from(AppletKind::Clock(applets::Clock::default()))
        );
    }

    #[test]
    fn the_next_event_applet_carries_its_own_settings() {
        let parsed: Config = toml::from_str(
            "[applets.next-event]\nwithin = 15\ncountdown = 5\nhorizon = 240\nall-day = true\nupcoming = 3\n",
        )
        .expect("the table names the kind and the keys are its own");

        let AppletKind::NextEvent(settings) = &parsed.applets["next-event"].kind else {
            panic!("the table names the next-event applet");
        };
        assert_eq!(settings.within, 15);
        assert_eq!(settings.countdown, 5);
        assert_eq!(settings.horizon, 240);
        assert!(settings.all_day);
        assert_eq!(settings.upcoming, 3);

        let bare: Config =
            toml::from_str("[applets.next-event]\n").expect("every setting is optional");
        assert_eq!(
            bare.applets["next-event"].kind,
            AppletKind::NextEvent(applets::NextEvent::default())
        );

        assert!(
            toml::from_str::<Config>("[applets.next-event]\nwith-in = 30\n").is_err(),
            "a struct variant is what refuses a misspelled key; a unit variant would have \
             swallowed it silently"
        );
    }

    #[test]
    fn a_place_table_reads_both_shapes_and_refuses_a_third() {
        let here: Config =
            toml::from_str("[applets.weather.place]\nat = \"here\"\n").expect("`here` is a place");
        assert_eq!(
            here.applets["weather"].kind,
            AppletKind::Weather(applets::Weather::default()),
            "following the fix is the default, so naming it changes nothing"
        );

        let fixed: Config = toml::from_str(
            "[applets.weather]\nlabel = \"Vilnius\"\ndays = 7\n\n             [applets.weather.place]\nat = \"coordinates\"\nlatitude = 54.6872\n             longitude = 25.2797\n",
        )
        .expect("a fixed pair is a place");
        let AppletKind::Weather(weather) = &fixed.applets["weather"].kind else {
            panic!("the table names the weather applet");
        };
        assert_eq!(weather.label.as_deref(), Some("Vilnius"));
        assert_eq!((weather.hours, weather.days), (4, 7));
        assert_eq!(
            weather.place,
            applets::Place::Coordinates {
                latitude: 54.6872,
                longitude: 25.2797
            }
        );

        let named: Config = toml::from_str(
            "[applets.weather]\nplace = { at = \"location\", name = \"Vilnius, LT\" }\n",
        )
        .expect("a city and country code are a place");
        let AppletKind::Weather(weather) = &named.applets["weather"].kind else {
            panic!("the table names the weather applet");
        };
        assert_eq!(
            weather.place,
            applets::Place::Location {
                name: "Vilnius, LT".to_owned()
            }
        );
        let long_city = "v".repeat(101);
        for name in [
            "Vilnius".to_owned(),
            "Vilnius, lt".to_owned(),
            ", LT".to_owned(),
            "Vilnius, LTU".to_owned(),
            format!("{long_city}, LT"),
        ] {
            let document =
                format!("[applets.weather]\nplace = {{ at = \"location\", name = \"{name}\" }}\n");
            let error = toml::from_str::<Config>(&document)
                .expect_err("malformed locations are rejected")
                .to_string();
            assert!(error.contains("location must be written as"), "{error}");
        }

        assert!(
            toml::from_str::<Config>("[applets.weather.place]\nat = \"postcode\"\n").is_err(),
            "there are two shapes and a third is a mistake, not a place"
        );
        assert!(
            toml::from_str::<Config>("[applets.weather.place]\nat = \"here\"\nlatitude = 54.6\n")
                .is_err(),
            "`deny_unknown_fields` is what stops coordinates being written under the wrong shape"
        );
    }

    #[test]
    fn a_named_place_is_canonicalized_when_loaded() {
        let document: Config = toml::from_str(
            "[applets.weather]\nplace = { at = \"location\", name = \" Washington, D.C. , US \" }\n",
        )
        .expect("a named place");
        let AppletKind::Weather(weather) = &document.applets["weather"].kind else {
            panic!("the table names the weather applet");
        };

        assert_eq!(
            weather.place,
            applets::Place::Location {
                name: "Washington, D.C., US".to_owned()
            }
        );
    }

    /// The wire refuses these too, but a document saying so names the table and the key, before
    /// anything has asked the provider to watch somewhere that is not on Earth.
    #[test]
    fn coordinates_outside_their_ranges_are_refused_at_load() {
        let place = |latitude: &str, longitude: &str| {
            format!(
                "[applets.weather.place]\nat = \"coordinates\"\nlatitude = {latitude}\n                 longitude = {longitude}\n"
            )
        };

        assert!(toml::from_str::<Config>(&place("54.6872", "25.2797")).is_ok());

        for (latitude, longitude, named) in [
            ("91.0", "25.2797", "latitude"),
            ("-90.5", "25.2797", "latitude"),
            ("54.6872", "180.5", "longitude"),
            ("54.6872", "-181.0", "longitude"),
        ] {
            let error = toml::from_str::<Config>(&place(latitude, longitude))
                .expect_err("that pair is not on Earth")
                .to_string();
            assert!(
                error.contains(named) && error.contains("[applets.weather]"),
                "the message names the key and the table it is in: {error}"
            );
        }
    }

    #[test]
    fn a_common_setting_sits_beside_the_kinds_own() {
        let parsed: Config =
            toml::from_str("[applets.clock]\nlabel-format = \"%H\"\ntooltip-format = \"%A\"\n")
                .expect("common and kind settings share one flat table");

        assert_eq!(
            parsed.applets["clock"].common.tooltip_format.as_deref(),
            Some("%A")
        );
        let AppletKind::Clock(clock) = &parsed.applets["clock"].kind else {
            panic!("the table names the clock");
        };
        assert_eq!(clock.label_format, "%H");
    }

    #[test]
    fn a_misspelled_common_setting_is_still_a_load_error() {
        assert!(
            toml::from_str::<Config>("[applets.clock]\ntooltipformat = \"%A\"\n").is_err(),
            "the splitter leaves what it does not recognise in the table, so the kind refuses it"
        );
    }

    #[test]
    fn an_unknown_setting_names_the_common_ones_too() {
        let error = toml::from_str::<Config>("[applets.clock]\ntooltipformat = \"%A\"\n")
            .expect_err("a misspelled key is refused")
            .to_string();

        assert!(
            error.contains("`tooltip-format`"),
            "the kind cannot list a setting it does not own, so the message has to name the one \
             the user meant: {error}"
        );
    }

    #[test]
    fn a_settings_row_needs_a_label_and_a_command() {
        assert!(
            toml::from_str::<Config>("[applets.clock]\nsettings-label = \"Open\"\n").is_err(),
            "a label with no command is a row that does nothing"
        );
        assert!(
            toml::from_str::<Config>("[applets.clock]\nsettings-command = [\"x\"]\n").is_err(),
            "a command with no label is a row nobody can see"
        );
        let parsed: Config = toml::from_str(
            "[applets.clock]\nsettings-label = \"Open\"\nsettings-command = [\"gnome-calendar\"]\n",
        )
        .expect("both halves together");
        assert_eq!(
            parsed.applets["clock"].common.settings(),
            Some(("Open", ["gnome-calendar".to_owned()].as_slice()))
        );
    }

    #[test]
    fn the_clock_still_reads_the_key_it_used_to_call_format() {
        let parsed: Config =
            toml::from_str("[applets.clock]\nformat = \"%H\"\n").expect("the alias parses");

        let AppletKind::Clock(clock) = &parsed.applets["clock"].kind else {
            panic!("the table names the clock");
        };
        assert_eq!(clock.label_format, "%H");
    }

    #[test]
    fn a_clock_reads_its_world_clock_zones() {
        let parsed: Config = toml::from_str(
            "[applets.clock]\n[[applets.clock.timezones]]\nlabel = \"Tokyo\"\ntimezone = \"Asia/Tokyo\"\n",
        )
        .expect("a full table");

        let AppletKind::Clock(clock) = &parsed.applets["clock"].kind else {
            panic!("the table names the clock");
        };
        assert_eq!(
            clock.first_day,
            FirstDay::Monday,
            "first-day has no locale variant, so the default has to say what it is"
        );
        assert_eq!(clock.timezones.len(), 1);
        assert_eq!(clock.timezones[0].label, "Tokyo");
        assert_eq!(clock.timezones[0].note, None);
    }

    #[test]
    fn a_configured_applet_survives_being_written_back_out() {
        let parsed: Config = toml::from_str(
            "[applets.clock]\nlabel-format = \"%H\"\ntooltip-format = \"%A\"\nsettings-label = \"Open\"\nsettings-command = [\"gnome-calendar\"]\n",
        )
        .expect("a table carrying both halves");

        let written = toml::to_string(&parsed).expect("the document serializes");
        let again: Config = toml::from_str(&written).expect("what was written parses again");

        assert_eq!(
            parsed.applets, again.applets,
            "`glimpsectl config show` writes this document out, and both halves are flattened \
             into one table on the way"
        );
    }

    #[test]
    fn a_setting_a_nested_table_lacks_is_not_blamed_on_a_common_one() {
        let error = toml::from_str::<Config>(
            "[applets.clock]\n[[applets.clock.timezones]]\nlabel = \"T\"\ntimezone = \"Asia/Tokyo\"\nnonesuch = 1\n",
        )
        .expect_err("a key the timezone entry does not have")
        .to_string();
        assert!(
            !error.contains("tooltip-format"),
            "a common setting is not valid inside a timezone entry: {error}"
        );
    }

    #[test]
    fn a_settings_command_that_names_no_program_is_refused() {
        let parsed = toml::from_str::<Config>(
            "[applets.clock]\nsettings-label = \"Open\"\nsettings-command = [\"\"]\n",
        );
        assert!(parsed.is_err(), "an empty program name was accepted");
    }

    #[test]
    fn a_command_applet_carries_a_program_per_gesture() {
        let parsed = toml::from_str::<Config>(
            "[applets.shot]\nextends = \"command\"\nicon = \"camera-photo-symbolic\"\non-click = [\"grim\"]\non-scroll-up = [\"pamixer\", \"-i\", \"5\"]\n",
        )
        .expect("a command applet with programs parses");
        let AppletKind::Command(command) = &parsed.applets["shot"].kind else {
            panic!("`extends = \"command\"` names the kind");
        };
        assert_eq!(command.on_click, ["grim"]);
        assert_eq!(command.on_scroll_up, ["pamixer", "-i", "5"]);
    }

    #[test]
    fn a_command_gesture_that_names_no_program_is_refused_by_key() {
        let error = toml::from_str::<Config>(
            "[applets.shot]\nextends = \"command\"\non-scroll-up = [\" \"]\n",
        )
        .expect_err("an empty program name was accepted")
        .to_string();
        assert!(
            error.contains("[applets.shot]") && error.contains("on-scroll-up"),
            "the error names the table and the key: {error}"
        );
    }

    #[test]
    fn a_command_icon_path_must_be_absolute() {
        let relative = "[applets.shot]\nextends = \"command\"\nicon = \"icons/shot.png\"\n";
        let absolute = "[applets.shot]\nextends = \"command\"\nicon = \"/usr/share/shot.png\"\n";
        assert!(toml::from_str::<Config>(relative).is_err());
        assert!(toml::from_str::<Config>(absolute).is_ok());
    }

    #[test]
    fn a_command_applet_refuses_a_key_it_does_not_have() {
        assert!(
            toml::from_str::<Config>("[applets.shot]\nextends = \"command\"\non-clik = [\"x\"]\n")
                .is_err(),
            "a unit variant would swallow the misspelling"
        );
    }

    #[test]
    fn the_schema_offers_every_applet_by_its_table_name() {
        let schema: serde_json::Value =
            serde_json::from_str(&crate::json_schema_document()).expect("the schema is JSON");
        let applets = &schema["properties"]["applets"];
        let by_name = applets["properties"]
            .as_object()
            .expect("one entry per applet, so an editor resolves [applets.clock] on its own");

        assert_eq!(
            by_name["clock"]["properties"]["label-format"]["default"],
            serde_json::json!("%a, %-d %b, %H:%M"),
            "a by-name entry carries the applet's own settings"
        );
        assert_eq!(
            by_name["clock"]["required"],
            serde_json::json!([]),
            "`extends` is not required when the table name already names the applet"
        );
        assert_eq!(
            applets["additionalProperties"]["oneOf"]
                .as_array()
                .map(Vec::len),
            Some(by_name.len()),
            "an aliased table falls through to the tagged form"
        );
    }

    #[test]
    fn extends_names_the_applet_so_one_kind_can_have_several_instances() {
        let parsed: Config = toml::from_str("[applets.clock-utc]\nextends = \"clock\"\n")
            .expect("extends wins over the key");

        assert_eq!(
            parsed.applets["clock-utc"],
            Applet::from(AppletKind::Clock(applets::Clock::default()))
        );
    }

    #[test]
    fn the_pager_defaults_to_dots_of_workspaces_on_this_output() {
        let parsed: Config = toml::from_str("[applets.pager]\n").expect("settings are optional");

        assert_eq!(
            parsed.applets["pager"],
            Applet::from(AppletKind::Pager(applets::Pager {
                mode: PagerMode::Workspaces,
                shape: PagerShape::Dots,
                scope: PagerScope::Output,
                label: "{index}".to_owned(),
                focused_label: None,
                unfocused_label: None,
                urgent_label: None,
            })),
            "the old applet defaulted to windows, which is the surprising one"
        );
    }

    #[test]
    fn the_pager_reads_its_settings_in_kebab_case() {
        let parsed: Config = toml::from_str(
            "[applets.pager]\nmode = \"windows\"\nshape = \"labels\"\nscope = \"session\"\nfocused-label = \"{name}\"\n",
        )
        .expect("a full table");

        let AppletKind::Pager(pager) = &parsed.applets["pager"].kind else {
            panic!("the table names the pager");
        };
        assert_eq!(pager.mode, PagerMode::Windows);
        assert_eq!(pager.shape, PagerShape::Labels);
        assert_eq!(pager.scope, PagerScope::Session);
        assert_eq!(pager.focused_label.as_deref(), Some("{name}"));
    }

    #[test]
    fn the_pager_refuses_a_setting_it_does_not_have() {
        assert!(
            toml::from_str::<Config>("[applets.pager]\nappearance = \"dots\"\n").is_err(),
            "`appearance` was the old name for `shape`; a silently ignored key is a setting that \
             never took effect"
        );
    }

    #[test]
    fn the_workspace_and_window_applets_are_gone_rather_than_unimplemented() {
        for name in ["workspace", "window"] {
            assert!(
                toml::from_str::<Config>(&format!("[applets.{name}]\n")).is_err(),
                "`{name}` folded into the pager as a mode and a scope; leaving the name resolvable \
                 would leave a second way to ask for the same strip"
            );
        }
    }

    #[test]
    fn an_unresolvable_table_name_is_refused_and_lists_the_applets() {
        let error = toml::from_str::<Config>("[applets.nonesuch]\n")
            .expect_err("an applet nobody implements is a bad document, not a silent skip")
            .to_string();

        assert!(error.contains("nonesuch"), "{error}");
        assert!(
            error.contains("clock"),
            "the message lists what is valid: {error}"
        );
    }

    #[test]
    fn a_setting_no_applet_declares_is_refused() {
        let error = toml::from_str::<Config>("[applets.clock]\nfrmat = \"%H\"\n")
            .expect_err("a typo is loud rather than ignored")
            .to_string();

        assert!(error.contains("frmat"), "{error}");
        assert!(
            error.contains("applets.clock"),
            "it names the table: {error}"
        );
    }
}
