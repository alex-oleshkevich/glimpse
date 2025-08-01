use glimpse_sdk::{JSONRPCRequest, JSONRPCResponse, Request, Response};
use tokio::sync::broadcast;

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

#[derive(Debug, Clone)]
pub enum Message {
    ClientRequest(JSONRPCRequest<Request>),
    PluginResponse(usize, JSONRPCResponse<Response>),
}
