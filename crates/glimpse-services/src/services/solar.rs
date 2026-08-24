use tokio::time;

use crate::{
    context::{Ctx, SourceGuard},
    service::{Input, Service, StartError},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Config {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Command {
    Refresh,
}

pub enum Phase {
    Day,
    Night,
}

pub struct Solar {
    phase: Phase,
    config: Config,
    _tick: SourceGuard,
}
enum Event {
    Tick,
}

impl Service for Solar {
    type Config = Config;
    type Command = Command;
    type Event = Event;

    async fn start(ctx: &Ctx<Self>, config: Self::Config) -> Result<Self, StartError> {
        Ok(Self {
            config,
            phase: Phase::Day,
            _tick: ctx.at_interval(time::Instant::now(), time::Duration::from_mins(1), || {
                Event::Tick
            }),
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Tick) => {}
            Input::Config(config) => self.config = config,
            Input::Command(Command::Refresh) => {}
        }
    }
}
