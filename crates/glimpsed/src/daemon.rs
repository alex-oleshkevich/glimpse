use std::{path::PathBuf, sync::Arc};

use crate::broker::{self, Handle, Message};
use crate::handler::BrokerHandler;
use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_ipc::Server;
use glimpse_services::{BrokerHandle, Dispatch, Service, ServiceRuntime};
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("failed to start ipc server")]
    IpcServer(#[from] glimpse_ipc::ServerError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("load config: {0}")]
    Config(String),
    #[error("socket: {0}")]
    Socket(String),
    #[error("runtime: {0}")]
    Runtime(String),
}

struct InitService {
    tasks: TaskTracker,
    cancel: CancellationToken,
    broker: Handle,
    buses: Buses,
    config: Config,
}

type Factory = Box<dyn FnOnce(&InitService) + Send>;

pub struct Daemon {
    tasks: TaskTracker,
    factories: Vec<Factory>,
}

impl Daemon {
    pub fn new() -> Self {
        Self {
            tasks: TaskTracker::new(),
            factories: Vec::new(),
        }
    }

    pub fn register<S: Service>(mut self) -> Self {
        self.factories.push(Box::new(|init: &InitService| {
            let config = S::peek_config(&init.config);
            let broker: Arc<dyn BrokerHandle> = Arc::new(init.broker.clone());
            let mut runtime =
                ServiceRuntime::<S>::new(broker, init.buses.clone(), init.cancel.child_token());

            // The one place the concrete service is still known, so the one place a `call` can be
            // turned into its command type.
            let sender = runtime.sender();
            let dispatch: Dispatch =
                Box::new(
                    move |method, args, responder| match S::decode(method, args) {
                        Ok(command) => sender.dispatch(command, responder),
                        Err(error) => responder.fail(error),
                    },
                );

            // Declared before the service starts, so a client can subscribe to a topic of a
            // service that has not published yet and be told it matched.
            init.broker.send(Message::Declare {
                service: S::NAME,
                topics: S::TOPICS,
                methods: S::METHODS,
                dispatch,
            });

            init.tasks.spawn(async move {
                if let Err(err) = runtime.run(config).await {
                    tracing::error!("service stopped: {}", err);
                }
            });
        }));
        self
    }

    pub async fn run(self, socket: &PathBuf, config: Config) -> Result<(), DaemonError> {
        tracing::info!("daemon starting");
        let accepting = CancellationToken::new();
        let running = CancellationToken::new();
        let brokering = CancellationToken::new();

        let broker = broker::spawn(brokering.clone());
        // Before the factories run: a service may build a proxy the moment `start` is called.
        let buses = Buses::connect().await;
        let init = InitService {
            config,
            buses,
            cancel: running.clone(),
            tasks: self.tasks.clone(),
            broker: broker.clone(),
        };
        for factory in self.factories {
            factory(&init);
        }

        let server = Server::bind(socket, BrokerHandler::new(broker.clone())).await?;
        // The publisher only exists once the socket is bound, and the broker only routes to
        // clients once it has one. Anything published before this is stored and delivered as a
        // snapshot to the first client that asks, which is what a state cell is for.
        broker.send(Message::SetPublisher(server.publisher()));
        tracing::info!(path=?socket,"daemon listening");
        let serving = tokio::spawn(server.serve(accepting.clone()));
        shutdown_signal().await?;

        accepting.cancel();
        if let Err(err) = serving.await {
            return Err(DaemonError::Runtime(err.to_string()));
        }

        running.cancel();
        self.tasks.close();
        self.tasks.wait().await;

        brokering.cancel();
        Ok(())
    }
}

async fn shutdown_signal() -> Result<(), DaemonError> {
    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut hangup = signal(SignalKind::hangup())?;

    loop {
        tokio::select! {
            _ = terminate.recv() => break,
            _ = interrupt.recv() => break,
            _ = hangup.recv() => tracing::info!("reload is not wired yet"),
        }
    }

    tracing::info!("shutting down");
    Ok(())
}
