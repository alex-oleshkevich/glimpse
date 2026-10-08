use std::time::Duration;

use zbus::zvariant::Type as _;

use super::*;
use crate::testing::PrivateBus;

#[test]
fn mode_wire_validates_codes_and_health() {
    assert_eq!(ModeSnapshot::SIGNATURE, "(bbuuus)");
    assert_eq!(
        decode_mode((true, false, 0, 0, 0, String::new())).unwrap(),
        ModeStatus {
            automatic: false,
            requested: None,
            observed: None,
            temporary: None,
        }
    );
    assert_eq!(
        decode_mode((false, true, 1, 2, 1, "write failed".into()))
            .unwrap()
            .temporary,
        Some(Mode::Light)
    );
    for codes in [(3, 0, 0), (0, 3, 0), (0, 0, 3)] {
        assert!(decode_mode((true, false, codes.0, codes.1, codes.2, String::new())).is_err());
    }
    assert!(decode_mode((true, false, 0, 0, 0, "failed".into())).is_err());
}

#[test]
fn solar_wire_decodes_known_unknown_and_polar_states() {
    assert_eq!(SolarSnapshot::SIGNATURE, "(buxs)");
    assert_eq!(decode_solar((false, 0, 0, "no fix".into())), Ok(None));
    for (code, phase) in [(1, SolarPhase::Day), (2, SolarPhase::Night)] {
        let decoded = decode_solar((true, code, 0, String::new()))
            .unwrap()
            .unwrap();
        assert_eq!(decoded.phase, phase);
        assert_eq!(decoded.next_change, None);
        let timestamp = 1_800_000_000_123_456;
        assert_eq!(
            decode_solar((true, code, timestamp, String::new()))
                .unwrap()
                .unwrap()
                .next_change
                .unwrap()
                .timestamp_micros(),
            timestamp,
        );
    }
    assert!(decode_solar((true, 0, 0, String::new())).is_err());
    assert!(decode_solar((true, 3, 0, String::new())).is_err());
    assert!(decode_solar((true, 1, i64::MAX, String::new())).is_err());
    assert!(decode_solar((true, 1, 0, "failed".into())).is_err());
}

struct FakeSettings(watch::Receiver<SolarSnapshot>, GeolocationSnapshot);

#[zbus::interface(name = "me.aresa.Glimpse.Settings1")]
impl FakeSettings {
    #[zbus(property)]
    fn solar(&self) -> SolarSnapshot {
        self.0.borrow().clone()
    }

    #[zbus(property)]
    fn geolocation(&self) -> GeolocationSnapshot {
        self.1.clone()
    }
}

async fn serve(
    bus: &PrivateBus,
    initial: SolarSnapshot,
) -> (zbus::Connection, watch::Sender<SolarSnapshot>) {
    let connection = bus.connection().await;
    let (updates, state) = watch::channel(initial);
    connection
        .object_server()
        .at(
            GLIMPSE_SETTINGS_OBJECT_PATH,
            FakeSettings(state, (false, 0.0, 0.0, String::new())),
        )
        .await
        .unwrap();
    crate::own_name(&connection, GLIMPSE_SETTINGS_BUS_NAME)
        .await
        .unwrap();
    (connection, updates)
}

#[test]
fn location_wire_preserves_cached_fixes_and_rejects_invalid_coordinates() {
    assert_eq!(GeolocationSnapshot::SIGNATURE, "(bdds)");
    assert_eq!(
        decode_geolocation((false, 0.0, 0.0, "no fix".into())),
        Ok(None)
    );
    for reason in ["", "location backend unavailable"] {
        assert_eq!(
            decode_geolocation((true, 0.0, 0.0, reason.into())),
            Ok(Some(GeoCoordinates {
                latitude: 0.0,
                longitude: 0.0
            }))
        );
    }
    for (latitude, longitude) in [
        (91.0, 0.0),
        (0.0, -181.0),
        (f64::NAN, 0.0),
        (0.0, f64::INFINITY),
    ] {
        assert!(decode_geolocation((true, latitude, longitude, String::new())).is_err());
    }
}

