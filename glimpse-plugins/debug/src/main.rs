use freedesktop_desktop_entry::{DesktopEntry, default_paths, get_languages_from_env};
use std::{collections::HashMap, error::Error};

use async_trait::async_trait;
use freedesktop_icons::lookup;
use glimpse_sdk::{
    Action, Match, MatchAction, Metadata, Plugin, PluginError, run_plugin, setup_logging,
};

struct EchoPlugin {}

impl EchoPlugin {
    fn example_search_results(&self) -> Vec<Match> {
        let locales = get_languages_from_env();

        tracing::debug!("Detected locales: {:?}", locales);
        tracing::debug!(
            "Looking up desktop entries in {}",
            default_paths()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(":")
        );

        let mut results = vec![
            "/usr/share/applications/org.telegram.desktop.desktop",
            "/usr/share/applications/code.desktop",
            "/usr/share/applications/org.gnome.Nautilus.desktop",
        ]
        .iter()
        .filter_map(|path| DesktopEntry::from_path(path, Some(&locales)).ok())
        .map(|de| {
            let mut actions: Vec<MatchAction> = vec![MatchAction {
                title: format!(
                    "Launch {}",
                    de.name(&locales).unwrap_or_else(|| "Unknown".into())
                ),
                close_on_action: true,
                action: Action::Launch {
                    app_id: de.id().to_string(),
                    action: None,
                },
            }];
            actions.extend_from_slice(
                de.actions()
                    .unwrap_or_default()
                    .iter()
                    .filter(|action_name| !action_name.is_empty())
                    .filter_map(|action_name| {
                        Some(MatchAction {
                            title: de
                                .action_name(action_name, &locales)
                                .unwrap_or_else(|| "Launch".into())
                                .to_string(),
                            close_on_action: true,
                            action: Action::Launch {
                                app_id: de.id().to_string(),
                                action: Some(action_name.to_string()),
                            },
                        })
                    })
                    .collect::<Vec<MatchAction>>()
                    .as_ref(),
            );

            Match {
                title: de
                    .name(&locales)
                    .unwrap_or_else(|| "Unknown".into())
                    .to_string(),
                description: de
                    .comment(&locales)
                    .unwrap_or_else(|| "".into())
                    .to_string(),
                icon: de.icon().and_then(|icon_name| {
                    lookup(&icon_name)
                        .find()
                        .map(|p| p.to_string_lossy().to_string())
                }),
                actions,
                score: 1.0,
            }
        })
        .collect::<Vec<_>>();
        results.extend_from_slice(&vec![
            Match {
                title: "No actions".to_string(),
                description: "A result with no actions".to_string(),
                icon: lookup("dialog-information")
                    .find()
                    .map(|p| p.to_string_lossy().to_string()),
                actions: vec![],
                score: 0.9,
            },
            Match {
                title: "Copy to Clipboard".to_string(),
                description: "Copies text to clipboard".to_string(),
                icon: lookup("edit-copy")
                    .find()
                    .map(|p| p.to_string_lossy().to_string()),
                actions: vec![
                    MatchAction {
                        title: "Copy Hello World".to_string(),
                        close_on_action: true,
                        action: Action::Clipboard {
                            text: "Hello World".to_string(),
                        },
                    },
                    MatchAction {
                        title: "Copy Hello World and keep open".to_string(),
                        close_on_action: false,
                        action: Action::Clipboard {
                            text: "Hello World".to_string(),
                        },
                    },
                ],
                score: 0.8,
            },
            Match {
                title: "Open Rust Website".to_string(),
                description: "Opens the Rust programming language website".to_string(),
                icon: lookup("applications-internet")
                    .find()
                    .map(|p| p.to_string_lossy().to_string()),
                actions: vec![MatchAction {
                    title: "Open https://www.rust-lang.org".to_string(),
                    close_on_action: true,
                    action: Action::Open {
                        uri: "https://www.rust-lang.org".to_string(),
                    },
                }],
                score: 0.7,
            },
            Match {
                title: "Open home directory".to_string(),
                description: "Opens the home directory in the file manager".to_string(),
                icon: lookup("user-home")
                    .find()
                    .map(|p| p.to_string_lossy().to_string()),
                actions: vec![MatchAction {
                    title: "Open Home".to_string(),
                    close_on_action: true,
                    action: Action::Open {
                        uri: format!(
                            "file:///home/{}",
                            std::env::var("USER").unwrap_or("user".to_string())
                        ),
                    },
                }],
                score: 0.6,
            },
            Match {
                title: "Run htop Command".to_string(),
                description: "Runs the htop command in a terminal".to_string(),
                icon: lookup("htop")
                    .find()
                    .map(|p| p.to_string_lossy().to_string()),
                actions: vec![MatchAction {
                    title: "Run htop".to_string(),
                    close_on_action: true,
                    action: Action::Exec {
                        command: "ghostty".to_string(),
                        args: vec!["-e".to_string(), "htop".to_string()],
                    },
                }],
                score: 0.6,
            },
            Match {
                title: "Execute Plugin callback".to_string(),
                description: "Executes a callback action".to_string(),
                icon: lookup("system-run")
                    .find()
                    .map(|p| p.to_string_lossy().to_string()),
                actions: vec![MatchAction {
                    title: "Execute Callback".to_string(),
                    close_on_action: false,
                    action: Action::Callback {
                        key: "example_callback".to_string(),
                        params: {
                            let mut map = HashMap::new();
                            map.insert("example_key".to_string(), "example_value".to_string());
                            map
                        },
                    },
                }],
                score: 0.6,
            },
        ]);
        results
    }
}

#[async_trait]
impl Plugin for EchoPlugin {
    fn metadata(&self) -> Metadata {
        Metadata {
            id: "me.aresa.glimpse.debug".to_string(),
            name: "Debug Plugin".to_string(),
            version: "0.1.1".to_string(),
            description: "A simple debug plugin that returns the search query as a result."
                .to_string(),
            author: "Your Name <you@example.com>".to_string(),
        }
    }

    async fn handle_search(&self, query: String) -> Result<Vec<Match>, PluginError> {
        match true {
            _ if query.trim().eq("panic") => {
                panic!("Simulated panic for query: {}", query);
            }
            _ if query.trim().eq("error") => Err(PluginError::Other("Simulated error".to_string())),
            _ => {
                let results = self.example_search_results();
                let filtered: Vec<Match> = results
                    .into_iter()
                    .filter(|item| {
                        item.title.to_lowercase().contains(&query.to_lowercase())
                            || item
                                .description
                                .to_lowercase()
                                .contains(&query.to_lowercase())
                    })
                    .collect();
                Ok(filtered)
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    setup_logging(tracing::Level::DEBUG);
    let plugin = EchoPlugin {};
    if let Err(err) = run_plugin(plugin).await {
        tracing::error!("error running plugin: {}", err);
    }
    Ok(())
}
