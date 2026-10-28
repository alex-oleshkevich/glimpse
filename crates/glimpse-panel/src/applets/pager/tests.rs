use super::*;

fn workspace(id: u64, output: &str, focused: bool, active: bool) -> WorkspaceInfo {
    WorkspaceInfo {
        id,
        index: Some(id as u8),
        name: None,
        output: Some(output.to_owned()),
        active,
        focused,
        urgent: false,
        windows: 0,
    }
}

fn window(id: u64, focused: bool) -> WindowInfo {
    WindowInfo {
        id,
        title: Some("a terminal".to_owned()),
        app_id: Some("ghostty".to_owned()),
        workspace: Some(4),
        focused,
        floating: false,
        urgent: false,
        order: Some(1),
    }
}

fn labelled(label: &str, focused_label: &str) -> PagerConfig {
    PagerConfig {
        label: label.to_owned(),
        focused_label: Some(focused_label.to_owned()),
        ..PagerConfig::default()
    }
}

#[test]
fn a_window_numbers_itself_by_position_rather_than_by_its_address() {
    let slot = window_slot(
        &labelled("w{index}", "w{index}"),
        1,
        &window(94_388_234_684_768, false),
        Some("work"),
    );

    assert_eq!(
        slot.label, "w2",
        "a window has no compositor index, and under Hyprland its id is a memory address"
    );
    assert_eq!(slot.tooltip, "a terminal");
    assert_eq!(slot.id, 94_388_234_684_768);
}

#[test]
fn an_unnamed_workspace_is_named_by_its_index() {
    let mut unnamed = workspace(7, "DP-1", false, false);
    unnamed.index = Some(4);
    unnamed.name = None;

    assert_eq!(workspace_token(&unnamed), "4");

    unnamed.name = Some(String::new());
    assert_eq!(
        workspace_token(&unnamed),
        "4",
        "a workspace renamed to nothing is unnamed, not named the empty string"
    );

    unnamed.name = Some("work".to_owned());
    assert_eq!(workspace_token(&unnamed), "work");
}

#[test]
fn a_workspace_slot_renders_the_index_where_it_has_no_name() {
    let settings = PagerConfig {
        label: "{workspace-name}".to_owned(),
        ..PagerConfig::default()
    };
    let mut unnamed = workspace(7, "DP-1", false, false);
    unnamed.index = Some(4);

    assert_eq!(
        workspace_slot(&settings, &unnamed).label,
        "4",
        "an empty slot reads as a broken template, not as an unnamed workspace"
    );
}

#[test]
fn a_window_slot_can_still_name_the_workspace_it_sits_on() {
    let slot = window_slot(
        &labelled("{workspace-name}", "{workspace-name}"),
        0,
        &window(94_388_234_684_768, false),
        Some("work"),
    );

    assert_eq!(
        slot.label, "work",
        "windows mode makes the name token the window's app id, so without this one there is \
             no way to show which workspace the strip is showing"
    );
}

#[test]
fn the_focused_slot_reads_its_own_template() {
    let settings = labelled("{index}", "{name-or-index}");
    let mut named = workspace(2, "DP-1", false, false);
    named.name = Some("chat".to_owned());

    assert_eq!(workspace_slot(&settings, &named).label, "2");

    named.focused = true;
    assert_eq!(
        workspace_slot(&settings, &named).label,
        "chat",
        "focused-label exists so the current workspace can show its name while the rest \
             stay plain"
    );
}

#[test]
fn each_state_reads_its_own_template_and_falls_back_to_the_plain_one() {
    let settings = PagerConfig {
        label: "{index}".to_owned(),
        focused_label: Some("[{index}]".to_owned()),
        unfocused_label: Some("-{index}-".to_owned()),
        urgent_label: Some("!{index}".to_owned()),
        ..PagerConfig::default()
    };

    assert_eq!(template(&settings, true, false), "[{index}]");
    assert_eq!(template(&settings, false, false), "-{index}-");
    assert_eq!(template(&settings, false, true), "!{index}");
    assert_eq!(
        template(&settings, true, true),
        "!{index}",
        "urgency outranks focus, because it is the one state the strip exists to surface"
    );

    let bare = PagerConfig {
        label: "{index}".to_owned(),
        ..PagerConfig::default()
    };
    for (focused, urgent) in [(true, true), (true, false), (false, true), (false, false)] {
        assert_eq!(
            template(&bare, focused, urgent),
            "{index}",
            "every state falls back to `label`, so setting one does not blank the others"
        );
    }
}

#[test]
fn vertical_steps_whatever_the_strip_is_showing() {
    assert!(!steps_windows(PagerMode::Workspaces, false));
    assert!(steps_windows(PagerMode::Windows, false));
}

#[test]
fn horizontal_steps_the_dimension_the_strip_is_not_showing() {
    assert!(steps_windows(PagerMode::Workspaces, true));
    assert!(!steps_windows(PagerMode::Windows, true));
}

#[test]
fn scope_output_keeps_only_the_workspaces_on_this_panels_monitor() {
    let here = workspace(1, "DP-1", true, true);
    let there = workspace(9, "HDMI-A-1", false, true);

    assert!(in_scope(&here, PagerScope::Output, Some("DP-1")));
    assert!(!in_scope(&there, PagerScope::Output, Some("DP-1")));
}

#[test]
fn scope_output_without_a_connector_shows_the_session_rather_than_nothing() {
    let there = workspace(9, "HDMI-A-1", false, true);

    assert!(
        in_scope(&there, PagerScope::Output, None),
        "an empty strip would look like a broken applet, not like a missing connector"
    );
}

#[test]
fn scope_current_is_the_single_label_the_old_workspace_applet_was() {
    let focused = workspace(1, "DP-1", true, true);
    let current_elsewhere = workspace(9, "HDMI-A-1", false, true);

    assert!(in_scope(&focused, PagerScope::Current, Some("DP-1")));
    assert!(
        !in_scope(&current_elsewhere, PagerScope::Current, Some("DP-1")),
        "another output's current workspace is not the one the user is standing in"
    );
}
