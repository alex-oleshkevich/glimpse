use gettextrs::{gettext, ngettext};
use glimpse_services::WorkspaceInfo;

use crate::applets::workspace::{Facts, render, workspace_token};

pub const ICON: &str = "view-grid-symbolic";
const CHIP_MAX_CHARS: usize = 32;

pub fn current<'a>(
    workspaces: &'a [WorkspaceInfo],
    output: Option<&str>,
) -> Option<&'a WorkspaceInfo> {
    output
        .and_then(|output| {
            workspaces
                .iter()
                .find(|workspace| workspace.active && workspace.output.as_deref() == Some(output))
        })
        .or_else(|| workspaces.iter().find(|workspace| workspace.focused))
}

pub fn chip(workspace: &WorkspaceInfo) -> String {
    glimpse_utils::clean(&workspace_token(workspace), CHIP_MAX_CHARS)
}

pub fn title(workspace: &WorkspaceInfo) -> String {
    let ordinal = workspace.index.map_or(workspace.id, u64::from);
    gettext("Workspace {number}").replace("{number}", &ordinal.to_string())
}

pub fn subtitle(workspace: &WorkspaceInfo) -> String {
    let windows = ngettext("{count} window", "{count} windows", workspace.windows)
        .replace("{count}", &workspace.windows.to_string());
    match workspace.output.as_deref() {
        Some(output) => format!("{output} · {windows}"),
        None => windows,
    }
}

pub fn tooltip(workspace: &WorkspaceInfo, format: Option<&str>) -> String {
    match format {
        Some(format) => render(
            format,
            &Facts {
                index: workspace.index.map(u64::from),
                id: workspace.id,
                name: workspace.name.as_deref(),
                workspace: Some(&workspace_token(workspace)),
            },
        ),
        None => match workspace.output.as_deref() {
            Some(output) => format!("{} · {output}", title(workspace)),
            None => title(workspace),
        },
    }
}

pub fn rename(current: Option<&str>, typed: &str) -> Option<Option<String>> {
    let typed = typed.trim();
    let wanted = (!typed.is_empty()).then(|| typed.to_owned());
    let current = current.filter(|name| !name.is_empty());
    (wanted.as_deref() != current).then_some(wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn the_hero_counts_windows_and_names_the_output() {
        let mut one = workspace(3, "DP-2", true, true);
        one.windows = 1;
        assert_eq!(title(&one), "Workspace 3");
        assert_eq!(subtitle(&one), "DP-2 · 1 window");
        one.windows = 4;
        one.output = None;
        assert_eq!(subtitle(&one), "4 windows");
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
}
