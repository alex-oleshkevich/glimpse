use std::time::Duration;

use glimpse_dbus::settings::{Settings1Proxy, SettingsProvider, SolarPhase};
use glimpse_dbus::testing::PrivateBus;
use glimpse_services::{FakeGamma, NightLight, NightLightDependencies, ServiceState};

use super::*;

async fn phase(
    state: &mut tokio::sync::watch::Receiver<Option<glimpse_dbus::settings::SolarStatus>>,
) -> SolarPhase {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(current) = state.borrow().as_ref() {
                return current.phase.clone();
            }
            state.changed().await.unwrap();
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn configured_coordinates_publish_solar_and_reconfigure_without_a_system_bus() {
    let bus = PrivateBus::start();
    let mut config = Config {
        geolocation: glimpse_config::Geolocation::Manual {
            latitude: 90.0,
            longitude: 0.0,
        },
        ..Config::default()
    };
    let services =
        SettingsServices::start_with_buses(&config, Buses::for_session(bus.connection().await))
            .await
            .unwrap();
    let client = SettingsProvider::start(bus.connection().await);
    let mut state = client.subscribe();
    let mut location = client.subscribe_location();
    tokio::time::timeout(Duration::from_secs(3), location.wait_for(Option::is_some))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(location.borrow().as_ref().unwrap().latitude, 90.0);
    let first = phase(&mut state).await;
    assert!(state.borrow().as_ref().unwrap().next_change.is_none());
    let duplicate =
        SettingsServices::start_with_buses(&config, Buses::for_session(bus.connection().await))
            .await;
    assert!(duplicate.is_err());
    config.geolocation = glimpse_config::Geolocation::Manual {
        latitude: -90.0,
        longitude: 0.0,
    };
    services.reconfigure(&config);
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if phase(&mut state).await != first {
                break;
            }
            state.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(
        Duration::from_secs(3),
        location.wait_for(|fix| fix.as_ref().is_some_and(|fix| fix.latitude == -90.0)),
    )
    .await
    .unwrap()
    .unwrap();
    services.shutdown().await;
    tokio::time::timeout(Duration::from_secs(3), async {
        while state.borrow().is_some() {
            state.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(3), location.wait_for(Option::is_none))
        .await
        .unwrap()
        .unwrap();
    client.shutdown().await;
}

#[tokio::test]
async fn no_location_exposes_unknown_instead_of_day_or_night() {
    let bus = PrivateBus::start();
    let services = SettingsServices::start_with_buses(
        &Config::default(),
        Buses::for_session(bus.connection().await),
    )
    .await
    .unwrap();
    let connection = bus.connection().await;
    let proxy = Settings1Proxy::new(&connection).await.unwrap();
    let (available, phase, next_change, reason) = proxy.solar().await.unwrap();
    assert!(!available);
    assert_eq!(phase, 0);
    assert_eq!(next_change, 0);
    assert!(!reason.is_empty());
    let (has_fix, latitude, longitude, reason) = proxy.geolocation().await.unwrap();
    assert!(!has_fix);
    assert_eq!((latitude, longitude), (0.0, 0.0));
    assert!(!reason.is_empty());
    services.shutdown().await;
}

#[tokio::test]
async fn night_light_follows_the_daemon_and_recovers_after_a_restart() {
    let bus = PrivateBus::start();
    let mut config = Config {
        geolocation: glimpse_config::Geolocation::Manual {
            latitude: 90.0,
            longitude: 0.0,
        },
        ..Config::default()
    };
    config.night_light.schedule = glimpse_config::Schedule::Automatic;
    config.night_light.temperature = 4200;
    let services =
        SettingsServices::start_with_buses(&config, Buses::for_session(bus.connection().await))
            .await
            .unwrap();
    let client = SettingsProvider::start(bus.connection().await);
    let mut solar = client.subscribe();
    if phase(&mut solar).await == SolarPhase::Day {
        config.geolocation = glimpse_config::Geolocation::Manual {
            latitude: -90.0,
            longitude: 0.0,
        };
        services.reconfigure(&config);
    }
    tokio::time::timeout(
        Duration::from_secs(3),
        solar.wait_for(|state| state.as_ref().is_some_and(|s| s.phase == SolarPhase::Night)),
    )
    .await
    .unwrap()
    .unwrap();
    let gamma = FakeGamma::default();
    let (mut night_light, handle) = Running::<NightLight>::spawn(
        &config,
        Buses::for_session(bus.connection().await),
        NightLightDependencies {
            solar: client.subscribe(),
            gamma: Box::new(gamma.clone()),
        },
    );
    let mut state = handle.subscribe();
    tokio::time::timeout(
        Duration::from_secs(3),
        state.wait_for(|state| state.temperature == 4200),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(gamma.applied().last(), Some(&4200));

    services.shutdown().await;
    let mut health = handle.health();
    tokio::time::timeout(
        Duration::from_secs(3),
        health.wait_for(|health| matches!(health, ServiceState::Degraded { .. })),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(solar.borrow().is_none());
    assert_eq!(gamma.resets(), 0);

    let glimpse_config::Geolocation::Manual {
        latitude,
        longitude,
    } = config.geolocation
    else {
        unreachable!();
    };
    config.geolocation = glimpse_config::Geolocation::Manual {
        latitude: -latitude,
        longitude,
    };
    let replacement =
        SettingsServices::start_with_buses(&config, Buses::for_session(bus.connection().await))
            .await
            .unwrap();
    tokio::time::timeout(
        Duration::from_secs(3),
        health.wait_for(|health| matches!(health, ServiceState::Running)),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(handle.snapshot().temperature, 6500);
    assert_eq!(gamma.resets(), 1);

    night_light.stop().await;
    client.shutdown().await;
    replacement.shutdown().await;
}
