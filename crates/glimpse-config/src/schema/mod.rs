mod appearance;
mod applets;
mod backdrop;
mod calendar;
mod idle;
mod keyboard;
mod location;
mod lock;
mod monitors;
mod night_light;
mod panels;
mod power;
mod wallpaper;

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use appearance::{Appearance, Scheme};
pub use applets::{Applet, Kind as AppletKind};
pub use backdrop::Backdrop;
pub use calendar::{Calendar, Source as CalendarSource, SourceKind as CalendarSourceKind};
pub use idle::{Idle, Listener as IdleListener, Profile as IdleProfile, Profiles as IdleProfiles};
pub use keyboard::{Keyboard, Remember};
pub use location::{Location, Provider};
pub use lock::{
    Background as LockBackground, Button as LockButton, Clock as LockClock,
    Controls as LockControls, Lock,
};
pub use monitors::Monitors;
pub use night_light::{NightLight, Schedule};
pub use panels::{DYNAMIC, Margin, Panel, Position};
pub use power::Power;
pub use wallpaper::{Fit, Wallpaper};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub appearance: Appearance,
    pub monitors: Monitors,
    pub location: Location,
    pub night_light: NightLight,
    pub idle: Idle,
    pub power: Power,
    pub keyboard: Keyboard,
    pub calendar: Calendar,
    pub wallpaper: Wallpaper,
    pub backdrop: Backdrop,
    pub lock: Lock,
    pub panels: Vec<Panel>,
    pub applets: BTreeMap<String, Applet>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            monitors: Monitors::default(),
            location: Location::default(),
            night_light: NightLight::default(),
            idle: Idle::default(),
            power: Power::default(),
            keyboard: Keyboard::default(),
            calendar: Calendar::default(),
            wallpaper: Wallpaper::default(),
            backdrop: Backdrop::default(),
            lock: Lock::default(),
            panels: vec![Panel::default()],
            applets: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reference_file_matches_the_compiled_in_defaults() {
        let checked_in = include_str!("../../../../data/config.default.toml");

        assert_eq!(checked_in, crate::default_document());
    }

    #[test]
    fn the_json_schema_matches_the_compiled_in_types() {
        let checked_in = include_str!("../../../../data/config.schema.json");

        assert_eq!(checked_in, crate::json_schema_document());
    }

    #[test]
    fn night_light_still_accepts_manual_for_schedule() {
        let parsed: Config =
            toml::from_str("[night_light]\nschedule = \"manual\"\n").expect("the alias parses");

        assert_eq!(parsed.night_light.schedule, Schedule::Schedule);
    }

    #[test]
    fn applet_settings_are_flat_alongside_extends() {
        let parsed: Config = toml::from_str(
            "[applets.clock]\nextends = \"clock\"\ntimezones = [\"UTC\"]\nanything = 1\n",
        )
        .expect("free-form settings");

        assert_eq!(parsed.applets["clock"].extends, Some(AppletKind::Clock));
        assert_eq!(
            parsed.applets["clock"].settings["timezones"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            parsed.applets["clock"].settings["anything"].as_integer(),
            Some(1)
        );
    }

    #[test]
    fn extends_defaults_to_none_when_absent() {
        let parsed: Config = toml::from_str("[applets.clock]\ntimezones = [\"UTC\"]\n")
            .expect("extends is optional");

        assert_eq!(parsed.applets["clock"].extends, None);
        assert_eq!(
            parsed.applets["clock"].settings["timezones"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn an_unknown_extends_value_is_still_loud() {
        toml::from_str::<Config>("[applets.clock]\nextends = \"not_a_type\"\n")
            .expect_err("extends, when given, is still checked against Kind");
    }
}
