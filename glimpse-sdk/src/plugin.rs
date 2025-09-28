use std::{collections::HashMap, path::PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{Match, Method, MethodResult, PluginError};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Metadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
}

#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    fn metadata(&self) -> Metadata;

    async fn initialize(&self, _context: &Context) -> Result<(), PluginError> {
        Ok(())
    }

    async fn dispatch(&self, method: Method) -> Result<MethodResult, PluginError> {
        self.handle(method).await
    }

    async fn handle(&self, method: Method) -> Result<MethodResult, PluginError> {
        tracing::debug!("handling method: {:?}", method);
        match method {
            Method::Search(query) => {
                let results = self.handle_search(query).await;
                match results {
                    Err(e) => Ok(MethodResult::Error(e.to_string())),
                    Ok(results) => Ok(MethodResult::Matches { items: results }),
                }
            }
            Method::CallAction(action, params) => {
                self.handle_action(action, params).await;
                Ok(MethodResult::None)
            }
            _ => Ok(MethodResult::None),
        }
    }

    async fn handle_search(&self, query: String) -> Result<Vec<Match>, PluginError>;

    async fn handle_action(&self, action: String, params: HashMap<String, String>) {
        tracing::warn!("unhandled action: {} {:?}", action, params);
    }
}

pub struct Context {
    pub config_dir: PathBuf,
}
