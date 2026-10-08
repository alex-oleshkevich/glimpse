use glimpse_dbus::testing::PrivateBus;

use super::*;
use glimpse_dbus::{Buses, weather::Weather1Proxy};
use glimpse_services::{Service, ServiceRuntime, Weather, WeatherDependencies};
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn introspection_and_typed_methods_match_the_frozen_interface() {
    let bus = PrivateBus::start();
    let cancel = CancellationToken::new();
    let buses = Buses::unavailable("no backend bus in test");
    let (_, location) = tokio::sync::watch::channel(None);
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
}
