use serde::{Deserialize, Serialize};

use crate::Metadata;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Method {
    Search(String),
    Cancel,
    Quit,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum MethodResult {
    Authenticate(Metadata),
    SearchResults(Vec<SearchItem>),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum Message {
    Request {
        id: usize,
        #[serde(flatten)]
        method: Method,
        target: Option<String>,
        context: Option<String>,
    },
    Response {
        id: usize,
        error: Option<String>,
        source: Option<String>,
        result: Option<MethodResult>,
    },
    Notification {
        #[serde(flatten)]
        method: Method,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case", tag = "_type")]
pub enum Action {
    ShellExec {
        command: String,
        args: Vec<String>,
    },
    OpenPath {
        path: String,
    },
    Clipboard {
        text: String,
    },
    Custom {
        action: String,
        params: serde_json::Value,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case", tag = "_type")]
pub struct SearchItem {
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<String>,
    pub actions: Vec<Action>,
    pub score: f64,
}
