mod appearance;
mod applets;
mod backdrop;
mod calendar;
mod geolocation;
mod idle;
mod keyboard;
mod lock;
mod monitors;
mod night_light;
mod panels;
mod power;
mod wallpaper;

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use appearance::{Appearance, ColorScheme};
pub use applets::{
    Applet, Clock as ClockConfig, Pager as PagerConfig, PagerMode, PagerScope, PagerShape,
};
pub use backdrop::Backdrop;
pub use calendar::{Calendar, Source as CalendarSource, SourceKind as CalendarSourceKind};
pub use geolocation::Geolocation;
pub use idle::{Idle, Listener as IdleListener, Profile as IdleProfile, Profiles as IdleProfiles};
pub use keyboard::{Keyboard, Remember};
pub use lock::{Button as LockButton, Clock as LockClock, Controls as LockControls, Lock};
pub use monitors::Monitors;
pub use night_light::{NightLight, Schedule};
pub use panels::{Margin, Panel, Position};
pub use power::Power;
pub use wallpaper::{Fit, Wallpaper};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    pub appearance: Appearance,
    pub monitors: Monitors,
    pub geolocation: Geolocation,
    pub night_light: NightLight,
    pub idle: Idle,
    pub power: Power,
    pub keyboard: Keyboard,
    pub calendar: Calendar,
    pub wallpaper: Wallpaper,
    pub backdrop: Backdrop,
    pub lock: Lock,
    pub panels: Vec<Panel>,
    #[serde(deserialize_with = "applets::deserialize")]
    #[schemars(schema_with = "applets::schema")]
    pub applets: BTreeMap<String, Applet>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            monitors: Monitors::default(),
            geolocation: Geolocation::default(),
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
            Applet::Clock(applets::Clock::default())
        );
    }

    #[test]
    fn an_applet_table_with_no_settings_takes_the_defaults() {
        let parsed: Config = toml::from_str("[applets.clock]\n").expect("settings are optional");

        assert_eq!(
            parsed.applets["clock"],
            Applet::Clock(applets::Clock::default())
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
            by_name["clock"]["properties"]["format"]["default"],
            serde_json::json!("%H:%M"),
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
            Applet::Clock(applets::Clock::default())
        );
    }

    #[test]
    fn the_pager_defaults_to_dots_of_workspaces_on_this_output() {
        let parsed: Config = toml::from_str("[applets.pager]\n").expect("settings are optional");

        assert_eq!(
            parsed.applets["pager"],
            Applet::Pager(applets::Pager {
                mode: PagerMode::Workspaces,
                shape: PagerShape::Dots,
                scope: PagerScope::Output,
                label: "{index}".to_owned(),
                focused_label: "{index}".to_owned(),
            }),
            "the old applet defaulted to windows, which is the surprising one"
        );
    }

    #[test]
    fn the_pager_reads_its_settings_in_kebab_case() {
        let parsed: Config = toml::from_str(
            "[applets.pager]\nmode = \"windows\"\nshape = \"numbers\"\nscope = \"session\"\nfocused-label = \"{name}\"\n",
        )
        .expect("a full table");

        let Applet::Pager(pager) = &parsed.applets["pager"] else {
            panic!("the table names the pager");
        };
        assert_eq!(pager.mode, PagerMode::Windows);
        assert_eq!(pager.shape, PagerShape::Numbers);
        assert_eq!(pager.scope, PagerScope::Session);
        assert_eq!(pager.focused_label, "{name}");
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
