use crate::{
    context::{Ctx, SourceGuard},
    service::{Input, Service, ServiceError},
};
use futures_util::{Stream, stream};
use glimpse_config::Provider as GeolocationProvider;
use glimpse_contracts::{GeoCoordinates, GeolocationStatus, Message};
use glimpse_dbus::geoclue::{GeoClueClientProxy, GeoClueManagerProxy};
use tokio_util::sync::CancellationToken;

const DESKTOP_ID: &str = "glimpse";
const EXACT_ACCURACY: u32 = 8;

#[derive(Debug, thiserror::Error)]
enum GeolocationError {
    #[error("geoclue error: {0}")]
    GeoClueClientError(String),
}

#[derive(Debug, PartialEq, Clone)]
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
    coordinates: Option<GeoCoordinates>,
    _handle: Option<SourceGuard>,
}

impl Service for Geolocation {
    const NAME: &'static str = "geolocation";
    const TOPICS: &'static [&'static str] = &[GeolocationStatus::NAME];

    type Config = Config;
    type Command = Command;
    type Event = Event;

    async fn start(ctx: &Ctx<Self>, config: Self::Config) -> Result<Self, ServiceError> {
        tracing::debug!("starting geolocation service");
        Ok(Self {
            coordinates: match &config.provider {
                Provider::Geoclue => None,
                Provider::Manual(coordinates) => Some(coordinates.clone()),
            },
            _handle: match &config.provider {
                Provider::Geoclue => Some(ctx.stream(geoclue_events)),
                Provider::Manual(_) => None,
            },
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: crate::service::Input<Self>) {
        match input {
            Input::Event(event) => match event {
                Event::Changed(coords) => self.coordinates = Some(coords),
            },
            Input::Config(config) => match config.provider {
                Provider::Geoclue => self._handle = Some(ctx.stream(geoclue_events)),
                Provider::Manual(coords) => self.coordinates = Some(coords),
            },
            Input::Command(cmd, responder) => match cmd {
                Command::Refresh => responder.ok(()),
            },
        }
    }

    fn peek_config(config: &glimpse_config::Config) -> Self::Config {
        Config {
            provider: match config.location.provider {
                GeolocationProvider::Geoclue => Provider::Geoclue,
                GeolocationProvider::Manual => Provider::Manual(
                    match (config.location.latitude, config.location.longitude) {
                        (Some(latitude), Some(longitude)) => GeoCoordinates {
                            latitude,
                            longitude,
                        },
                        _ => {
                            tracing::warn!(
                                "manual geolocation provider requires both latitude and longitude to be set"
                            );
                            GeoCoordinates {
                                latitude: 0.0,
                                longitude: 0.0,
                            }
                        }
                    },
                ),
            },
        }
    }
}

async fn geoclue_events(_ctx: Ctx<Geolocation>) -> impl Stream<Item = Event> {
    // let manager = GeoClueManagerProxy::new(&dbus)
    //     .await
    //     .map_err(|e| GeolocationError::GeoClueClientError(e.to_string()))?;
    // let path = match manager.get_client().await {
    //     Ok(path) => path,
    //     Err(_) => manager
    //         .create_client()
    //         .await
    //         .map_err(|e| GeolocationError::GeoClueClientError(e.to_string()))?,
    // };

    // let proxy = GeoClueClientProxy::builder(&dbus)
    //     .path(path.clone())
    //     .map_err(|e| GeolocationError::GeoClueClientError(e.to_string()))?
    //     .build()
    //     .await
    //     .map_err(|e| GeolocationError::GeoClueClientError(e.to_string()))?;

    // let mut location_changes = proxy.receive_location_changed().await;
    // proxy.set_desktop_id(DESKTOP_ID).await.map_err(|e| {
    //     GeolocationError::GeoClueClientError(format!("failed to set desktop id: {}", e))
    // })?;
    // proxy
    //     .set_requested_accuracy_level(EXACT_ACCURACY)
    //     .await
    //     .map_err(|e| {
    //         GeolocationError::GeoClueClientError(format!("failed to set accuracy level: {}", e))
    //     })?;
    // proxy.start().await.map_err(|e| {
    //     GeolocationError::GeoClueClientError(format!("failed to start geoclue client: {}", e))
    // })?;

    // let location_task = tokio::spawn(async move {
    //     tokio::select! {
    //         _ = () => cancel.cancelled().await,
    //         update = () => stream_location_changes().await,
    //     }
    //     while location_changes.next().await.is_some() {
    //         if sender.send(()).await.is_err() {
    //             break;
    //         }
    //     }
    // });

    // location_task.await.map_err(|e| {
    //     GeolocationError::GeoClueClientError(format!("failed to receive location changes: {}", e))
    // })?;
    stream::empty()
}

async fn stream_location_changes() {}
