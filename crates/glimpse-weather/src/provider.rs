use glimpse_dbus::weather::WatchedPlace;
use glimpse_dbus::weather::{
    GLIMPSE_WEATHER_BUS_NAME, GLIMPSE_WEATHER_OBJECT_PATH, WeatherSnapshot, encode_snapshot,
};
use glimpse_dbus::{Exported, Snapshot};
use glimpse_services::{CommandError, WeatherHandle};
use zbus::{Connection, DBusError};

#[derive(Debug, DBusError)]
#[zbus(prefix = "me.aresa.Glimpse.Weather1.Error", impl_display = true)]
pub enum Error {
    InvalidPlace(String),
    LimitExceeded(String),
    Unavailable(String),
    #[zbus(error)]
    ZBus(zbus::Error),
}

pub(crate) struct Provider {
    weather: WeatherHandle,
}

impl Snapshot for Provider {
    async fn emit_snapshot_changed(
        &self,
        emitter: &zbus::object_server::SignalEmitter<'_>,
    ) -> zbus::Result<()> {
        self.snapshot_changed(emitter).await
    }
}

#[zbus::interface(name = "me.aresa.Glimpse.Weather1")]
impl Provider {
    #[zbus(property)]
    fn snapshot(&self) -> WeatherSnapshot {
        snapshot(&self.weather)
    }

    async fn watch_place(
        &self,
        kind: u8,
        latitude: f64,
        longitude: f64,
        location: &str,
    ) -> Result<(), Error> {
        let place = watched_place(kind, latitude, longitude, location)?;
        tracing::debug!(place_kind = kind_name(kind), "weather watch requested");
        self.weather.watch(place).await.map_err(Error::from)
    }

    async fn refresh(&self) -> Result<(), Error> {
        tracing::debug!("weather refresh requested");
        self.weather.refresh().await.map_err(Error::from)
    }
}

fn watched_place(
    kind: u8,
    latitude: f64,
    longitude: f64,
    location: &str,
) -> Result<WatchedPlace, Error> {
    match kind {
        0 => Ok(WatchedPlace::Here),
        1 => Ok(WatchedPlace::Coordinates {
            latitude,
            longitude,
        }),
        2 => Ok(WatchedPlace::Location {
            name: location.to_owned(),
        }),
        _ => Err(Error::InvalidPlace(format!("unknown place kind {kind}"))),
    }
}

impl From<CommandError> for Error {
    fn from(error: CommandError) -> Self {
        match error {
            CommandError::InvalidArgument(reason) => Self::InvalidPlace(reason),
            CommandError::LimitExceeded(reason) => Self::LimitExceeded(reason),
            error => Self::Unavailable(error.to_string()),
        }
    }
}

pub(crate) type Runtime = Exported<Provider>;

pub(crate) async fn start(connection: Connection, weather: WeatherHandle) -> zbus::Result<Runtime> {
    Exported::serve(
        connection,
        GLIMPSE_WEATHER_BUS_NAME,
        GLIMPSE_WEATHER_OBJECT_PATH,
        Provider {
            weather: weather.clone(),
        },
        weather.subscribe(),
        weather.health(),
    )
    .await
}

fn snapshot(weather: &WeatherHandle) -> WeatherSnapshot {
    let status = weather.snapshot();
    let health = weather.health();
    let health = health.borrow();
    let available = !status.places.is_empty();
    let unavailable = health.unavailable_reason();
    let stale = available && unavailable.is_some();
    let reason = unavailable.or((!available).then_some("weather has no successful reading yet"));
    encode_snapshot(&status, available, stale, reason)
}

fn kind_name(kind: u8) -> &'static str {
    match kind {
        0 => "here",
        1 => "coordinates",
        2 => "location",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests;
