use glimpse_dbus::Exported;
use glimpse_dbus::weather::WatchedPlace;
use glimpse_dbus::weather::{
    GLIMPSE_WEATHER_BUS_NAME, GLIMPSE_WEATHER_OBJECT_PATH, WeatherSnapshot, encode_snapshot,
};
use glimpse_services::ServiceState;
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

#[zbus::interface(name = "me.aresa.Glimpse.Weather1")]
impl Provider {
    #[zbus(property)]
    fn snapshot(&self) -> WeatherSnapshot {
        snapshot(&self.weather)
    }

    async fn watch_place(
        &self,
        kind: u8,       // 0 here, 1 coordinates, 2 named location
        latitude: f64,  // degrees north, ignored for kinds 0 and 2
        longitude: f64, // degrees east, ignored for kinds 0 and 2
        location: &str, // requested name, used only for kind 2
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
    Exported::start(
        connection.clone(),
        GLIMPSE_WEATHER_BUS_NAME,
        GLIMPSE_WEATHER_OBJECT_PATH,
        Provider {
            weather: weather.clone(),
        },
        follow_changes(connection, weather),
    )
    .await
}

async fn follow_changes(connection: Connection, weather: WeatherHandle) {
    let mut state = weather.subscribe();
    let mut health = weather.health();
    let mut previous_health = health.borrow().clone();
    loop {
        tokio::select! {
            changed = state.changed() => {
                if changed.is_err() {
                    return;
                }
                let state = state.borrow_and_update();
                tracing::debug!(places = state.places.len(), "weather provider state changed");
            }
            changed = health.changed() => {
                if changed.is_err() {
                    return;
                }
                let current = health.borrow_and_update().clone();
                if current != previous_health {
                    match &current {
                        ServiceState::Running => tracing::info!("weather service is running"),
                        ServiceState::Starting => tracing::debug!("weather service is starting"),
                        ServiceState::Degraded { .. } => tracing::warn!("weather service is degraded"),
                        ServiceState::Stopped { .. } => tracing::warn!("weather service stopped"),
                    }
                    previous_health = current;
                }
            }
        }
        let interface = match connection
            .object_server()
            .interface::<_, Provider>(GLIMPSE_WEATHER_OBJECT_PATH)
            .await
        {
            Ok(interface) => interface,
            Err(error) => {
                tracing::error!(%error, "weather provider object disappeared");
                return;
            }
        };
        if let Err(error) = interface
            .get()
            .await
            .snapshot_changed(interface.signal_emitter())
            .await
        {
            tracing::warn!(%error, "weather snapshot change signal failed");
        }
    }
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
mod tests {
    use std::io::BufRead as _;
    use std::process::{Child, Command, Stdio};

    use super::*;
    use glimpse_dbus::{Buses, weather::Weather1Proxy};
    use glimpse_services::{Geolocation, Service, ServiceRuntime, Weather, WeatherDependencies};
    use tokio_util::sync::CancellationToken;

    #[test]
    fn watch_place_kinds_preserve_here_coordinates_and_named_locations() {
        assert_eq!(
            watched_place(0, 12.0, 34.0, "ignored").unwrap(),
            WatchedPlace::Here
        );
        assert_eq!(
            watched_place(1, 54.7, 25.3, "ignored").unwrap(),
            WatchedPlace::Coordinates {
                latitude: 54.7,
                longitude: 25.3,
            }
        );
        assert_eq!(
            watched_place(2, 0.0, 0.0, "Warsaw, PL").unwrap(),
            WatchedPlace::Location {
                name: "Warsaw, PL".to_owned(),
            }
        );
    }

    #[test]
    fn a_snapshot_prefers_the_health_reason_over_the_empty_reading_one() {
        let (_runtime, weather) = ServiceRuntime::<Weather>::new(
            <Weather as Service>::Config::from(&glimpse_config::Config::default()),
            Buses::unavailable("no bus in tests"),
            CancellationToken::new(),
        );

        let (available, stale, reason, ..) = snapshot(&weather);

        assert!(!available);
        assert!(
            !stale,
            "nothing has been read, so there is nothing to go stale"
        );
        assert_eq!(
            reason, "starting",
            "a service still starting says so; the empty-reading message is for one that has started"
        );
    }

    struct PrivateBus {
        child: Child,
        address: String,
    }

    impl PrivateBus {
        fn start() -> Self {
            let mut child = Command::new("dbus-daemon")
                .args([
                    "--session",
                    "--nofork",
                    "--print-address=1",
                    "--print-pid=1",
                ])
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let stdout = child.stdout.as_mut().unwrap();
            let mut lines = std::io::BufReader::new(stdout).lines();
            let address = lines.next().unwrap().unwrap();
            let _pid = lines.next().unwrap().unwrap();
            Self { child, address }
        }

        async fn connection(&self) -> Connection {
            zbus::connection::Builder::address(self.address.as_str())
                .unwrap()
                .build()
                .await
                .unwrap()
        }
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn introspection_and_typed_methods_match_the_frozen_interface() {
        let bus = PrivateBus::start();
        let cancel = CancellationToken::new();
        let buses = Buses::unavailable("no backend bus in test");
        let (mut location_runtime, location) = ServiceRuntime::<Geolocation>::new(
            <Geolocation as Service>::Config::from(&glimpse_config::Config::default()),
            buses.clone(),
            cancel.child_token(),
        );
        let location_task = tokio::spawn(async move { location_runtime.run(()).await });
        let (mut weather_runtime, weather) = ServiceRuntime::<Weather>::new(
            <Weather as Service>::Config::from(&glimpse_config::Config::default()),
            buses,
            cancel.child_token(),
        );
        let weather_task = tokio::spawn(async move {
            weather_runtime
                .run(WeatherDependencies {
                    geolocation: location,
                })
                .await
        });
        let provider = start(bus.connection().await, weather.clone())
            .await
            .unwrap();
        let client = bus.connection().await;
        let proxy = Weather1Proxy::new(&client).await.unwrap();

        proxy.watch_place(1, 54.7, 25.3, "").await.unwrap();
        proxy.refresh().await.unwrap();
        let invalid = proxy.watch_place(3, 0.0, 0.0, "").await.unwrap_err();
        assert!(matches!(
            invalid,
            zbus::Error::MethodError(name, _, _)
                if name.as_str() == "me.aresa.Glimpse.Weather1.Error.InvalidPlace"
        ));
        let snapshot = proxy.snapshot().await.unwrap();
        assert!(!snapshot.0);
        assert_eq!(snapshot.4, 0);

        let reply = client
            .call_method(
                Some(GLIMPSE_WEATHER_BUS_NAME),
                GLIMPSE_WEATHER_OBJECT_PATH,
                Some("org.freedesktop.DBus.Introspectable"),
                "Introspect",
                &(),
            )
            .await
            .unwrap();
        let xml: String = reply.body().deserialize().unwrap();
        let start = xml
            .find("<interface name=\"me.aresa.Glimpse.Weather1\">")
            .unwrap();
        let end = start + xml[start..].find("</interface>").unwrap() + "</interface>".len();
        assert_eq!(
            &xml[start..end],
            r#"<interface name="me.aresa.Glimpse.Weather1">
    <method name="WatchPlace">
      <arg name="kind" type="y" direction="in"/>
      <arg name="latitude" type="d" direction="in"/>
      <arg name="longitude" type="d" direction="in"/>
      <arg name="location" type="s" direction="in"/>
    </method>
    <method name="Refresh">
    </method>
    <property name="Snapshot" type="(bbsxya(yddsddssi(b(xybd(bd)(by)(bd)(bq)(bd)))a(xybd)a(xydd(by)(bx)(bx))a(ys(bs)(bs)(bx)(bx))))" access="read"/>
  </interface>"#
        );

        provider.shutdown().await;
        cancel.cancel();
        weather_task.await.unwrap().unwrap();
        location_task.await.unwrap().unwrap();
    }
}
