use super::*;
use glimpse_dbus::Buses;
use glimpse_dbus::geoclue::GeoClueClientProxy;
use glimpse_dbus::testing::PrivateBus;
use glimpse_dbus::testing::geoclue::FakeGeoClue;

#[tokio::test]
async fn the_location_source_reports_unavailable_without_a_system_bus() {
    let cancel = tokio_util::sync::CancellationToken::new();
    let (events, _inbox) = tokio::sync::mpsc::channel::<crate::service::Input<Privacy>>(8);
    let (state, _state_rx) =
        tokio::sync::watch::channel(super::super::super::super::PrivacyState::default());
    let (health, _health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
    let ctx = Ctx::<Privacy>::new(
        events,
        &cancel,
        state,
        health,
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );

    let mut stream = location(ctx).await;

    let event = futures_util::FutureExt::now_or_never(stream.next())
        .expect("a stream::once with no real await point resolves synchronously")
        .expect("one item");
    assert!(
        matches!(event, Event::Unavailable(Resource::Location, _)),
        "with no system bus the source must say it cannot see, never silently report nothing \
             in use"
    );
}

async fn next_in_use(stream: &mut Events) -> bool {
    let event = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("a location event within the bound")
        .expect("a location stream that stays open");
    match event {
        Event::Location(in_use) => in_use,
        _ => panic!("the location source must only report location events"),
    }
}

async fn geoclue_client(bus: &glimpse_dbus::testing::PrivateBus) -> GeoClueClientProxy<'static> {
    let connection = bus.connection().await;
    let manager = GeoClueManagerProxy::new(&connection)
        .await
        .expect("a manager proxy");
    let path = manager.create_client().await.expect("a client");
    GeoClueClientProxy::builder(&connection)
        .path(path)
        .expect("a valid client path")
        .build()
        .await
        .expect("a client proxy")
}

#[tokio::test]
async fn the_location_source_follows_geoclue_through_absence_appearance_and_restart() {
    let bus = PrivateBus::start();
    let cancel = tokio_util::sync::CancellationToken::new();
    let (events, _inbox) = tokio::sync::mpsc::channel::<crate::service::Input<Privacy>>(8);
    let (state, _state_rx) =
        tokio::sync::watch::channel(super::super::super::super::PrivacyState::default());
    let (health, _health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
    let ctx = Ctx::<Privacy>::new(
        events,
        &cancel,
        state,
        health,
        Buses::for_system(bus.connection().await),
    );

    let mut stream = location(ctx).await;
    assert!(
        !next_in_use(&mut stream).await,
        "GeoClue absent at panel start is not in use, and never unavailable forever"
    );

    let first_bus = bus.connection().await;
    let _first = FakeGeoClue::start(first_bus.clone())
        .await
        .expect("a first GeoClue");
    assert!(
        !next_in_use(&mut stream).await,
        "GeoClue appearing after the panel started must be followed, reading InUse=false"
    );
    let client = geoclue_client(&bus).await;
    client.start().await.expect("start a client");
    assert!(
        next_in_use(&mut stream).await,
        "InUse turning true on an appeared GeoClue must light the location indicator"
    );
    client.stop().await.expect("stop the client");
    assert!(
        !next_in_use(&mut stream).await,
        "InUse turning false must clear the location indicator"
    );
    client.start().await.expect("restart the client");
    assert!(next_in_use(&mut stream).await);

    first_bus
        .release_name(glimpse_dbus::testing::geoclue::SERVICE_NAME)
        .await
        .expect("GeoClue leaves the bus");
    assert!(
        !next_in_use(&mut stream).await,
        "GeoClue exiting must report not in use"
    );

    let _second = FakeGeoClue::start(bus.connection().await)
        .await
        .expect("a restarted GeoClue");
    assert!(
        !next_in_use(&mut stream).await,
        "a restarted GeoClue must be followed again"
    );
    let client = geoclue_client(&bus).await;
    client.start().await.expect("start a client on the restart");
    assert!(
        next_in_use(&mut stream).await,
        "InUse on the restarted GeoClue must light the location indicator"
    );
}
