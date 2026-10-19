use super::*;

#[test]
fn a_title_keeps_its_text_and_loses_its_bidi_overrides() {
    let hostile = "\u{200e}\u{2068}Jenny\u{2069} @ \u{2068}Sam\u{2069}";
    assert_eq!(sanitize(hostile, TITLE_CAP), "\u{200e}Jenny @ Sam");
}

#[test]
fn a_title_loses_control_characters_that_would_break_a_log_line() {
    assert_eq!(sanitize("a\nb\tc\u{0}d", TITLE_CAP), "abcd");
}

#[test]
fn an_overlong_title_truncates_on_a_char_boundary() {
    let long = "\u{1f600}".repeat(TITLE_CAP + 10);
    let capped = sanitize(&long, TITLE_CAP);

    assert_eq!(capped.chars().count(), TITLE_CAP);
    assert_eq!(capped.len(), TITLE_CAP * 4);
}

#[test]
fn an_app_id_is_capped_shorter_than_a_title() {
    let long = "x".repeat(1000);

    assert_eq!(sanitize(&long, APP_ID_CAP).len(), APP_ID_CAP);
    assert_eq!(sanitize(&long, TITLE_CAP).len(), TITLE_CAP);
}

#[test]
fn a_workspace_name_is_capped_and_sanitized_like_a_window_title() {
    assert_eq!(capped_name_str("a\nb"), Some("ab".to_owned()));
    assert_eq!(capped_name_str(""), None);
    assert_eq!(
        capped_name_str(&"x".repeat(NAME_CAP + 10)),
        Some("x".repeat(NAME_CAP)),
        "a workspace name is set by whatever ran renameworkspace, not only by hand"
    );
}

#[test]
fn an_edid_string_is_sanitized_before_it_is_trimmed() {
    assert_eq!(
        capped_edid_str("AW2725Q \u{1}"),
        Some("AW2725Q".to_owned()),
        "trimming first would leave the control character in place to defeat it"
    );
    assert_eq!(capped_edid_str("\u{202a} \u{202a}"), None);
}

#[test]
fn internal_panels_are_recognized_by_connector() {
    assert!(is_built_in("eDP-1"));
    assert!(is_built_in("LVDS-1"));
    assert!(!is_built_in("DP-3"));
    assert!(!is_built_in("HDMI-A-1"));
}

#[test]
fn a_tiled_window_has_no_rect_and_a_floating_window_does() {
    let tiled: Window = serde_json::from_value(serde_json::json!({
        "id": 374, "is_floating": false,
        "layout": { "pos_in_scrolling_layout": [4, 1], "window_size": [1018, 1644],
                    "tile_pos_in_workspace_view": null, "window_offset_in_tile": [0.0, 0.0] }
    }))
    .expect("tiled window");

    assert_eq!(tiled.size, Some((1018, 1644)));
    assert_eq!(tiled.rect, None);

    let floating: Window = serde_json::from_value(serde_json::json!({
        "id": 435, "is_floating": true,
        "layout": { "pos_in_scrolling_layout": null, "window_size": [1807, 924],
                    "tile_pos_in_workspace_view": [259.2, 189.6],
                    "window_offset_in_tile": [3.0, -2.0] }
    }))
    .expect("floating window");

    assert_eq!(floating.size, Some((1807, 924)));
    assert_eq!(
        floating.rect,
        Some(Rect {
            x: 262.2,
            y: 187.6,
            width: 1807.0,
            height: 924.0
        })
    );
}

#[test]
fn a_window_with_no_layout_at_all_decodes_with_neither_size_nor_rect() {
    let window: Window =
        serde_json::from_value(serde_json::json!({ "id": 1, "is_focused": false }))
            .expect("window with no layout key");

    assert_eq!(window.size, None);
    assert_eq!(window.rect, None);
    assert_eq!(window.layout_order, None);
}

#[test]
fn a_negative_window_size_drops_the_geometry_but_not_the_window() {
    let window: Window = serde_json::from_value(serde_json::json!({
        "id": 1,
        "layout": { "window_size": [-1, 924], "tile_pos_in_workspace_view": [10.0, 20.0] }
    }))
    .expect("a negative size still decodes the window");

    assert_eq!(window.size, None);
    assert_eq!(window.rect, None);
}

#[test]
fn a_window_s_output_comes_from_its_workspace() {
    let mut windows = vec![Window {
        id: WindowId(9),
        title: None,
        app_id: None,
        pid: None,
        workspace_id: Some(WorkspaceId(5)),
        is_focused: false,
        is_floating: false,
        is_urgent: false,
        layout_order: None,
        size: None,
        rect: None,
        output: None,
    }];
    let workspaces = vec![Workspace {
        id: WorkspaceId(5),
        idx: None,
        name: None,
        output: Some("eDP-1".to_owned()),
        is_active: false,
        is_focused: false,
        is_urgent: false,
        active_window_id: None,
    }];

    fill_window_outputs(&mut windows, &workspaces);

    assert_eq!(windows[0].output.as_deref(), Some("eDP-1"));
}

#[test]
fn a_window_deserialized_directly_still_caps_its_title() {
    let long = "x".repeat(5000);
    let window: Window =
        serde_json::from_value(serde_json::json!({ "id": 1, "title": long })).expect("window");

    assert_eq!(
        window.title.map(|title| title.chars().count()),
        Some(TITLE_CAP)
    );
}
