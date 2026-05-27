use glimpse_core::compositors::Monitor;

pub const DEFAULT_TOOLTIP_FORMAT: &str = "{active}/{total} monitors";

pub fn hero_subtitle(monitors: &[Monitor]) -> String {
    let total = monitors.len();
    if total == 0 {
        return "No displays".into();
    }
    let active = monitors.iter().filter(|m| m.enabled).count();
    format!("{active} of {total} monitors")
}

pub fn tooltip(format: &str, monitors: &[Monitor]) -> String {
    if format.is_empty() {
        return String::new();
    }
    let total = monitors.len();
    let active = monitors.iter().filter(|m| m.enabled).count();
    format
        .replace("{active}", &active.to_string())
        .replace("{total}", &total.to_string())
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

    #[test]
    fn hero_subtitle_shows_active_of_total() {
        let monitors = vec![monitor("eDP-1", true, true), monitor("DP-2", false, false)];
        assert_eq!(hero_subtitle(&monitors), "1 of 2 monitors");
    }

    #[test]
    fn hero_subtitle_is_no_displays_when_empty() {
        assert_eq!(hero_subtitle(&[]), "No displays");
    }

    #[test]
    fn tooltip_replaces_active_and_total() {
        let monitors = vec![monitor("eDP-1", true, true), monitor("DP-2", true, false)];
        assert_eq!(
            tooltip("{active}/{total} monitors", &monitors),
            "2/2 monitors"
        );
    }

    #[test]
    fn tooltip_is_empty_when_format_is_empty() {
        assert_eq!(tooltip("", &[monitor("eDP-1", true, true)]), "");
    }

    #[test]
    fn monitor_display_name_prefers_builtin_then_make_model_then_name() {
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
    fn monitor_display_name_deduplicates_make_in_model() {
        let mut external = monitor("DP-2", true, false);
        external.make = Some("Dell Inc.".into());
        external.model = Some("Dell Inc. U2723QE".into());
        assert_eq!(monitor_display_name(&external), "Dell Inc. U2723QE");
    }
}
