use glimpse_core::compositors::{Monitor, MonitorMode};

pub fn row_label(monitor: &Monitor) -> String {
    monitor
        .model
        .as_deref()
        .filter(|m| !m.is_empty())
        .unwrap_or(monitor.name.as_str())
        .to_string()
}

pub fn row_sublabel(monitor: &Monitor) -> String {
    match monitor.current_mode {
        Some(mode) => format!(
            "{} \u{00b7} {}\u{00d7}{} @ {} Hz",
            monitor.name,
            mode.width,
            mode.height,
            refresh_hz(&mode)
        ),
        None => monitor.name.clone(),
    }
}

pub fn row_tooltip(monitor: &Monitor) -> String {
    match monitor.current_mode {
        Some(mode) => format!(
            "{} \u{00b7} {}\u{00d7}{} @ {} Hz",
            monitor.name,
            mode.width,
            mode.height,
            refresh_hz(&mode)
        ),
        None => monitor.name.clone(),
    }
}

pub fn refresh_hz(mode: &MonitorMode) -> u32 {
    (mode.refresh_mhz as f64 / 1000.0).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_monitor(name: &str, model: Option<&str>, mode: Option<MonitorMode>) -> Monitor {
        Monitor {
            id: Some(1),
            name: name.to_string(),
            description: None,
            active_workspace: None,
            focused: false,
            make: None,
            model: model.map(str::to_string),
            enabled: true,
            built_in: false,
            current_mode: mode,
        }
    }

    #[test]
    fn row_label_prefers_model_over_connector() {
        let monitor = mk_monitor("DP-1", Some("Dell U2720Q"), None);
        assert_eq!(row_label(&monitor), "Dell U2720Q");
    }

    #[test]
    fn row_label_falls_back_to_connector_when_no_model() {
        let monitor = mk_monitor("HDMI-A-1", None, None);
        assert_eq!(row_label(&monitor), "HDMI-A-1");
    }

    #[test]
    fn row_tooltip_includes_mode_when_present() {
        let monitor = mk_monitor(
            "DP-1",
            Some("Dell"),
            Some(MonitorMode {
                width: 3840,
                height: 2160,
                refresh_mhz: 59997,
            }),
        );
        assert_eq!(
            row_tooltip(&monitor),
            "DP-1 \u{00b7} 3840\u{00d7}2160 @ 60 Hz"
        );
    }

    #[test]
    fn row_tooltip_omits_mode_when_none() {
        let monitor = mk_monitor("eDP-1", Some("Internal"), None);
        assert_eq!(row_tooltip(&monitor), "eDP-1");
    }

    #[test]
    fn refresh_hz_rounds_to_nearest_integer() {
        assert_eq!(
            refresh_hz(&MonitorMode {
                width: 1920,
                height: 1080,
                refresh_mhz: 59997,
            }),
            60
        );
        assert_eq!(
            refresh_hz(&MonitorMode {
                width: 2560,
                height: 1440,
                refresh_mhz: 144000,
            }),
            144
        );
    }
}
