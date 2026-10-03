use gettextrs::gettext;
use glimpse_services::WorkspaceInfo;

use crate::applets::workspace::{Facts, render, workspace_token};

pub const ICON: &str = "view-grid-symbolic";
const CHIP_MAX_CHARS: usize = 32;
const RECENT: usize = 6;

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

pub fn taken(workspaces: &[WorkspaceInfo], current: u64) -> Vec<(String, String)> {
    workspaces
        .iter()
        .filter(|workspace| workspace.id != current)
        .filter_map(|workspace| {
            let name = workspace.name.as_deref().filter(|name| !name.is_empty())?;
            let message = gettext("Already the name of {workspace}")
                .replace("{workspace}", &title(workspace));
            Some((name.to_owned(), message))
        })
        .collect()
}

pub fn remember(recent: &mut Vec<String>, workspaces: &[WorkspaceInfo]) {
    for name in workspaces
        .iter()
        .filter_map(|workspace| workspace.name.as_deref())
    {
        let name = glimpse_utils::clean(name, CHIP_MAX_CHARS);
        if name.is_empty() || recent.iter().any(|known| known.eq_ignore_ascii_case(&name)) {
            continue;
        }
        recent.insert(0, name);
    }
    recent.truncate(RECENT);
}

pub fn suggestions(recent: &[String], workspaces: &[WorkspaceInfo]) -> Vec<String> {
    recent
        .iter()
        .filter(|name| {
            !workspaces.iter().any(|workspace| {
                workspace
                    .name
                    .as_deref()
                    .is_some_and(|held| held.eq_ignore_ascii_case(name))
            })
        })
        .cloned()
        .collect()
}

pub fn rename(current: Option<&str>, typed: &str) -> Option<Option<String>> {
    let typed = typed.trim();
    let wanted = (!typed.is_empty()).then(|| typed.to_owned());
    let current = current.filter(|name| !name.is_empty());
    (wanted.as_deref() != current).then_some(wanted)
}

#[cfg(test)]
mod tests;
