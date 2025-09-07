use std::error::Error;

use async_trait::async_trait;
use glimpse_sdk::{
    Action, Method, MethodResult, Plugin, PluginError, SearchItem, run_plugin, setup_logging,
};

struct EchoPlugin {}

#[async_trait]
impl Plugin for EchoPlugin {
    async fn handle(&self, method: Method) -> Result<MethodResult, PluginError> {
        match method {
            Method::Search(query) => {
                let item = SearchItem {
                    title: format!("Echo: {}", query),
                    subtitle: Some("From echo plugin".to_string()),
                    icon: Some("echo.png".to_string()),
                    actions: vec![Action::CopyToClipboard { text: query }],
                    score: 1.0,
                };

                Ok(MethodResult::SearchResults(vec![item]))
            }
            _ => Ok(MethodResult::SearchResults(vec![])),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    setup_logging(tracing::Level::DEBUG);

    let plugin = EchoPlugin {};
    if let Err(err) = run_plugin(plugin).await {
        tracing::error!("Error running plugin: {}", err);
    }
    Ok(())
}
