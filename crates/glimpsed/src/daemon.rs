use std::{path::PathBuf, sync::Arc};

use glimpse_config::Config;
use glimpse_ipc::Server;
use glimpse_services::{Broker, BrokerHandle, Service, ServiceRuntime};
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("failed to start ipc server")]
    IpcServer(#[from] glimpse_ipc::ServerError),
    #[error("failed to start message broker")]
    MessageBroker(#[from] glimpse_services::BrokerError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

struct InitService {
    tasks: TaskTracker,
    cancel: CancellationToken,
    broker: Arc<BrokerHandle>,
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
            let mut runtime =
                ServiceRuntime::<S>::new(init.broker.clone(), init.cancel.child_token());
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

        let broker = Broker::spawn(brokering.clone()).await?;
        let init = InitService {
            config,
            cancel: running.clone(),
            tasks: self.tasks.clone(),
            broker: Arc::new(broker.handle()),
        };
        for factory in self.factories {
            factory(&init);
        }

        let server = Server::bind(socket).await?;
        tracing::info!(path=?socket,"daemon listening");
        let serving = tokio::spawn(server.serve(accepting.clone()));
        shutdown_signal().await?;

        accepting.cancel();
        let _ = serving.await;

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
