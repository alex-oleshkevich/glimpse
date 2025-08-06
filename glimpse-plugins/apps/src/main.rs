use gio::prelude::*;
use glimpse_sdk::{Action, Command, Icon, ReplyWriter, Response, SearchPlugin};
use std::env;
use std::path::Path;
use std::process;

struct App {}

impl App {
    fn new() -> Self {
        Self {}
    }
}

fn make_icon(app_info: &gio::AppInfo) -> String {
    let mut icon = "application-x-executable-symbolic".to_string();
    if let Some(icon_) = app_info.icon().as_ref() {
        if let Some(icon_str) = icon_.to_string() {
            icon = icon_str.to_string();
        }
    }
    icon
}

impl SearchPlugin for App {
    async fn search(&self, query: String, output: &mut ReplyWriter<'_>) {
        let input = query.trim();
        if input.is_empty() {
            return;
        }

        let all_apps = gio::AppInfo::all();
        let results: Vec<Command> = all_apps
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
                    actions.push(Action::LaunchApp {
                        title: "Launch".to_string(),
                        app_id: app_id.to_string(),
                        new_instance: false,
                    });
                    actions.push(Action::LaunchApp {
                        title: "Launch new instance".to_string(),
                        app_id: app_id.to_string(),
                        new_instance: true,
                    });
                }

                Command {
                    title: app_info.name().to_string(),
                    subtitle: app_info.description().unwrap_or_default().to_string(),
                    icon: Icon::Freedesktop {
                        name: make_icon(app_info),
                    },
                    category: "Apps".to_string(),
                    actions,
                }
            })
            .collect();

        output.reply(Response::SearchResults(results)).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <path>", args[0]);
        process::exit(1);
    }

    let socket_path = &args[1];
    if !Path::new(socket_path).exists() {
        process::exit(1);
    }

    let plugin = App::new();
    if let Err(e) = plugin.run(socket_path.into()).await {
        eprintln!("Error running plugin: {}", e);
        process::exit(1);
    }

    Ok(())
}
