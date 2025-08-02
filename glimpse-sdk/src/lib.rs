pub mod errors;
pub mod jsonrpc;
pub mod messages;
pub mod search_plugin;
pub use errors::GlimpseError;
pub use jsonrpc::{JSONRPCError, JSONRPCRequest, JSONRPCResponse};
pub use messages::{Request, Response};
pub use search_plugin::{ReplyWriter, SearchPlugin};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Icon {
    None,
    Path { path: String },
    Freedesktop { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    Open { path: String },
    OpenUrl { url: String },
    CopyToClipboard { text: String },
    RunCommand { command: String },
    LaunchApp { app_id: String, new_instance: bool },
    Callback { payload: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub title: String,
    pub subtitle: String,
    pub icon: Icon,
    pub category: String,
    pub actions: Vec<Action>,
}
