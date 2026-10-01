use std::pin::Pin;
use std::time::Duration;

use futures_util::stream::BoxStream;
use futures_util::{Stream, StreamExt, stream};
use glimpse_config::Geolocation as ConfiguredGeolocation;
use glimpse_dbus::geoclue::{GeoClueClientProxy, GeoClueLocationProxy, GeoClueManagerProxy};
use glimpse_dbus::weather::GeoCoordinates;
use tokio::sync::{oneshot, watch};
use tokio::time::{sleep, timeout};
use zbus::{Connection, zvariant::OwnedObjectPath};

use super::say;
use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, Service, ServiceEndpoint, ServiceError},
    subscription::Sub,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GeolocationStatus {
    pub coordinates: Option<GeoCoordinates>,
}

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

const FIX_TIMEOUT: Duration = Duration::from_secs(30);
const NO_FIX_RETRY: Duration = Duration::from_secs(45);
const FIX_REFRESH: Duration = Duration::from_secs(1800);

#[derive(Debug, Clone, PartialEq)]
pub enum Provider {
    Geoclue,
    /// `None` when the table names coordinates outside their ranges. Presence is not a case here:
    /// a `manual` table missing either key never loads.
    Manual(Option<GeoCoordinates>),
}

#[derive(Debug)]
pub enum Command {
    Refresh {
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
}

#[derive(Debug)]
pub enum Event {
    Located(Option<GeoCoordinates>),
    Unavailable(String),
    Retry,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub(crate) provider: Provider,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            provider: match document.geolocation {
                ConfiguredGeolocation::Geoclue => Provider::Geoclue,
                ConfiguredGeolocation::Manual {
                    latitude,
                    longitude,
                } => Provider::Manual(coordinates(latitude, longitude)),
            },
        }
    }
}

pub struct Geolocation {
    status: Publisher<GeolocationStatus>,
    provider: Provider,
    attempt: u64,
    has_fix: bool,
}

#[derive(Clone)]
pub struct GeolocationHandle(ServiceEndpoint<Geolocation>);

impl GeolocationHandle {
    pub fn snapshot(&self) -> GeolocationStatus {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> watch::Receiver<GeolocationStatus> {
        self.0.subscribe()
    }

    pub fn health(&self) -> watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    pub async fn refresh(&self) -> Result<(), CommandError> {
        let (reply, answer) = oneshot::channel();
        self.0.command(Command::Refresh { reply })?;
        answer.await.map_err(|_| {
            CommandError::Unavailable("`geolocation` stopped before refreshing".to_owned())
        })?
    }
}

impl Geolocation {}

/// `attempt` carries nothing but its own difference: `geolocation.refresh` has no parameter to
/// change, and a key that does not move would leave the watch running untouched.
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Watch {
    Geoclue { attempt: u64 },
    Retry { fixed: bool },
}

impl Service for Geolocation {
    const NAME: &'static str = "geolocation";

    type Config = Config;
    type State = GeolocationStatus;
    type Handle = GeolocationHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Watch;

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
        GeolocationHandle(endpoint)
    }

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        match self.provider {
            Provider::Geoclue => {
                let period = if self.has_fix {
                    FIX_REFRESH
                } else {
                    NO_FIX_RETRY
                };
                vec![
                    Sub::stream(
                        Watch::Geoclue {
                            attempt: self.attempt,
                        },
                        geoclue,
                    ),
                    Sub::stream(
                        Watch::Retry {
                            fixed: self.has_fix,
                        },
                        move |_ctx| async move { retry_ticks(period) },
                    ),
                ]
            }
            Provider::Manual(_) => Vec::new(),
        }
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        _dependencies: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        let mut service = Self {
            status: ctx.publisher(),
            provider: Provider::Manual(None),
            attempt: 0,
            has_fix: false,
        };
        service.apply(ctx, config.provider);
        Ok(service)
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(_) if !matches!(self.provider, Provider::Geoclue) => {}
            Input::Event(Event::Located(coordinates)) => {
                if coordinates.is_some() {
                    ctx.running();
                }
                self.publish(coordinates);
            }
            // A fix that cannot be obtained is a degraded service, not a dead one: the daemon
            // keeps running and a manual `[geolocation]` still works.
            Input::Event(Event::Unavailable(reason)) => {
                ctx.degraded(reason);
            }
            Input::Event(Event::Retry) => self.refresh(),
            Input::Config(config) => {
                if config.provider != self.provider {
                    self.apply(ctx, config.provider);
                }
            }
            Input::Command(Command::Refresh { reply }) => {
                self.refresh();
                let _ = reply.send(Ok(()));
            }
        }
    }
}

