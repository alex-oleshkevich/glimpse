use glimpse_contracts::{HeartbeatTick, Message};
use tokio::time;

use crate::{
    context::{Ctx, SourceGuard},
    publisher::Publisher,
    service::{Input, Service, ServiceError},
};

const TICK: time::Duration = time::Duration::from_secs(1);

pub enum Event {
    Tick,
}

/// A development fixture: the one service that publishes on its own, so `get`, `topics` and `watch`
/// have something live to show before any real service works. The counter is what makes it visible
/// — an unchanging payload would be swallowed by the publisher's equality gate and nothing would
/// arrive after the first tick.
pub struct Heartbeat {
    tick: Publisher<HeartbeatTick>,
    count: u64,
    _timer: SourceGuard,
}

impl Service for Heartbeat {
    const NAME: &'static str = "heartbeat";
    const TOPICS: &'static [&'static str] = &[HeartbeatTick::NAME];

    type Config = ();
    type Command = ();
    type Event = Event;

    async fn start(ctx: &Ctx<Self>, _config: Self::Config) -> Result<Self, ServiceError> {
        tracing::debug!("starting heartbeat service");
        Ok(Self {
            count: 0,
            tick: ctx.publisher::<HeartbeatTick>(),
            _timer: ctx.interval(TICK, || Event::Tick),
        })
    }

    async fn handle(&mut self, _ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Tick) => {
                self.count += 1;
                self.tick.set(HeartbeatTick { count: self.count });
            }
            Input::Command(()) | Input::Config(()) => {}
        }
    }

    fn peek_config(_config: &glimpse_config::Config) -> Self::Config {}
}
