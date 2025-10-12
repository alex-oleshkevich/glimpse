use std::{cell::RefCell, collections::HashMap, error::Error, sync::Mutex};

use async_trait::async_trait;
use freedesktop_desktop_entry::{Iter, default_paths, get_languages_from_env};
use freedesktop_icons::lookup;
use glimpse_sdk::{Match, MatchAction, Metadata, Plugin, PluginError, run_plugin, setup_logging};

struct App {
    default_icon: Option<String>,
    locales: Vec<String>,
    entries: Mutex<RefCell<Vec<freedesktop_desktop_entry::DesktopEntry>>>,
    icon_cache: Mutex<HashMap<String, Option<String>>>,
}

impl App {
    fn new() -> Self {
        Self {
            entries: Mutex::new(RefCell::new(Vec::new())),
            icon_cache: Mutex::new(HashMap::new()),
            locales: get_languages_from_env(),
            default_icon: lookup("system-run")
                .find()
                .map(|icon| icon.to_string_lossy().to_string()),
        }
    }
    fn get_entries(&self) -> Vec<freedesktop_desktop_entry::DesktopEntry> {
        let entries = self.entries.lock().unwrap();
        if !entries.borrow().is_empty() {
            return entries.borrow().clone();
        }

        tracing::debug!("loading desktop entries from filesystem");
        entries.replace(self.load_entries());
        entries.borrow().clone()
    }

    fn load_entries(&self) -> Vec<freedesktop_desktop_entry::DesktopEntry> {
        Iter::new(default_paths())
            .entries(Some(&self.locales[..]))
            .collect::<Vec<_>>()
    }

    fn get_icon(&self, icon_name: &str) -> Option<String> {
        let mut cache = self.icon_cache.lock().unwrap();

        if let Some(cached_icon) = cache.get(icon_name) {
            return cached_icon.clone();
        }

        let icon_path = lookup(icon_name)
            .with_size(128)
            .find()
            .map(|icon| icon.to_string_lossy().to_string());

        cache.insert(icon_name.to_string(), icon_path.clone());
        icon_path
    }
}

#[async_trait]
impl Plugin for App {
    fn metadata(&self) -> Metadata {
        Metadata {
            id: "me.aresa.glimpse.apps".to_string(),
            name: "Apps".to_string(),
            version: "0.1.1".to_string(),
            description: "A plugin to search and launch installed applications.".to_string(),
            author: "Alex Oleshkevich <alex.oleshkevich@gmail.com>".to_string(),
        }
    }

    async fn search(&self, query: String) -> Result<Vec<Match>, PluginError> {
        let mut matches = Vec::new();
        for entry in self.get_entries() {
            let name = entry.name(&self.locales).unwrap_or_default();
            let display_name = entry.full_name(&self.locales).unwrap_or_default();

            if entry.no_display() {
                continue;
            }

            if !name.to_lowercase().contains(&query.to_lowercase())
                && !display_name.to_lowercase().contains(&query.to_lowercase())
            {
                continue;
            }

            let mut score = 0.5;

            if name.to_lowercase().starts_with(&query.to_lowercase()) {
                score = 0.75;
            }

            if name.to_lowercase() == query.to_lowercase() {
                score = 1.0;
            }

            let desktop_path = entry.path.to_string_lossy().to_string();
            let mut actions = vec![MatchAction {
                title: "Launch".to_string(),
                action: glimpse_sdk::Action::DesktopFile {
                    path: desktop_path.clone(),
                    action: None,
                },
                close_on_action: true,
            }];

            if let Some(entry_actions) = entry.actions() {
                let new_actions = entry_actions
                    .iter()
                    .filter_map(|action| {
                        if !action.is_empty() {
                            return None;
                        }
                        return Some(MatchAction {
                            title: entry
                                .action_name(action, &self.locales)
                                .unwrap_or_default()
                                .to_string(),
                            action: glimpse_sdk::Action::DesktopFile {
                                path: desktop_path.clone(),
                                action: Some(action.to_string()),
                            },
                            close_on_action: true,
                        });
                    })
                    .collect::<Vec<_>>();

                tracing::debug!(
                    "found {} actions for entry {}",
                    new_actions.len(),
                    entry.id()
                );
                actions.extend(new_actions);
            }

            let match_item = Match {
                title: display_name.to_string(),
                description: entry
                    .comment(&self.locales)
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
                actions,
                score,
                icon: entry
                    .icon()
                    .and_then(|icon_name| self.get_icon(icon_name))
                    .or_else(|| self.default_icon.clone()),
            };

            matches.push(match_item);
        }

        matches.sort_by(|a, b| a.title.cmp(&b.title));
        Ok(matches)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    setup_logging(tracing::Level::DEBUG);
    let plugin = App::new();
    plugin.get_entries();
    if let Err(err) = run_plugin(plugin).await {
        tracing::error!("error running plugin: {}", err);
    }

    Ok(())
}
