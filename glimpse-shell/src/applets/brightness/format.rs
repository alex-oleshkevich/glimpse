use glimpse_core::{
    compositors::Monitor,
    services::brightness::{BrightnessSource, State},
};

pub const DEFAULT_LABEL_FORMAT: &str = "";
pub const DEFAULT_TOOLTIP_FORMAT: &str = "{source}: {percent}%";
pub const ICON_NAME: &str = "display-brightness-symbolic";

pub fn label(format: &str, state: &State) -> String {
    render(format, primary_source(state))
}

pub fn label_with_monitors(format: &str, state: &State, monitors: &[Monitor]) -> String {
    if monitors.is_empty() {
        return label(format, state);
    }

    render_with_monitors(format, primary_source(state), monitors)
}

pub fn tooltip(format: &str, state: &State) -> String {
    render(format, primary_source(state))
}

pub fn tooltip_with_monitors(format: &str, state: &State, monitors: &[Monitor]) -> String {
    if monitors.is_empty() {
        return tooltip(format, state);
    }

    render_with_monitors(format, primary_source(state), monitors)
}

pub fn hero_subtitle(state: &State) -> String {
    primary_source(state)
        .map(|source| source.name.clone())
        .unwrap_or_else(|| "No brightness controls".into())
}

pub fn hero_subtitle_with_monitors(state: &State, monitors: &[Monitor]) -> String {
    if monitors.is_empty() {
        return hero_subtitle(state);
    }

    primary_source(state)
        .map(|source| source_display_name(source, monitors))
        .unwrap_or_else(|| "No brightness controls".into())
}

pub fn icon_name(_state: &State) -> &str {
    ICON_NAME
}

fn render(format: &str, source: Option<&BrightnessSource>) -> String {
    let source_name = source.map(|source| source.name.as_str());
    render_with_source_name(format, source, source_name)
}

fn render_with_monitors(
    format: &str,
    source: Option<&BrightnessSource>,
    monitors: &[Monitor],
) -> String {
    let source_name = source.map(|source| source_display_name(source, monitors));
    render_with_source_name(format, source, source_name.as_deref())
}

fn render_with_source_name(
    format: &str,
    source: Option<&BrightnessSource>,
    source_name: Option<&str>,
) -> String {
    if format.is_empty() {
        return String::new();
    }

    let source_name = source_name.unwrap_or("Brightness");
    let percent = source
        .map(|source| source.percent.to_string())
        .unwrap_or_else(|| "0".into());

    format
        .replace("{source}", source_name)
        .replace("{percent}", &percent)
}

pub fn primary_source(state: &State) -> Option<&BrightnessSource> {
    state
        .sources
        .iter()
        .find(|source| source.primary && source.available && source.writable)
        .or_else(|| {
            state
                .sources
                .iter()
                .find(|source| source.available && source.writable)
        })
}

pub fn source_display_name(source: &BrightnessSource, monitors: &[Monitor]) -> String {
    source
        .connector
        .as_deref()
        .and_then(|connector| {
            monitors
                .iter()
                .find(|monitor| monitor.name.eq_ignore_ascii_case(connector))
        })
        .map(monitor_display_name)
        .unwrap_or_else(|| source.name.clone())
}

pub fn monitor_display_name(monitor: &Monitor) -> String {
    if monitor.built_in {
        return "Built-in display".into();
    }

    if let Some(label) = make_model_label(monitor) {
        return label;
    }

    monitor
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&monitor.name)
        .to_owned()
}

