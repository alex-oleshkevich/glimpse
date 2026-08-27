use std::{path::PathBuf, sync::Arc};

use crate::broker::{self, Handle, Message};
use crate::handler::BrokerHandler;
use crate::reload::{ConfigSink, Reloader};
use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_ipc::Server;
use glimpse_services::{BrokerHandle, Dispatch, Service, ServiceRuntime};
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    // Transparent: `ServerError::AlreadyRunning` names the path, and a message of our own here
    // would replace the only sentence that says what actually went wrong.
    #[error(transparent)]
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

type Factory = Box<dyn FnOnce(&InitService) -> ConfigSink + Send>;

/// Which services `--only` and `--without` leave running. An empty allowlist means everything, so
/// the two flags share one rule rather than each carrying its own idea of the default.
#[derive(Default)]
pub struct Filter {
    pub only: Vec<String>,
    pub without: Vec<String>,
}

impl Filter {
    fn allows(&self, name: &str) -> bool {
        match self.only.is_empty() {
            false => self.only.iter().any(|wanted| wanted == name),
            true => !self.without.iter().any(|refused| refused == name),
        }
    }

    fn unmatched<'a>(&'a self, registered: &'a [&'static str]) -> Vec<&'a String> {
        self.only
            .iter()
            .chain(&self.without)
            .filter(|name| !registered.contains(&name.as_str()))
            .collect()
    }
}

pub struct Daemon {
    tasks: TaskTracker,
    factories: Vec<Factory>,
    filter: Filter,
    known: Vec<&'static str>,
}

impl Daemon {
    pub fn new(filter: Filter) -> Self {
        Self {
            tasks: TaskTracker::new(),
            factories: Vec::new(),
            filter,
            known: Vec::new(),
        }
    }

    pub fn register<S: Service>(mut self) -> Self {
        self.known.push(S::NAME);
        // Excluded before anything is declared, so the service is absent from `system.services`
        // rather than listed as one that failed.
        if !self.filter.allows(S::NAME) {
            tracing::info!(service = S::NAME, "excluded by the command line");
            return self;
        }

        self.factories.push(Box::new(|init: &InitService| {
            let config = S::Config::from(&init.config);
            let mut previous = config.clone();
            let broker: Arc<dyn BrokerHandle> = Arc::new(init.broker.clone());
            let mut runtime =
                ServiceRuntime::<S>::new(broker, init.buses.clone(), init.cancel.child_token());

            // The one place the concrete service is still known, so the one place a `call` can be
            // turned into its command type — and the one place a reload can be narrowed to the
            // table this service owns.
            let reconfigure = runtime.sender();
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

            // A service whose own table did not move never learns a reload happened, so editing
            // `[[panels]]` cannot perturb the night light schedule.
            Box::new(move |document: &Config| {
                let next = S::Config::from(document);
                if next == previous {
                    return;
                }
                previous = next.clone();
                tracing::debug!(service = S::NAME, "reconfiguring");
                reconfigure.reconfigure(next);
            })
        }));
        self
    }

    pub async fn run(
        self,
        socket: &PathBuf,
        config: Config,
        config_path: Option<PathBuf>,
    ) -> Result<(), DaemonError> {
        tracing::info!("daemon starting");
        // A misspelt `--only` otherwise starts nothing at all and reads as a daemon that broke.
        for name in self.filter.unmatched(&self.known) {
            tracing::warn!(service = %name, "no such service; the name matches nothing");
        }
        let accepting = CancellationToken::new();
        let running = CancellationToken::new();
        let brokering = CancellationToken::new();

        let broker = broker::spawn(brokering.clone());
        // Bound before anything else is built. A second instance fails here, and every line below
        // this — a bus connection, a service task — is work it would otherwise do and then abandon
        // on the way out.
        let server = Server::bind(socket, BrokerHandler::new(broker.clone())).await?;

        // Before the factories run: a service may build a proxy the moment `start` is called.
        let buses = Buses::connect().await;
        let init = InitService {
            config,
            buses,
            cancel: running.clone(),
            tasks: self.tasks.clone(),
            broker: broker.clone(),
        };
        let sinks: Vec<ConfigSink> = self
            .factories
            .into_iter()
            .map(|factory| factory(&init))
            .collect();

        // After the factories, because the sinks are what they produce: the reloader has nothing
        // to hand a new document to until every service has been built.
        let hangup = signal(SignalKind::hangup())?;
        let reloader = Reloader::new(config_path, init.config, sinks);
        self.tasks
            .spawn(reloader.run(hangup, running.child_token()));

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

    tokio::select! {
        _ = terminate.recv() => {},
        _ = interrupt.recv() => {},
    }

    tracing::info!("shutting down");
    Ok(())
}
