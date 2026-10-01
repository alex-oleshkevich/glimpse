use super::*;

fn output(connector: &str, built_in: bool) -> OutputInfo {
    OutputInfo {
        connector: connector.to_owned(),
        label: None,
        built_in,
        focused: false,
        make: None,
        model: None,
        serial: None,
        current_mode: None,
        logical: None,
        enabled: true,
    }
}

#[test]
fn the_widget_display_carries_whether_the_output_is_built_in() {
    assert!(to_widget_display(&output("eDP-1", true)).built_in);
    assert!(!to_widget_display(&output("DP-2", false)).built_in);
}
