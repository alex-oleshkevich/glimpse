use std::pin::Pin;

use futures_util::{Stream, StreamExt, stream};
use glimpse_config::Provider as ConfiguredProvider;
use glimpse_contracts::{
    Command as _, GeoCoordinates, GeolocationRefresh, GeolocationStatus, Message,
};
use glimpse_dbus::geoclue::{GeoClueClientProxy, GeoClueLocationProxy, GeoClueManagerProxy};
use glimpse_ipc::CallError;
use serde_json::Value;
use zbus::{Connection, zvariant::OwnedObjectPath};

use crate::{
    context::{Ctx, SourceGuard},
    publisher::Publisher,
    service::{Input, Service, ServiceError, unknown_command},
};

/// The desktop id GeoClue authorizes against, and the section name of the shipped
/// `data/geoclue/conf.d/glimpse.conf`. The two have to agree, or the request falls through to
/// whatever agent is running — a prompt nobody is here to answer, or nothing at all.
const DESKTOP_ID: &str = "glimpse";

/// `GCLUE_ACCURACY_LEVEL_CITY`. Everything downstream of this service wants sunrise, sunset and
/// weather, none of which is sharper than a city, and asking for a street or an exact fix would
/// collect a precision nothing here can use.
const CITY_ACCURACY: u32 = 4;

/// GeoClue parks `Location` at the root path until it has a fix.
const NO_FIX: &str = "/";

#[derive(Debug, PartialEq)]
pub enum Provider {
    Geoclue,
    /// `None` when the table names `manual` without a usable pair of coordinates.
    Manual(Option<GeoCoordinates>),
}

#[derive(Debug)]
pub enum Command {
    Refresh,
}

pub enum Event {
    Located(Option<GeoCoordinates>),
    Unavailable(String),
}

#[derive(Debug, PartialEq)]
pub struct Config {
    provider: Provider,
}

pub struct Geolocation {
    status: Publisher<GeolocationStatus>,
    provider: Provider,
    geoclue: Option<SourceGuard>,
}

impl Service for Geolocation {
    const NAME: &'static str = "geolocation";
    const TOPICS: &'static [&'static str] = &[GeolocationStatus::NAME];
    const METHODS: &'static [&'static str] = &[GeolocationRefresh::NAME];

    type Config = Config;
    type Command = Command;
    type Event = Event;

    fn decode(method: &str, _args: Value) -> Result<Self::Command, CallError> {
        match method {
            GeolocationRefresh::NAME => Ok(Command::Refresh),
            _ => Err(unknown_command(Self::NAME, method)),
        }
    }

    async fn start(ctx: &Ctx<Self>, config: Self::Config) -> Result<Self, ServiceError> {
        let mut service = Self {
            status: ctx.publisher::<GeolocationStatus>(),
            provider: Provider::Manual(None),
            geoclue: None,
        };
        service.apply(ctx, config.provider);
        Ok(service)
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Located(coordinates)) => {
                if coordinates.is_some() {
                    ctx.running();
                }
                self.publish(coordinates);
            }
            // A fix that cannot be obtained is a degraded service, not a dead one: the daemon
            // keeps running and a manual `[location]` still works.
            Input::Event(Event::Unavailable(reason)) => {
                ctx.degraded(reason);
                self.publish(None);
            }
            Input::Config(config) => {
                if config.provider != self.provider {
                    self.apply(ctx, config.provider);
                }
            }
            Input::Command(Command::Refresh, responder) => {
                self.refresh(ctx);
                responder.ok(());
            }
        }
    }

    fn peek_config(config: &glimpse_config::Config) -> Self::Config {
        Config {
            provider: match config.location.provider {
                ConfiguredProvider::Geoclue => Provider::Geoclue,
                ConfiguredProvider::Manual => Provider::Manual(coordinates(
                    config.location.latitude,
                    config.location.longitude,
                )),
            },
        }
    }
}

