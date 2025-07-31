use std::fmt::Display;

use glimpse_sdk::Command;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::jsonrpc::{JSONRPCRequest, JSONRPCResponse};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Request {
    Ping,
    Search { query: String },
}

impl Display for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Request")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response {
    Pong,
    SearchResults(Vec<Command>),
}

impl Response {}

impl Display for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Response")
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    ClientRequest(JSONRPCRequest<Request>),
    PluginResponse(JSONRPCResponse<Response>),
}

pub struct MessageBus {
    sender: broadcast::Sender<Message>,
    _receiver: broadcast::Receiver<Message>,
}

impl MessageBus {
    pub fn new() -> Self {
        let (sender, receiver) = broadcast::channel(12);
        Self {
            sender,
            _receiver: receiver,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Message> {
        self.sender.subscribe()
    }

    pub fn publisher(&self) -> broadcast::Sender<Message> {
        self.sender.clone()
    }
}
