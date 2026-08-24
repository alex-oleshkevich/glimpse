use crate::{
    context::{Ctx, SourceGuard},
    service::{Input, Service, StartError},
};

pub struct Coordinates {
    pub latitude: f64,
    pub longitude: f64,
}

pub enum Provider {
    Geoclue,
    Manual(Coordinates),
}

pub enum Command {
    Refresh,
}

enum Event {
    Changed(Coordinates),
}

pub struct Config {
    provider: Provider,
}

pub struct Geolocation {
    provider: Provider,
    coordinates: Option<Coordinates>,
    _handle: SourceGuard,
}

impl Service for Geolocation {
    type Config = Config;
    type Command = Command;
    type Event = Event;

    async fn start(ctx: &Ctx<Self>, config: Self::Config) -> Result<Self, StartError> {
        Ok(Self {
            provider: config.provider,
            _handle: ctx.spawn(async move {}),
            coordinates: match config.provider {
                Provider::Geoclue => None,
                Provider::Manual(coordinates) => Some(coordinates),
            },
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: crate::service::Input<Self>) {
        match input {
            Input::Event(_) => {}
            Input::Config(_) => {}
            Input::Command(cmd) => match cmd {
                Command::Refresh => {}
            },
        }
    }
}
