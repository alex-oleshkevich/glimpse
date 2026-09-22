use glimpse_services::OutputInfo;

pub const ICON: &str = "video-display-symbolic";
pub const NAME_CAP: usize = 32;

pub fn chip(count: usize) -> Option<&'static str> {
    (count > 0).then_some(ICON)
}

pub fn heading(output: &OutputInfo) -> String {
    let heading = match output.label.as_deref() {
        Some(label) if !label.is_empty() => [output.connector.as_str(), label].join(" · "),
        _ => output.connector.clone(),
    };
    glimpse_utils::clean(&heading, NAME_CAP)
}

pub fn tooltip(outputs: &[OutputInfo]) -> Option<String> {
    let named = outputs
        .iter()
        .find(|output| output.focused)
        .or_else(|| outputs.first())?;
    Some(heading(named))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(connector: &str, label: Option<&str>, focused: bool) -> OutputInfo {
        OutputInfo {
            connector: connector.to_owned(),
            label: label.map(str::to_owned),
            built_in: false,
            focused,
            make: None,
            model: None,
            serial: None,
            current_mode: None,
            logical: None,
            enabled: true,
        }
    }

    #[test]
    fn the_chip_is_gone_with_no_outputs() {
        assert_eq!(chip(0), None, "AC-13");
    }

    #[test]
    fn every_output_count_gets_the_same_single_display_icon() {
        assert_eq!(chip(1), Some(ICON), "AC-12");
        assert_eq!(
            chip(3),
            Some(ICON),
            "the chip does not change glyph with the output count"
        );
    }

    #[test]
    fn the_heading_joins_the_connector_and_the_label() {
        assert_eq!(
            heading(&output("DP-2", Some("Dell U2720Q"), false)),
            "DP-2 · Dell U2720Q"
        );
    }

    #[test]
    fn a_hostile_compositor_supplied_label_is_capped_before_it_reaches_a_tooltip() {
        let long = "Дисплей ".repeat(20);
        let shown = heading(&output("DP-2", Some(&long), false));
        assert!(
            shown.chars().count() <= NAME_CAP + 2,
            "clean() can push a pending separator and the next character in the same step that \
             crosses the cap, so its worst case is two past it rather than one"
        );
        assert!(shown.ends_with('…'));
    }

    #[test]
    fn the_heading_is_the_bare_connector_without_a_label() {
        assert_eq!(heading(&output("eDP-1", None, false)), "eDP-1");
    }

    #[test]
    fn the_tooltip_names_the_focused_output_among_several() {
        let outputs = vec![
            output("eDP-1", Some("Built-in"), false),
            output("DP-2", Some("Dell U2720Q"), true),
        ];
        assert_eq!(
            tooltip(&outputs).as_deref(),
            Some("DP-2 · Dell U2720Q"),
            "AC-12"
        );
    }

    #[test]
    fn the_tooltip_falls_back_to_the_first_output_without_a_focused_one() {
        let outputs = vec![output("eDP-1", None, false)];
        assert_eq!(tooltip(&outputs).as_deref(), Some("eDP-1"));
    }

    #[test]
    fn the_tooltip_is_none_with_no_outputs() {
        assert_eq!(tooltip(&[]), None);
    }
}
