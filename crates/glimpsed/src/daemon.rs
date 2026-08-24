use clap::builder::Str;
use glimpse_config::Config;
use tokio::sync::watch;

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {}

struct ServiceDescriptor<S> {
    prefix: String,
    topics: Vec<String>,
    service: S,
}

pub struct Call {
    pub frame: CallFrame,
    pub out: ReplySink,
}

pub struct Daemon {
    config: Config,
    descriptors: Vec<ServiceDescriptor>,
}

impl Daemon {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            descriptors: Vec::new(),
        }
    }

    pub async fn run(self, socket: Listener) -> Result<(), DaemonError> {
        let broker = Broker::spawn();
        let shutdown = CancellationToken::new();

        let (calls_tx, calls_rx) = mpsc::channel::<Call>(256);

        shutdown.cancel();

        Ok(())
    }

    pub fn register<T: Service>(&mut self) {
        self.descriptors.push(ServiceDescriptor {
            prefix: T::prefix(),
            topics: T::topics(),
            service: T::new(),
        });
    }
}

async fn accept(socket: Listener) {}