fn make_model_label(monitor: &Monitor) -> Option<String> {
    let make = monitor.make.as_deref().map(str::trim).unwrap_or_default();
    let model = monitor.model.as_deref().map(str::trim).unwrap_or_default();
    if make.is_empty() && model.is_empty() {
        return None;
    }
    if make.is_empty() {
        return Some(model.to_owned());
    }
    if model.is_empty() {
        return Some(make.to_owned());
    }
    if model
        .to_ascii_lowercase()
        .starts_with(&make.to_ascii_lowercase())
    {
        return Some(model.to_owned());
    }
    Some(format!("{make} {model}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::compositors::Monitor;
    use glimpse_core::services::brightness::BrightnessSourceKind;

    #[test]
    fn default_label_is_empty() {
        assert_eq!(DEFAULT_LABEL_FORMAT, "");
    }

    #[test]
    fn tooltip_uses_primary_source() {
        let state = State {
            available: true,
            sources: vec![BrightnessSource {
                id: "backlight:intel_backlight".into(),
                name: "Intel backlight".into(),
                connector: None,
                kind: BrightnessSourceKind::BuiltInDisplay,
                icon: "display-brightness-symbolic".into(),
                current: 50,
                max: 100,
                percent: 50,
                writable: true,
                primary: true,
                available: true,
            }],
            active: None,
        };

        assert_eq!(
            tooltip(DEFAULT_TOOLTIP_FORMAT, &state),
            "Intel backlight: 50%"
        );
    }

    #[test]
    fn tooltip_with_monitors_uses_matching_monitor_name() {
        let state = State {
            available: true,
            sources: vec![source_on_connector(
                "ddcutil:1",
                "Raw DDC display",
                BrightnessSourceKind::ExternalDisplay,
                50,
                "DP-2",
            )],
            active: None,
        };
        let mut monitor = monitor("DP-2", true, false);
        monitor.make = Some("Dell Inc.".into());
        monitor.model = Some("AW2725Q".into());

        assert_eq!(
            tooltip_with_monitors(DEFAULT_TOOLTIP_FORMAT, &state, &[monitor]),
            "Dell Inc. AW2725Q: 50%"
        );
    }

    #[test]
    fn hero_subtitle_never_includes_percent() {
        let state = State {
            available: true,
            sources: vec![BrightnessSource {
                id: "backlight:intel_backlight".into(),
                name: "Built-in display".into(),
                connector: None,
                kind: BrightnessSourceKind::BuiltInDisplay,
                icon: "input-keyboard-symbolic".into(),
                current: 50,
                max: 100,
                percent: 50,
                writable: true,
                primary: true,
                available: true,
            }],
            active: None,
        };

        assert_eq!(hero_subtitle(&state), "Built-in display");
        assert!(!hero_subtitle(&state).contains('%'));
    }

    #[test]
    fn icon_name_is_always_brightness_icon() {
        let state = State {
            available: true,
            sources: vec![BrightnessSource {
                id: "keyboard:upower".into(),
                name: "Keyboard backlight".into(),
                connector: None,
                kind: BrightnessSourceKind::Keyboard,
                icon: "input-keyboard-symbolic".into(),
                current: 1,
                max: 3,
                percent: 33,
                writable: true,
                primary: true,
                available: true,
            }],
            active: None,
        };

        assert_eq!(icon_name(&state), ICON_NAME);
    }

    #[test]
    fn monitor_display_name_prefers_builtin_then_make_model_then_connector() {
        let mut external = monitor("DP-2", true, false);
        external.make = Some("Dell Inc.".into());
        external.model = Some("AW2725Q".into());

        assert_eq!(
            monitor_display_name(&monitor("eDP-1", true, true)),
            "Built-in display"
        );
        assert_eq!(monitor_display_name(&external), "Dell Inc. AW2725Q");
        assert_eq!(
            monitor_display_name(&monitor("HDMI-A-1", true, false)),
            "HDMI-A-1"
        );
    }

    #[test]
    fn monitor_display_name_deduplicates_make_when_model_already_contains_it() {
        let mut external = monitor("DP-2", true, false);
        external.make = Some("Dell Inc.".into());
        external.model = Some("Dell Inc. U2723QE".into());

        assert_eq!(monitor_display_name(&external), "Dell Inc. U2723QE");
    }

    #[test]
    fn source_display_name_uses_matching_monitor_name() {
        let mut external = monitor("DP-2", true, false);
        external.make = Some("Dell Inc.".into());
        external.model = Some("AW2725Q".into());
        let source = source_on_connector(
            "ddcutil:1",
            "Raw DDC display",
            BrightnessSourceKind::ExternalDisplay,
            50,
            "DP-2",
        );

        assert_eq!(
            source_display_name(&source, &[external]),
            "Dell Inc. AW2725Q"
        );
    }

    fn source_on_connector(
        id: &str,
        name: &str,
        kind: BrightnessSourceKind,
        percent: u8,
        connector: &str,
    ) -> BrightnessSource {
        BrightnessSource {
            id: id.into(),
            name: name.into(),
            connector: Some(connector.into()),
            kind,
            icon: "display-brightness-symbolic".into(),
            current: percent.into(),
            max: 100,
            percent,
            writable: true,
            primary: true,
            available: true,
        }
    }

    fn monitor(name: &str, enabled: bool, built_in: bool) -> Monitor {
        Monitor {
            id: None,
            name: name.into(),
            description: None,
            active_workspace: None,
            focused: false,
            make: None,
            model: None,
            enabled,
            built_in,
            current_mode: None,
        }
    }
}
