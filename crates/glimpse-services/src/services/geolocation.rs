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
    context::Ctx,
    publisher::Publisher,
    service::{Input, Service, ServiceError, unknown_command},
    subscription::Sub,
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
    attempt: u64,
}

/// `attempt` carries nothing but its own difference: `geolocation.refresh` has no parameter to
/// change, and a key that does not move would leave the watch running untouched.
#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    Geoclue { attempt: u64 },
}

impl Service for Geolocation {
    const NAME: &'static str = "geolocation";
    const TOPICS: &'static [&'static str] = &[GeolocationStatus::NAME];
    const METHODS: &'static [&'static str] = &[GeolocationRefresh::NAME];

    type Config = Config;
    type Command = Command;
    type Event = Event;
    type SubKey = Watch;

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        match self.provider {
            Provider::Geoclue => vec![Sub::stream(
                Watch::Geoclue {
                    attempt: self.attempt,
                },
                geoclue,
            )],
            Provider::Manual(_) => Vec::new(),
        }
    }

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
            attempt: 0,
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
                self.refresh();
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
        match &provider {
            Provider::Manual(Some(coordinates)) => {
                ctx.running();
                self.publish(Some(coordinates.clone()));
            }
            Provider::Manual(None) => {
                ctx.degraded("`[location] provider = \"manual\"` needs latitude and longitude");
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glimpse_dbus::Buses;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{BrokerHandle, MockBroker, service::ServiceRuntime};

    fn manual(latitude: f64, longitude: f64) -> Config {
        Config {
            provider: Provider::Manual(coordinates(Some(latitude), Some(longitude))),
        }
    }

    fn published(mock: &MockBroker) -> Vec<Option<GeoCoordinates>> {
        mock.published()
            .into_iter()
            .filter(|(topic, _)| topic == GeolocationStatus::NAME)
            .filter_map(|(_, data)| serde_json::from_value::<GeolocationStatus>(data).ok())
            .map(|status| status.coordinates)
            .collect()
    }

    /// A watch that has been torn down can still have an event waiting in the inbox behind the
    /// configuration that tore it down.
    #[tokio::test]
    async fn a_geoclue_event_arriving_after_a_switch_to_manual_is_ignored() {
        let mock = Arc::new(MockBroker::default());
        let broker: Arc<dyn BrokerHandle> = mock.clone();
        let cancel = CancellationToken::new();
        let mut runtime = ServiceRuntime::<Geolocation>::new(
            broker,
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );

        let sender = runtime.sender();
        sender
            .send(Input::Config(manual(51.5074, -0.1278)))
            .await
            .expect("queued");
        sender
            .send(Input::Event(Event::Located(Some(GeoCoordinates {
                latitude: 52.2297,
                longitude: 21.0122,
            }))))
            .await
            .expect("queued");

        let running = tokio::spawn(async move {
            let _ = runtime
                .run(Config {
                    provider: Provider::Geoclue,
                })
                .await;
        });
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        cancel.cancel();
        let _ = running.await;

        let coordinates = published(&mock);
        assert_eq!(
            coordinates.last(),
            Some(&Some(GeoCoordinates {
                latitude: 51.5074,
                longitude: -0.1278,
            })),
            "the manual pair must survive the straggler, got {coordinates:?}"
        );
    }
}
