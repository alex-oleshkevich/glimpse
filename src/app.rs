use serde::Deserialize;
use tokio::sync::mpsc;

use crate::extensions::Extension;
use crate::extensions::Request;
use crate::extensions::Response;
use crate::extensions::load_extensions;
use crate::icons::Icon;

#[derive(Debug, Clone, Deserialize)]
pub struct Action {}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchItem {
    pub title: String,
    pub subtitle: String,
    pub category: String,
    pub icon: Icon,
    pub actions: Vec<Action>,
}

impl SearchItem {
    pub fn primary_action(&self) -> Option<&Action> {
        self.actions.first()
    }
}

#[derive(Debug, Clone)]
pub enum AppMessage {
    Bootstrap(mpsc::Sender<AppMessage>),
    Request(Request),
    Response(Response),
}

pub struct App {
    pending: usize,
    start_time: std::time::Instant,
}

impl App {
    pub fn new() -> Self {
        App {
            pending: 0,
            start_time: std::time::Instant::now(),
        }
    }

    pub async fn run(self, to_ui: mpsc::Sender<AppMessage>, from_ui: mpsc::Receiver<AppMessage>) {
        let (app_tx, app_rx) = mpsc::channel(16);

        let extensions = load_extensions(app_tx);
        tokio::select! {
            _ = self.start_request_handler(extensions, from_ui) => {
                tracing::debug!("request handler finished");
            },
            _ = App::start_response_handler(to_ui, app_rx) => {
                tracing::debug!("response handler finished");
            },
        }
        tracing::debug!("app run completed");
    }

    async fn start_request_handler(
        mut self,
        extensions: Vec<Extension>,
        mut from_ui: mpsc::Receiver<AppMessage>,
    ) {
        tracing::debug!("starting request handler");
        while let Some(input) = from_ui.recv().await {
            match input {
                AppMessage::Request(Request::Search(_)) => {
                    self.pending = extensions.len();
                }
                _ => {}
            }

            for extension in extensions.iter() {
                if let Err(err) = extension.dispatch(input.clone()).await {
                    tracing::error!("failed to dispatch request to extension: {:?}", err);
                }
            }
        }
    }

    async fn start_response_handler(
        to_ui: mpsc::Sender<AppMessage>,
        mut from_app: mpsc::Receiver<AppMessage>,
    ) {
        tracing::debug!("starting response handler");
        while let Some(input) = from_app.recv().await {
            to_ui.send(input).await.ok();
        }
    }
}
