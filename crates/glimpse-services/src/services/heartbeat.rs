use tokio::{sync::oneshot, time};

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, NoConfig, Service, ServiceEndpoint},
    subscription::Sub,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatInterval {
    pub previous_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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

pub type HeartbeatHandle = ServiceEndpoint<Heartbeat>;

impl ServiceEndpoint<Heartbeat> {
    pub async fn reset(&self) -> Result<(), CommandError> {
        self.call("`heartbeat` stopped before resetting", |reply| {
            Command::Reset { reply }
        })
        .await?
    }

    pub async fn set_interval(&self, period_ms: u64) -> Result<HeartbeatInterval, CommandError> {
        self.call(
            "`heartbeat` stopped before changing its interval",
            |reply| Command::SetInterval { period_ms, reply },
        )
        .await?
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
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Tick;

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
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
    ) -> Self {
        tracing::debug!("starting heartbeat service");
        Self {
            count: 0,
            period_ms: DEFAULT_PERIOD_MS,
            tick: ctx.publisher(),
        }
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
mod tests;
