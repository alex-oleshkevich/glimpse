use tokio::{
    sync::{oneshot, watch},
    time,
};

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, NoConfig, Service, ServiceEndpoint, ServiceError},
    subscription::Sub,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatInterval {
    pub previous_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeartbeatTick {
    pub count: u64,
}

const DEFAULT_PERIOD_MS: u64 = 1000;
const MIN_PERIOD_MS: u64 = 10;
const MAX_PERIOD_MS: u64 = 60_000;

pub enum Event {
    Tick,
}

#[derive(Debug)]
pub enum Command {
    Reset {
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    SetInterval {
        period_ms: u64,
        reply: oneshot::Sender<Result<HeartbeatInterval, CommandError>>,
    },
}

pub struct Heartbeat {
    tick: Publisher<HeartbeatTick>,
    count: u64,
    period_ms: u64,
}

#[derive(Clone)]
pub struct HeartbeatHandle(ServiceEndpoint<Heartbeat>);

impl HeartbeatHandle {
    pub fn snapshot(&self) -> HeartbeatTick {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> watch::Receiver<HeartbeatTick> {
        self.0.subscribe()
    }

    pub fn health(&self) -> watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    pub async fn reset(&self) -> Result<(), CommandError> {
        let (reply, answer) = oneshot::channel();
        self.0.command(Command::Reset { reply })?;
        answer.await.map_err(|_| {
            CommandError::Unavailable("`heartbeat` stopped before resetting".to_owned())
        })?
    }

    pub async fn set_interval(&self, period_ms: u64) -> Result<HeartbeatInterval, CommandError> {
        let (reply, answer) = oneshot::channel();
        self.0.command(Command::SetInterval { period_ms, reply })?;
        answer.await.map_err(|_| {
            CommandError::Unavailable("`heartbeat` stopped before changing its interval".to_owned())
        })?
    }
}

impl Heartbeat {}

#[derive(PartialEq, Eq, Hash)]
pub struct Tick {
    period_ms: u64,
}

impl Service for Heartbeat {
    const NAME: &'static str = "heartbeat";

    type Config = NoConfig;
    type State = HeartbeatTick;
    type Handle = HeartbeatHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Tick;

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
        HeartbeatHandle(endpoint)
    }

    fn initial_state(config: &Self::Config) -> Self::State {
        let _ = config;
        HeartbeatTick { count: 0 }
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        vec![Sub::interval(
            Tick {
                period_ms: self.period_ms,
            },
            time::Duration::from_millis(self.period_ms),
            |_ctx| async { Event::Tick },
        )]
    }

    async fn start(
        ctx: &Ctx<Self>,
        _config: Self::Config,
        _dependencies: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        tracing::debug!("starting heartbeat service");
        Ok(Self {
            count: 0,
            period_ms: DEFAULT_PERIOD_MS,
            tick: ctx.publisher(),
        })
    }

    async fn handle(&mut self, _ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Tick) => {
                self.count += 1;
                self.tick.set(HeartbeatTick { count: self.count });
            }
            Input::Command(Command::Reset { reply }) => {
                self.count = 0;
                self.tick.set(HeartbeatTick { count: 0 });
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::SetInterval { period_ms, reply }) => {
                let _ = reply.send(self.set_interval(period_ms));
            }
            Input::Config(NoConfig) => {}
        }
    }
}

impl Heartbeat {
    fn set_interval(&mut self, period_ms: u64) -> Result<HeartbeatInterval, CommandError> {
        if !(MIN_PERIOD_MS..=MAX_PERIOD_MS).contains(&period_ms) {
            return Err(CommandError::InvalidArgument(format!(
                "period_ms must be {MIN_PERIOD_MS}..={MAX_PERIOD_MS}, got {period_ms}"
            )));
        }

        // The period is part of the subscription key, so moving it is what restarts the timer.
        let previous_ms = std::mem::replace(&mut self.period_ms, period_ms);
        Ok(HeartbeatInterval { previous_ms })
    }
}

#[cfg(test)]
mod tests {
    use glimpse_dbus::Buses;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::service::ServiceRuntime;

    async fn running() -> (
        HeartbeatHandle,
        CancellationToken,
        tokio::task::JoinHandle<()>,
    ) {
        let cancel = CancellationToken::new();
        let (mut runtime, handle) = ServiceRuntime::<Heartbeat>::new(
            NoConfig,
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        let task = tokio::spawn(async move {
            let _ = runtime.run(()).await;
        });
        (handle, cancel, task)
    }

    #[tokio::test]
    async fn set_interval_reports_the_period_it_replaced() {
        let (handle, cancel, task) = running().await;
        let value = handle.set_interval(250).await.expect("accepted");
        assert_eq!(value.previous_ms, DEFAULT_PERIOD_MS);
        cancel.cancel();
        let _ = task.await;
    }

    /// A rejected period must not be retryable: retrying the same argument cannot start working.
    #[tokio::test]
    async fn set_interval_refuses_a_period_outside_the_supported_range() {
        let (handle, cancel, task) = running().await;
        let error = handle.set_interval(0).await.expect_err("refused");
        assert!(matches!(error, CommandError::InvalidArgument(_)));
        cancel.cancel();
        let _ = task.await;
    }
}