impl Geolocation {
    fn apply(&mut self, ctx: &Ctx<Self>, provider: Provider) {
        // Dropping the guard stops the GeoClue watch, which is what makes switching to `manual`
        // release the backend rather than leave it running behind an unread stream.
        self.geoclue = None;

        match &provider {
            Provider::Manual(Some(coordinates)) => {
                ctx.running();
                self.publish(Some(coordinates.clone()));
            }
            Provider::Manual(None) => {
                ctx.degraded("`[location] provider = \"manual\"` needs latitude and longitude");
                self.publish(None);
            }
            Provider::Geoclue => {
                self.publish(None);
                self.geoclue = Some(ctx.stream(geoclue));
            }
        }
        self.provider = provider;
    }

    fn refresh(&mut self, ctx: &Ctx<Self>) {
        match &self.provider {
            Provider::Geoclue => self.geoclue = Some(ctx.stream(geoclue)),
            Provider::Manual(coordinates) => self.publish(coordinates.clone()),
        }
    }

    fn publish(&mut self, coordinates: Option<GeoCoordinates>) {
        self.status.set(GeolocationStatus { coordinates });
    }
}

/// Both coordinates or neither, and both in range. A half-configured or out-of-range pair is a
/// mistake worth reporting as one, not a silent fix at latitude zero off the coast of Africa.
fn coordinates(latitude: Option<f64>, longitude: Option<f64>) -> Option<GeoCoordinates> {
    let (latitude, longitude) = (latitude?, longitude?);
    ((-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude)).then_some(
        GeoCoordinates {
            latitude,
            longitude,
        },
    )
}

async fn geoclue(ctx: Ctx<Geolocation>) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
    match locations(&ctx).await {
        Ok(locations) => Box::pin(locations),
        Err(reason) => Box::pin(stream::once(async move { Event::Unavailable(reason) })),
    }
}

async fn locations(
    ctx: &Ctx<Geolocation>,
) -> Result<impl Stream<Item = Event> + Send + 'static, String> {
    let bus = ctx.system_bus().map_err(str::to_owned)?.clone();
    let manager = GeoClueManagerProxy::new(&bus).await.map_err(say)?;

    // GeoClue hands a caller back the client it already has; only the first call needs a new one.
    let path = match manager.get_client().await {
        Ok(path) => path,
        Err(_) => manager.create_client().await.map_err(say)?,
    };
    let client = GeoClueClientProxy::builder(&bus)
        .path(path)
        .map_err(say)?
        .build()
        .await
        .map_err(say)?;

    // Subscribed before `Start`, because the first fix can arrive before it returns.
    let updates = client.receive_location_changed().await;

    client.set_desktop_id(DESKTOP_ID).await.map_err(say)?;
    client
        .set_requested_accuracy_level(CITY_ACCURACY)
        .await
        .map_err(say)?;
    client.start().await.map_err(say)?;

    let known = client.location().await.ok();
    let first = {
        let bus = bus.clone();
        async move {
            match known {
                Some(path) => Event::Located(read(&bus, path).await),
                None => Event::Located(None),
            }
        }
    };

    let following = updates.then(move |change| {
        let bus = bus.clone();
        async move {
            match change.get().await {
                Ok(path) => Event::Located(read(&bus, path).await),
                Err(error) => Event::Unavailable(error.to_string()),
            }
        }
    });

    Ok(stream::once(first).chain(following))
}

async fn read(bus: &Connection, path: OwnedObjectPath) -> Option<GeoCoordinates> {
    if path.as_str() == NO_FIX {
        return None;
    }
    let location = GeoClueLocationProxy::builder(bus)
        .path(path)
        .ok()?
        .build()
        .await
        .ok()?;

    Some(GeoCoordinates {
        latitude: location.latitude().await.ok()?,
        longitude: location.longitude().await.ok()?,
    })
}

fn say(error: impl std::fmt::Display) -> String {
    error.to_string()
}
