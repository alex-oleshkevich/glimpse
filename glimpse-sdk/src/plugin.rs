use std::{collections::HashMap, path::PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{Method, MethodResult, PluginError};

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

    async fn initialize(&self, _config_dir: &PathBuf) -> Result<(), PluginError> {
        Ok(())
    }

    async fn dispatch(&self, method: Method) -> Result<MethodResult, PluginError> {
        self.handle(method).await
    }

    async fn handle(&self, method: Method) -> Result<MethodResult, PluginError>;

    async fn handle_action(&self, action: String, params: HashMap<String, String>) -> MethodResult {
        tracing::warn!("unhandled action: {} {:?}", action, params);
        MethodResult::None
    }
}
