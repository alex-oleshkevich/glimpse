use std::sync::Arc;

use glimpse_config::Config;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{BrokerHandle, Ctx};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("service failed to start")]
    StartError,
    #[error("did not send message: {0}")]
    SendError(String),
}

pub enum Input<S: Service> {
    Event(S::Event),
    Command(S::Command),
    Config(S::Config),
}

pub trait Service: Sized + Send + 'static {
    type Config: PartialEq + Send + 'static;
    type Command: Send + 'static;
    type Event: Send + 'static;

    fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
    ) -> impl Future<Output = Result<Self, ServiceError>> + Send;
    fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) -> impl Future<Output = ()> + Send;
    fn stop(self, ctx: &Ctx<Self>) -> impl Future<Output = ()> + Send {
        let _ = ctx;
        async {}
    }
    fn peek_config(config: &Config) -> Self::Config;
}

const EVENT_BACKLOG_SIZE: usize = 128;

pub struct ServiceSender<S: Service> {
    inbox_tx: mpsc::Sender<Input<S>>,
}

impl<S: Service> ServiceSender<S> {
    pub async fn send(&self, input: Input<S>) -> Result<(), ServiceError> {
        self.inbox_tx
            .send(input)
            .await
            .map_err(|e| ServiceError::SendError(e.to_string()))
    }
}

pub struct ServiceRuntime<S: Service> {
    inbox_sender: mpsc::Sender<Input<S>>,
    inbox: mpsc::Receiver<Input<S>>,
    broker: Arc<BrokerHandle>,
    cancel: CancellationToken,
}

impl<S: Service> ServiceRuntime<S> {
    pub fn new(broker_handle: Arc<BrokerHandle>, cancel: CancellationToken) -> Self {
        let (inbox_tx, inbox_rx) = mpsc::channel::<Input<S>>(EVENT_BACKLOG_SIZE);
        return Self {
            cancel: cancel,
            inbox: inbox_rx,
            broker: broker_handle,
            inbox_sender: inbox_tx,
        };
    }

    pub fn sender(&self) -> ServiceSender<S> {
        ServiceSender {
            inbox_tx: self.inbox_sender.clone(),
        }
    }

    pub async fn run(&mut self, config: S::Config) -> Result<(), ServiceError> {
        let (events_tx, events_rx) = mpsc::channel::<S::Event>(EVENT_BACKLOG_SIZE);
        let ctx = Ctx::<S>::new(events_tx, &self.cancel, self.broker.clone());
        let mut service = S::start(&ctx, config).await?;

        loop {
            let input = tokio::select! {
                () = self.cancel.cancelled() => break,
                // Some(event) = events_rx.recv() => Input::Event(event),
                Some(input) = self.inbox.recv() => input,
                else => break,
            };
            service.handle(&ctx, input).await;
        }

        service.stop(&ctx).await;
        Ok(())
    }
}
