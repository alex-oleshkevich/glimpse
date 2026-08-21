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

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
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
    fn night_light_still_accepts_manual_for_schedule() {
        let parsed: Config =
            toml::from_str("[night_light]\nschedule = \"manual\"\n").expect("the alias parses");

        assert_eq!(parsed.night_light.schedule, Schedule::Schedule);
    }

    #[test]
    fn applet_settings_take_any_key_but_the_applet_itself_does_not() {
        let parsed: Config = toml::from_str(
            "[applets.clock]\nextends = \"clock\"\n[applets.clock.settings]\nanything = 1\n",
        )
        .expect("free-form settings");

        assert_eq!(parsed.applets["clock"].extends, AppletKind::Clock);
        assert_eq!(
            parsed.applets["clock"].settings["anything"].as_integer(),
            Some(1)
        );

        toml::from_str::<Config>("[applets.clock]\nextends = \"clock\"\ntypo = 1\n")
            .expect_err("a typo beside `settings` is still loud");
    }
}
