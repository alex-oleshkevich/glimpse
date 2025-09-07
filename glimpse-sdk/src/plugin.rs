use async_trait::async_trait;

use crate::{Method, MethodResult, PluginError};

#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    async fn handle(&self, method: Method) -> Result<MethodResult, PluginError>;
}
