use std::{collections::HashMap, error::Error};

use async_trait::async_trait;
use glimpse_sdk::{
    Action, Icon, Match, MatchAction, Metadata, Method, MethodResult, Plugin, PluginError,
    run_plugin, setup_logging,
};

struct EchoPlugin {}

impl EchoPlugin {
    fn example_search_results(&self) -> Vec<Match> {
        vec![
            Match {
                title: "Visual Studio Code".to_string(),
                description: "Code editor".to_string(),
                icon: Some(Icon::Name {
                    value: "vscode".to_string(),
                }),
                actions: vec![
                    MatchAction {
                        title: "Open VSCode".to_string(),
                        close_on_action: true,
                        action: Action::Launch {
                            app_id: "code".to_string(),
                            args: vec![],
                            new_instance: false,
                        },
                    },
                    MatchAction {
                        title: "New VSCode instance".to_string(),
                        close_on_action: true,
                        action: Action::Launch {
                            app_id: "code".to_string(),
                            args: vec![],
                            new_instance: true,
                        },
                    },
                ],
                score: 0.9,
            },
            Match {
                title: "No actions".to_string(),
                description: "A result with no actions".to_string(),
                icon: Some(Icon::Name {
                    value: "dialog-information".to_string(),
                }),
                actions: vec![],
                score: 0.9,
            },
            Match {
                title: "Copy to Clipboard".to_string(),
                description: "Copies text to clipboard".to_string(),
                icon: Some(Icon::Name {
                    value: "edit-copy".to_string(),
                }),
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
                icon: Some(Icon::Name {
                    value: "rust".to_string(),
                }),
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
                icon: Some(Icon::Name {
                    value: "user-home".to_string(),
                }),
                actions: vec![MatchAction {
                    title: "Open Home".to_string(),
                    close_on_action: true,
                    action: Action::Open {
                        uri: format!("file:///home/{}", std::env::var("USER").unwrap_or("user".to_string())),
                    },
                }],
                score: 0.6,
            },
            Match {
                title: "Run htop Command".to_string(),
                description: "Runs the htop command in a terminal".to_string(),
                icon: Some(Icon::Name {
                    value: "utilities-terminal".to_string(),
                }),
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
                icon: Some(Icon::Name {
                    value: "system-run".to_string(),
                }),
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
        ]
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

    async fn handle(&self, method: Method) -> Result<MethodResult, PluginError> {
        tracing::debug!("handling method: {:?}", method);
        match method {
            Method::Search(query) => match true {
                _ if query.trim().eq("panic") => {
                    panic!("Simulated panic for query: {}", query);
                }
                _ if query.trim().eq("error") => {
                    Err(PluginError::Other("Simulated error".to_string()))
                }
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
                    Ok(MethodResult::Matches { items: filtered })
                }
            },
            Method::Quit => {
                tracing::info!("Received Quit method, shutting down plugin.");
                return Ok(MethodResult::None);
            }
            _ => Ok(MethodResult::Matches { items: vec![] }),
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
