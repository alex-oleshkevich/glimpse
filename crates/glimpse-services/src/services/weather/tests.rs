use chrono::{NaiveDate, TimeZone as _};

use glimpse_dbus::Buses;

use glimpse_dbus::weather::{AlertSeverity, Condition};

use tokio::sync::{mpsc, oneshot};

use tokio_util::sync::CancellationToken;

use super::*;

use crate::{ServiceState, service::ServiceRuntime, services::geolocation::Geolocation};

enum Asked {
    Watch(WatchedPlace),
    Refresh,
}

struct Harness {
    service: Weather,
    ctx: Ctx<Weather>,
    health: tokio::sync::watch::Receiver<ServiceState>,
    _inbox: mpsc::Receiver<Input<Weather>>,
    _cancel: CancellationToken,
}

async fn harness() -> Harness {
    harness_with(Config {
        provider: Provider::OpenMeteo,
        units: UnitSystem::Metric,
        poll_interval: 900,
        forecast_days: 7,
    })
    .await
}

async fn harness_with(config: Config) -> Harness {
    let (events, inbox) = mpsc::channel(32);
    let cancel = CancellationToken::new();
    let (location_runtime, location) = ServiceRuntime::<Geolocation>::new(
        <Geolocation as Service>::Config::from(&glimpse_config::Config::default()),
        Buses::unavailable("no bus in tests"),
        cancel.clone(),
    );
    drop(location_runtime);
    let (health, health_rx) = tokio::sync::watch::channel(ServiceState::Starting);
    let (state, _state_rx) = tokio::sync::watch::channel(initial_state(&config));
    let ctx = Ctx::<Weather>::new(
        events,
        &cancel,
        state,
        health,
        Buses::unavailable("no bus in tests"),
    );
    let service = Weather::start(
        &ctx,
        config,
        WeatherDependencies {
            geolocation: location,
        },
    )
    .await
    .expect("the service starts");

    Harness {
        service,
        ctx,
        health: health_rx,
        _inbox: inbox,
        _cancel: cancel,
    }
}

async fn call(harness: &mut Harness, asked: Asked) -> Result<(), CommandError> {
    let (reply, answer) = oneshot::channel();
    let command = match asked {
        Asked::Watch(place) => Command::Watch { place, reply },
        Asked::Refresh => Command::Refresh { reply },
    };
    harness
        .service
        .handle(&harness.ctx, Input::Command(command))
        .await;
    answer.await.expect("the command answers")
}

fn at(latitude: f64, longitude: f64) -> WatchedPlace {
    WatchedPlace::Coordinates {
        latitude,
        longitude,
    }
}

fn fix(latitude: f64, longitude: f64) -> GeoCoordinates {
    GeoCoordinates {
        latitude,
        longitude,
    }
}

fn reason(health: &tokio::sync::watch::Receiver<ServiceState>) -> Option<String> {
    match health.borrow().clone() {
        ServiceState::Degraded { reason } => Some(reason),
        _ => None,
    }
}

fn document(weather: glimpse_config::WeatherConfig) -> glimpse_config::Config {
    glimpse_config::Config {
        weather,
        ..Default::default()
    }
}

fn a_reading() -> Reading {
    Reading {
        utc_offset_seconds: 7200,
        current: None,
        hours: Vec::new(),
        days: Vec::new(),
        alerts: Vec::new(),
    }
}

fn fetched(service: &Weather, readings: Vec<Reading>) -> Fetch {
    let targets = service
        .targets()
        .into_iter()
        .zip(&readings)
        .enumerate()
        .map(|(index, (target, _))| ResolvedTarget {
            place: target.place,
            resolved: ResolvedPlace {
                coordinates: fix(54.6872 + index as f64, 25.2797),
                city: format!("City {index}"),
                country_code: "LT".to_owned(),
            },
        })
        .collect();
    Fetch {
        targets,
        readings,
        failure: None,
    }
}

fn an_alert(headline: &str) -> WeatherAlert {
    WeatherAlert {
        severity: AlertSeverity::Severe,
        headline: headline.to_owned(),
        description: None,
        source: None,
        starts_at: None,
        expires_at: None,
    }
}

mod alerts;
mod drift;
mod expiry;
mod failures;
mod places;
mod readings;
mod watching;
