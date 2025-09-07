use std::error::Error;

use async_trait::async_trait;
use glimpse_sdk::{
    Action, Metadata, Method, MethodResult, Plugin, PluginError, SearchItem, run_plugin,
    setup_logging,
};

struct EchoPlugin {}

#[async_trait]
impl Plugin for EchoPlugin {
    fn metadata(&self) -> Metadata {
        Metadata {
            id: "me.aresa.glimpse.debug".to_string(),
            name: "Debug Plugin".to_string(),
            version: "0.1.0".to_string(),
            description: "A simple debug plugin that returns the search query as a result."
                .to_string(),
            author: "Your Name <you@example.com>".to_string(),
        }
    }

    async fn handle(&self, method: Method) -> Result<MethodResult, PluginError> {
        match method {
            Method::Search(query) => match true {
                _ if query.trim().eq("panic") => {
                    panic!("Simulated panic for query: {}", query);
                }
                _ if query.trim().eq("error") => {
                    Err(PluginError::Other("Simulated error".to_string()))
                }
                val if val == query.starts_with("sleep") => {
                    let duration = query
                        .split_whitespace()
                        .nth(1)
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(2);
                    tokio::time::sleep(tokio::time::Duration::from_secs(duration)).await;
                    tracing::info!("slept for {} seconds", duration);
                    Ok(MethodResult::SearchResults(vec![SearchItem {
                        title: format!("Slept for {} seconds: {}", duration, query),
                        subtitle: Some("Sleep command executed".to_string()),
                        icon: Some("sleep.png".to_string()),
                        actions: vec![Action::Clipboard {
                            text: format!("Executed: {}", query),
                        }],
                        score: 1.0,
                    }]))
                }
                _ => Ok(MethodResult::SearchResults(vec![SearchItem {
                    title: format!("Debug: {}", query),
                    subtitle: Some("From debug plugin".to_string()),
                    icon: Some("echo.png".to_string()),
                    actions: vec![Action::Clipboard { text: query }],
                    score: 1.0,
                }])),
            },
            _ => Ok(MethodResult::SearchResults(vec![])),
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
