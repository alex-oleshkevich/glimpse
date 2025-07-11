use crate::commands;
use crate::extensions;
use crate::messages;
use gio::prelude::*;

pub struct Apps {}

const EXTENSION_ID: &str = "me.aresa.glimpse.apps";

impl Apps {
    pub fn new() -> Self {
        Self {}
    }

    fn make_icon(&self, app_info: &gio::AppInfo) -> String {
        let mut icon = "application-x-executable-symbolic".to_string();
        if let Some(icon_) = app_info.icon().as_ref() {
            if let Some(icon_str) = icon_.to_string() {
                icon = icon_str.to_string();
            }
        }
        icon
    }

    fn query_apps(&self, query: &str) -> Vec<commands::Command> {
        gio::AppInfo::all()
            .iter()
            .filter(|app_info| {
                if !app_info.should_show() {
                    return false;
                }

                let title_matches = app_info
                    .name()
                    .to_lowercase()
                    .contains(&query.to_lowercase());
                let description_matches = app_info
                    .description()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&query.to_lowercase());

                title_matches || description_matches
            })
            .map(|app_info| {
                let mut actions = vec![];
                if let Some(app_id) = app_info.id() {
                    actions.push(commands::Action::LaunchApp {
                        app_id: app_id.to_string(),
                    });
                }

                commands::Command::new(
                    app_info.name().to_string(),
                    app_info.description().unwrap_or_default().to_string(),
                    self.make_icon(app_info),
                    actions,
                )
            })
            .collect()
    }
}

impl extensions::Extension for Apps {
    fn id(&self) -> String {
        EXTENSION_ID.to_string()
    }

    fn metadata(&self) -> extensions::ExtensionMetadata {
        extensions::ExtensionMetadata {
            name: "Apps".to_string(),
            description: "Provides commands to launch applications.".to_string(),
            version: "0.1.0".to_string(),
            author: "Alex Oleshkevich".to_string(),
        }
    }

    fn query(&self, query: &messages::Message) -> Vec<commands::Command> {
        match query {
            messages::Message::Query(query_str) => self.query_apps(query_str),
            _ => vec![],
        }
    }
}
