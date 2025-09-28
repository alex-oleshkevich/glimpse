use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::Metadata;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Method {
    Search(String),
    Activate(usize, usize),                      // match index, action index
    CallAction(String, HashMap<String, String>), // action key
    Cancel,
    Quit,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MethodResult {
    Authenticate(Metadata),
    Matches { items: Vec<Match> },
    Error(String),
    None,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum Message {
    Request {
        id: usize,
        #[serde(flatten)]
        method: Method,
        plugin_id: Option<String>,
    },
    Response {
        id: usize,
        error: Option<String>,
        result: Option<MethodResult>,
        plugin_id: Option<String>,
    },
    Notification {
        #[serde(flatten)]
        method: Method,
        plugin_id: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Exec {
        command: String,
        args: Vec<String>,
    },
    Launch {
        app_id: String,
        action: Option<String>,
    },
    Open {
        uri: String,
    },
    Clipboard {
        text: String,
    },
    Callback {
        key: String,
        params: HashMap<String, String>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub struct MatchAction {
    pub title: String,
    pub action: Action,
    pub close_on_action: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub struct Match {
    pub title: String,
    pub description: String,
    pub icon: Option<String>,
    pub actions: Vec<MatchAction>,
    pub score: f64,
}
