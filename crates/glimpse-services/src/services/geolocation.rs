use crate::{
    context::{Ctx, SourceGuard},
    service::{Input, Service, ServiceError},
};
use glimpse_config::Provider as GeolocationProvider;
use glimpse_contracts::GeoCoordinates;

#[derive(Debug, PartialEq)]
pub enum Provider {
    Geoclue,
    Manual(GeoCoordinates),
}

#[derive(Debug)]
pub enum Command {
    Refresh,
}

pub enum Event {
    Changed(GeoCoordinates),
}

#[derive(Debug, PartialEq)]
pub struct Config {
    provider: Provider,
}

pub struct Geolocation {
    provider: Provider,
    coordinates: Option<GeoCoordinates>,
    _handle: SourceGuard,
}

impl Service for Geolocation {
    type Config = Config;
    type Command = Command;
    type Event = Event;

    async fn start(ctx: &Ctx<Self>, config: Self::Config) -> Result<Self, ServiceError> {
        Ok(Self {
            coordinates: match &config.provider {
                Provider::Geoclue => None,
                Provider::Manual(coordinates) => Some(coordinates.clone()),
            },
            provider: config.provider,
            _handle: ctx.spawn(async move {}),
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

    fn peek_config(config: &glimpse_config::Config) -> Self::Config {
        Config {
            provider: match config.location.provider {
                GeolocationProvider::Geoclue => Provider::Geoclue,
                GeolocationProvider::Manual => Provider::Manual(GeoCoordinates {
                    latitude: 0.0,
                    longitude: 0.0,
                }),
            },
        }
    }
}
