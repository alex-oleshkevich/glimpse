use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use serde::de::Error;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::app::{self, SearchItem};
use crate::jsonrpc::{JSONRPCRequest, JSONRPCResponse};

mod process;

#[derive(Debug)]
pub enum ExtensionError {
    DispatchError(String),
}

#[derive(Debug)]
pub enum Extension {
    Process(process::ProcessHandle),
}

impl Extension {
    pub async fn dispatch(&self, request: app::AppMessage) -> Result<(), ExtensionError> {
        match self {
            Extension::Process(handle) => handle.dispatch(request).await,
        }
    }
}

pub fn extension_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(data_dir) = dirs::data_dir() {
        let plugins_dir = data_dir.join("glimpse").join("plugins");
        if plugins_dir.exists() {
            paths.push(plugins_dir);
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let local_path = cwd.join("plugins");
        if local_path.exists() {
            paths.push(local_path);
        }
    }

    paths
}

pub fn load_extensions(app_tx: mpsc::Sender<app::AppMessage>) -> Vec<Extension> {
    let mut extensions = Vec::new();
    let paths = extension_paths();
    tracing::info!("looking for extensions in: {:?}", paths);
    for path in paths {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                if entry.path().is_file() {
                    let path_metadata = entry.path().metadata();
                    if let Err(e) = path_metadata {
                        tracing::error!("failed to read metadata for {:?}: {}", entry.path(), e);
                        continue;
                    }

                    let permissions = path_metadata.unwrap().permissions();
                    let mode = permissions.mode();
                    if mode & 0o111 == 0 {
                        tracing::warn!("skipping non-executable file: {:?}", entry.path());
                        continue;
                    }

                    match process::ProcessHandle::new(entry.path(), app_tx.clone()) {
                        Ok(extension) => {
                            tracing::info!("loaded extension: {:?}", entry.path());
                            extensions.push(Extension::Process(extension));
                        }
                        Err(e) => {
                            tracing::error!(
                                "create process handle for {:?}: {:?}",
                                entry.path(),
                                e
                            );
                        }
                    }
                }
            }
        }
    }
    extensions
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Request {
    Search(String),
}

impl Request {
    pub fn to_jsonrpc(&self) -> JSONRPCRequest {
        match self {
            Request::Search(query) => {
                let params = serde_json::json!({ "query": query });
                JSONRPCRequest::new("search".to_string(), Some(params))
            }
        }
    }

    pub fn to_string(&self) -> Result<String, serde_json::Error> {
        self.to_jsonrpc().to_json()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Response {
    SearchItem(SearchItem),
}

impl Response {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let response: JSONRPCResponse = serde_json::from_str(json)?;
        if let Some(result) = response.result {
            match serde_json::from_value::<SearchItem>(result) {
                Ok(item) => return Ok(Response::SearchItem(item)),
                Err(e) => return Err(serde_json::Error::custom(format!("invalid response format: {}", e))),
            }
        }
        Err(serde_json::Error::custom("invalid response format"))
    }
}
