use std::time::Duration;

use glimpse_config::{Config, Schedule};
use glimpse_dbus::Buses;
use glimpse_services::{
    FakeGamma, NightLight, NightLightDependencies, NightLightHandle, Running, ServiceState,
};

async fn at_night() -> (Running<NightLight>, NightLightHandle, FakeGamma, Config) {
    let now = chrono::Local::now();
    let mut document = Config::default();
    document.night_light.schedule = Schedule::Schedule;
    document.night_light.start_time = Some(
        (now - chrono::TimeDelta::hours(2))
            .format("%H:%M")
            .to_string(),
    );
    document.night_light.end_time = Some(
        (now + chrono::TimeDelta::hours(2))
            .format("%H:%M")
            .to_string(),
    );
    let night = document.night_light.temperature;

    let gamma = FakeGamma::default();
    let (_solar_tx, solar) = tokio::sync::watch::channel(None);
    let (running, night_light) = Running::<NightLight>::spawn(
        &document,
        Buses::unavailable("no backend bus in test"),
        NightLightDependencies {
            solar,
            gamma: Box::new(gamma.clone()),
        },
    );

    tokio::time::sleep(Duration::from_secs(6)).await;
    assert_eq!(gamma.applied().last(), Some(&night), "night is on screen");
    (running, night_light, gamma, document)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn switching_off_at_night_fades_and_hands_the_outputs_back() {
    let (mut running, night_light, gamma, _document) = at_night().await;

    night_light
        .set_schedule(Schedule::Off)
        .await
        .expect("off is accepted");
    tokio::time::sleep(Duration::from_secs(8)).await;

    let applied = gamma.applied();
    let resets = gamma.resets();
    running.cancel();
    running.stop().await;
    assert_eq!(
        resets,
        1,
        "switching off must glide to daylight and release; still at {:?}",
        applied.last()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reloading_off_at_night_fades_and_hands_the_outputs_back() {
    let (mut running, _night_light, gamma, mut document) = at_night().await;

    document.night_light.schedule = Schedule::Off;
    running.reconfigure(&document);
    tokio::time::sleep(Duration::from_secs(8)).await;

    let applied = gamma.applied();
    let resets = gamma.resets();
    running.cancel();
    running.stop().await;
    assert_eq!(
        resets,
        1,
        "reloading off must glide to daylight and release; still at {:?}",
        applied.last()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failing_backend_degrades_the_glide_to_off_and_recovers() {
    let (mut running, night_light, gamma, _document) = at_night().await;
    let mut health = night_light.health();

    night_light
        .set_schedule(Schedule::Off)
        .await
        .expect("off is accepted");
    gamma.fail(Some("x"));

    let degraded = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let ServiceState::Degraded { reason } = &*health.borrow() {
                break reason.clone();
            }
            if health.changed().await.is_err() {
                break String::new();
            }
        }
    })
    .await;
    gamma.fail(None);
    assert_eq!(
        degraded.expect("health reports degraded while the glide runs"),
        "x"
    );

    tokio::time::sleep(Duration::from_secs(8)).await;

    let applied = gamma.applied();
    let resets = gamma.resets();
    running.cancel();
    running.stop().await;
    assert_eq!(
        resets,
        1,
        "the glide must finish and release once the backend recovers; still at {:?}",
        applied.last()
    );
}
