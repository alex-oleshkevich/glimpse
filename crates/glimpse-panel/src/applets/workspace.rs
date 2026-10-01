use glimpse_services::WorkspaceInfo;

use crate::applets::tokens;

pub struct Facts<'a> {
    pub index: Option<u64>,
    pub id: u64,
    pub name: Option<&'a str>,
    pub workspace: Option<&'a str>,
}

impl Facts<'_> {
    fn ordinal(&self) -> String {
        match self.index {
            Some(index) => index.to_string(),
            None => self.id.to_string(),
        }
    }
}

pub fn render(template: &str, facts: &Facts) -> String {
    let ordinal = facts.ordinal();
    let id = facts.id.to_string();
    let name = facts.name.unwrap_or_default();
    let name_or_index = match name.is_empty() {
        true => ordinal.as_str(),
        false => name,
    };

    tokens::render(template, |token| match token {
        "workspace-name" => Some(facts.workspace.unwrap_or_default()),
        "name-or-index" => Some(name_or_index),
        "index" => Some(ordinal.as_str()),
        "id" => Some(id.as_str()),
        "name" => Some(name),
        _ => None,
    })
}

pub fn workspace_token(workspace: &WorkspaceInfo) -> String {
    match workspace.name.as_deref() {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ => workspace
            .index
            .map_or_else(|| workspace.id.to_string(), |index| index.to_string()),
    }
}

#[cfg(test)]
mod tests;