impl Geolocation {
    fn apply(&mut self, ctx: &Ctx<Self>, provider: Provider) {
        match &provider {
            Provider::Manual(Some(coordinates)) => {
                ctx.running();
                self.publish(Some(coordinates.clone()));
            }
            Provider::Manual(None) => {
                ctx.degraded("`[geolocation]` coordinates are out of range");
                self.publish(None);
            }
            Provider::Geoclue => self.publish(None),
        }
        self.provider = provider;
    }

    fn refresh(&mut self) {
        match &self.provider {
            Provider::Geoclue => self.attempt += 1,
            Provider::Manual(coordinates) => self.publish(coordinates.clone()),
        }
    }

    fn publish(&mut self, coordinates: Option<GeoCoordinates>) {
        self.has_fix = coordinates.is_some();
        self.status.set(GeolocationStatus { coordinates });
    }
}

/// Presence is the configuration's job — a `manual` table missing either key is refused before the
/// document loads. What is left is range: a mistyped latitude is a mistake worth reporting as one,
/// not a silent fix at zero off the coast of Africa.
fn coordinates(latitude: f64, longitude: f64) -> Option<GeoCoordinates> {
    ((-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude)).then_some(
        GeoCoordinates {
            latitude,
            longitude,
        },
    )
}

async fn geoclue(ctx: Ctx<Geolocation>) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
    match locations(&ctx).await {
        Ok(event) => Box::pin(stream::iter(event)),
        Err(reason) => Box::pin(stream::once(async move { Event::Unavailable(reason) })),
    }
}

async fn locations(ctx: &Ctx<Geolocation>) -> Result<Option<Event>, String> {
    let bus = ctx.system_bus().map_err(str::to_owned)?.clone();
    match timeout(FIX_TIMEOUT, attempt(ctx.clone(), bus)).await {
        Ok(outcome) => outcome,
        Err(_) => Ok(None),
    }
}

async fn attempt(ctx: Ctx<Geolocation>, bus: Connection) -> Result<Option<Event>, String> {
    let manager = GeoClueManagerProxy::new(&bus).await.map_err(say)?;

    // GeoClue hands a caller back the client it already has; only the first call needs a new one.
    let path = match manager.get_client().await {
        Ok(path) => path,
        Err(_) => manager.create_client().await.map_err(say)?,
    };
    let client = GeoClueClientProxy::builder(&bus)
        .path(path.clone())
        .map_err(say)?
        .build()
        .await
        .map_err(say)?;

    let transient = {
        let manager = manager.clone();
        let client = client.clone();
        TransientClient::new(ctx, move || async move {
            let _ = client.stop().await;
            let _ = manager.delete_client(path).await;
        })
    };

    // Subscribed before `Start`, because the first fix can arrive before it returns.
    let updates = client.receive_location_changed().await;

    client.set_desktop_id(DESKTOP_ID).await.map_err(say)?;
    client
        .set_requested_accuracy_level(CITY_ACCURACY)
        .await
        .map_err(say)?;
    client.start().await.map_err(say)?;

    let decoded: BoxStream<'static, Result<Option<GeoCoordinates>, String>> = {
        let bus = bus.clone();
        Box::pin(updates.then(move |change| {
            let bus = bus.clone();
            async move {
                match change.get().await {
                    Ok(path) => Ok(read(&bus, path).await),
                    Err(error) => Err(error.to_string()),
                }
            }
        }))
    };

    let event = first_fix(decoded).await;
    transient.release().await;

    Ok(event)
}

async fn first_fix(
    mut updates: BoxStream<'static, Result<Option<GeoCoordinates>, String>>,
) -> Option<Event> {
    loop {
        match updates.next().await? {
            Ok(Some(coordinates)) => return Some(Event::Located(Some(coordinates))),
            Ok(None) => continue,
            Err(error) => return Some(Event::Unavailable(error)),
        }
    }
}

type ReleaseEffect = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

struct TransientClient {
    ctx: Ctx<Geolocation>,
    release: Option<ReleaseEffect>,
}

impl TransientClient {
    fn new<F, Fut>(ctx: Ctx<Geolocation>, release: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        Self {
            ctx,
            release: Some(Box::new(move || Box::pin(release()))),
        }
    }

    async fn release(mut self) {
        if let Some(release) = self.release.take() {
            release().await;
        }
    }
}

impl Drop for TransientClient {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            self.ctx.spawn_detached(move |_ctx| release());
        }
    }
}

fn retry_ticks(period: Duration) -> impl Stream<Item = Event> {
    stream::unfold((), move |()| async move {
        sleep(period).await;
        Some((Event::Retry, ()))
    })
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

#[cfg(test)]
mod tests;