#[tokio::test]
async fn location_follower_observes_changes_and_discards_a_lost_owner() {
    let bus = PrivateBus::start();
    let (server, _) = serve(&bus, (false, 0, 0, String::new())).await;
    let interface = server
        .object_server()
        .interface::<_, FakeSettings>(GLIMPSE_SETTINGS_OBJECT_PATH)
        .await
        .unwrap();
    interface.get_mut().await.1 = (true, 51.5, -0.1, String::new());
    let provider = SettingsProvider::start(bus.connection().await);
    let mut location = provider.subscribe_location();
    tokio::time::timeout(
        Duration::from_secs(3),
        location.wait_for(|fix| fix.is_some()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(location.borrow().as_ref().unwrap().latitude, 51.5);
    for (snapshot, expected) in [
        (
            (true, 52.0, 21.0, "cached fix".into()),
            Some(GeoCoordinates {
                latitude: 52.0,
                longitude: 21.0,
            }),
        ),
        ((true, 99.0, 0.0, String::new()), None),
        (
            (true, 0.0, 0.0, String::new()),
            Some(GeoCoordinates {
                latitude: 0.0,
                longitude: 0.0,
            }),
        ),
    ] {
        interface.get_mut().await.1 = snapshot;
        interface
            .get()
            .await
            .geolocation_changed(interface.signal_emitter())
            .await
            .unwrap();
        tokio::time::timeout(
            Duration::from_secs(3),
            location.wait_for(|fix| fix == &expected),
        )
        .await
        .unwrap()
        .unwrap();
    }
    server
        .release_name(GLIMPSE_SETTINGS_BUS_NAME)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), location.wait_for(Option::is_none))
        .await
        .unwrap()
        .unwrap();
    let (replacement, _) = serve(&bus, (false, 0, 0, String::new())).await;
    let interface = replacement
        .object_server()
        .interface::<_, FakeSettings>(GLIMPSE_SETTINGS_OBJECT_PATH)
        .await
        .unwrap();
    interface.get_mut().await.1 = (true, -33.0, 151.0, String::new());
    interface
        .get()
        .await
        .geolocation_changed(interface.signal_emitter())
        .await
        .unwrap();
    tokio::time::timeout(
        Duration::from_secs(3),
        location.wait_for(|fix| fix.as_ref().is_some_and(|fix| fix.latitude == -33.0)),
    )
    .await
    .unwrap()
    .unwrap();
    provider.shutdown().await;
}

async fn wait(state: &mut watch::Receiver<Option<SolarStatus>>, phase: Option<SolarPhase>) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if state.borrow().as_ref().map(|status| status.phase.clone()) == phase {
                return;
            }
            state.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

async fn change(
    connection: &zbus::Connection,
    updates: &watch::Sender<SolarSnapshot>,
    snapshot: SolarSnapshot,
) {
    updates.send_replace(snapshot);
    let interface = connection
        .object_server()
        .interface::<_, FakeSettings>(GLIMPSE_SETTINGS_OBJECT_PATH)
        .await
        .unwrap();
    interface
        .get()
        .await
        .solar_changed(interface.signal_emitter())
        .await
        .unwrap();
}

#[tokio::test]
async fn follower_reads_initial_changes_unknowns_and_a_replacement_owner() {
    let bus = PrivateBus::start();
    let provider = SettingsProvider::start(bus.connection().await);
    let mut state = provider.subscribe();
    assert!(state.borrow().is_none());
    let (first, updates) = serve(&bus, (true, 1, 0, String::new())).await;
    wait(&mut state, Some(SolarPhase::Day)).await;
    let late = SettingsProvider::start(bus.connection().await);
    wait(&mut late.subscribe(), Some(SolarPhase::Day)).await;
    change(&first, &updates, (true, 2, 0, String::new())).await;
    wait(&mut state, Some(SolarPhase::Night)).await;
    change(&first, &updates, (false, 0, 0, "no fix".into())).await;
    wait(&mut state, None).await;
    change(&first, &updates, (true, 1, 0, String::new())).await;
    wait(&mut state, Some(SolarPhase::Day)).await;
    change(&first, &updates, (true, 3, 0, String::new())).await;
    wait(&mut state, None).await;
    change(&first, &updates, (true, 1, 0, String::new())).await;
    wait(&mut state, Some(SolarPhase::Day)).await;
    first.release_name(GLIMPSE_SETTINGS_BUS_NAME).await.unwrap();
    wait(&mut state, None).await;
    let (second, _) = serve(&bus, (true, 2, 0, String::new())).await;
    wait(&mut state, Some(SolarPhase::Night)).await;
    wait(&mut late.subscribe(), Some(SolarPhase::Night)).await;
    late.shutdown().await;
    provider.shutdown().await;
    drop(second);
}

#[tokio::test]
async fn losing_the_bus_discards_solar_and_location() {
    let mut bus = PrivateBus::start();
    let (server, _) = serve(&bus, (true, 1, 0, String::new())).await;
    server
        .object_server()
        .interface::<_, FakeSettings>(GLIMPSE_SETTINGS_OBJECT_PATH)
        .await
        .unwrap()
        .get_mut()
        .await
        .1 = (true, 52.0, 21.0, String::new());
    let provider = SettingsProvider::start(bus.connection().await);
    let mut state = provider.subscribe();
    let mut location = provider.subscribe_location();
    wait(&mut state, Some(SolarPhase::Day)).await;
    tokio::time::timeout(Duration::from_secs(3), location.wait_for(Option::is_some))
        .await
        .unwrap()
        .unwrap();
    bus.kill();
    wait(&mut state, None).await;
    tokio::time::timeout(Duration::from_secs(3), location.wait_for(Option::is_none))
        .await
        .unwrap()
        .unwrap();
    provider.shutdown().await;
}
