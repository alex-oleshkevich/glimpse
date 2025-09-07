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
    async fn handle(&self, method: Method) -> Result<MethodResult, PluginError>;
}
