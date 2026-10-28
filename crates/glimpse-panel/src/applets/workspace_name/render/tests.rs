use super::*;

fn named(id: u64, name: &str) -> WorkspaceInfo {
    WorkspaceInfo {
        name: Some(name.to_owned()),
        ..workspace(id, "DP-2", false, false)
    }
}

#[test]
fn a_name_another_workspace_holds_is_taken_and_its_own_is_not() {
    let workspaces = [named(1, "web"), named(2, "chat")];
    let taken = taken(&workspaces, 2);
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].0, "web");
    assert!(taken[0].1.contains("Workspace 1"));
}

#[test]
fn recent_names_survive_their_workspace_and_never_offer_one_in_use() {
    let mut recent = Vec::new();
    remember(&mut recent, &[named(1, "web"), named(2, "chat")]);
    remember(&mut recent, &[named(1, "Web"), named(2, "music")]);
    assert_eq!(
        recent,
        ["music", "chat", "web"],
        "newest first, case-folded"
    );

    let now = [named(1, "web")];
    assert_eq!(suggestions(&recent, &now), ["music", "chat"]);
}

fn workspace(id: u64, output: &str, active: bool, focused: bool) -> WorkspaceInfo {
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

#[test]
fn the_bar_names_the_workspace_active_on_its_own_output() {
    let workspaces = [
        workspace(1, "DP-2", true, true),
        workspace(2, "eDP-1", false, false),
        workspace(3, "eDP-1", true, false),
    ];
    assert_eq!(current(&workspaces, Some("eDP-1")).map(|w| w.id), Some(3));
    assert_eq!(current(&workspaces, Some("DP-2")).map(|w| w.id), Some(1));
}

#[test]
fn an_unknown_output_falls_back_to_the_focused_workspace() {
    let workspaces = [
        workspace(1, "DP-2", true, false),
        workspace(2, "eDP-1", true, true),
    ];
    assert_eq!(current(&workspaces, None).map(|w| w.id), Some(2));
    assert_eq!(current(&workspaces, Some("HDMI-1")).map(|w| w.id), Some(2));
    assert_eq!(current(&[], Some("DP-2")).map(|w| w.id), None);
}

#[test]
fn an_unnamed_workspace_shows_its_index_and_a_named_one_its_name() {
    let mut named = workspace(3, "DP-2", true, true);
    assert_eq!(chip(&named), "3");
    named.name = Some("dev".to_owned());
    assert_eq!(chip(&named), "dev");
}

#[test]
fn the_chip_is_capped_by_characters_not_bytes() {
    let mut long = workspace(1, "DP-2", true, true);
    long.name = Some("ж".repeat(100));
    assert_eq!(chip(&long).chars().count(), CHIP_MAX_CHARS + 1);
    assert!(chip(&long).ends_with('…'));
}

#[test]
fn a_hostile_name_reaches_the_chip_flattened() {
    let mut hostile = workspace(1, "DP-2", true, true);
    hostile.name = Some("dev\n\u{202e}exe.jpg".to_owned());
    assert_eq!(chip(&hostile), "dev exe.jpg");
}

#[test]
fn the_tooltip_reads_its_format_tokens() {
    let mut named = workspace(3, "DP-2", true, true);
    named.name = Some("dev".to_owned());
    assert_eq!(tooltip(&named, None), "Workspace 3 · DP-2");
    assert_eq!(tooltip(&named, Some("{index}: {name}")), "3: dev");
    named.name = None;
    assert_eq!(tooltip(&named, Some("{name-or-index}")), "3");
    assert_eq!(tooltip(&named, Some("{workspace-name}")), "3");
}

#[test]
fn a_rename_is_trimmed_and_an_empty_one_clears_the_name() {
    assert_eq!(rename(None, "  dev "), Some(Some("dev".to_owned())));
    assert_eq!(rename(Some("dev"), "   "), Some(None));
    assert_eq!(rename(Some("dev"), "chat"), Some(Some("chat".to_owned())));
}

#[test]
fn an_unchanged_name_sends_nothing() {
    assert_eq!(rename(Some("dev"), "dev "), None);
    assert_eq!(rename(None, ""), None);
    assert_eq!(rename(Some(""), ""), None);
}
