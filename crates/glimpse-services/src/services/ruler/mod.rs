mod history;
mod runner;

use std::sync::Arc;

use tokio::sync::oneshot;

use crate::{
    context::{Ctx, SourceGuard},
    publisher::Publisher,
    selection::{Offer, Selection},
    service::{CommandError, Input, Service, ServiceEndpoint},
};

use history::History;
pub use history::Measurement;
pub use runner::{Measured, ProcessRunner, Runner};

const TEXT: &str = "text/plain;charset=utf-8";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    limit: usize,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            limit: document.ruler.history_limit,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RulerState {
    pub history: Vec<Measurement>,
    pub measuring: bool,
}

pub enum Command {
    Measure {
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Copy {
        id: u32,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
}

pub enum Event {
    Measured(Result<Vec<Measurement>, String>),
}

pub struct Dependencies {
    pub selection: Arc<dyn Selection>,
    pub runner: Arc<dyn Runner>,
}

pub type RulerHandle = ServiceEndpoint<Ruler>;

impl ServiceEndpoint<Ruler> {
    pub async fn measure(&self) -> Result<(), CommandError> {
        self.call(|reply| Command::Measure { reply }).await?
    }

    pub async fn copy(&self, id: u32) -> Result<(), CommandError> {
        self.call(|reply| Command::Copy { id, reply }).await?
    }
}

pub struct Ruler {
    state: Publisher<RulerState>,
    selection: Arc<dyn Selection>,
    runner: Arc<dyn Runner>,
    history: History,
    pending: Option<(SourceGuard, oneshot::Sender<Result<(), CommandError>>)>,
}

impl Service for Ruler {
    const NAME: &'static str = "ruler";

    type Config = Config;
    type State = RulerState;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = ();

    fn command_stopped_reason(command: &Self::Command) -> &'static str {
        match command {
            Command::Measure { .. } => "the ruler stopped while measuring",
            Command::Copy { .. } => "the ruler stopped while copying",
        }
    }

    fn initial_state(_config: &Self::Config) -> Self::State {
        RulerState {
            history: Vec::new(),
            measuring: false,
        }
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> Self {
        Self {
            state: ctx.publisher(),
            selection: dependencies.selection,
            runner: dependencies.runner,
            history: History::new(config.limit),
            pending: None,
        }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(Command::Measure { reply }) => {
                if self.pending.is_some() {
                    let _ = reply.send(Err(CommandError::LimitExceeded(
                        "a measurement is already in progress".to_owned(),
                    )));
                    return;
                }
                let runner = Arc::clone(&self.runner);
                let guard =
                    ctx.spawn(move |_ctx| async move { Event::Measured(runner.measure().await) });
                self.pending = Some((guard, reply));
            }
            Input::Command(Command::Copy { id, reply }) => {
                let _ = reply.send(self.copy(id));
            }
            Input::Event(Event::Measured(outcome)) => {
                let Some((_guard, reply)) = self.pending.take() else {
                    return;
                };
                let _ = reply.send(self.measured(outcome));
            }
            Input::Config(_) => return,
        }
        self.publish();
    }
}

impl Ruler {
    fn measured(&mut self, outcome: Result<Vec<Measurement>, String>) -> Result<(), CommandError> {
        let measurements = outcome.map_err(CommandError::Unavailable)?;
        self.history.push_all(measurements);
        Ok(())
    }

    fn copy(&self, id: u32) -> Result<(), CommandError> {
        let entry = self.history.get(id).ok_or_else(|| {
            CommandError::InvalidArgument("that measurement is no longer in history".to_owned())
        })?;
        self.offer(format!("{:.1}px", entry.distance))
    }

    fn offer(&self, text: String) -> Result<(), CommandError> {
        self.selection
            .offer(Offer {
                mime: TEXT.to_owned(),
                data: Arc::from(text.into_bytes()),
            })
            .map_err(CommandError::Unavailable)
    }

    fn publish(&self) {
        self.state.set(RulerState {
            history: self.history.entries(),
            measuring: self.pending.is_some(),
        });
    }
}

#[cfg(test)]
mod tests;
