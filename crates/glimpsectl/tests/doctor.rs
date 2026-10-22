use std::path::PathBuf;

use glimpsectl::commands::doctor::{Configuration, Provider};

#[test]
fn a_provider_that_answers_but_is_not_serving_is_not_reported_healthy() {
    let degraded = Provider::answered(
        "x",
        false,
        "another gamma client holds the outputs".to_owned(),
    );
    assert!(degraded.running, "it answered, so it is running");
    let [_, state, detail] = degraded.row();
    assert!(state.contains("degraded"));
    assert!(detail.contains("another gamma client"));
}

#[test]
fn a_provider_that_owns_its_name_but_never_answers_is_degraded_not_missing() {
    let hung = Provider::answered(
        "me.aresa.Glimpse.NightLight",
        false,
        "did not answer within 10s".to_owned(),
    );
    let missing = Provider::absent(
        "me.aresa.Glimpse.Weather",
        "nobody owns the name".to_owned(),
    );

    assert!(hung.running, "it owns the name, it just will not talk");
    let [_, hung_state, detail] = hung.row();
    let [_, missing_state, _] = missing.row();

    assert!(hung_state.contains("degraded"), "{hung_state}");
    assert!(missing_state.contains("not running"), "{missing_state}");
    assert_ne!(hung_state, missing_state);
    assert!(detail.contains("did not answer"));
}

#[test]
fn a_serving_provider_has_no_detail_to_add() {
    let [_, state, detail] = Provider::answered("x", true, String::new()).row();
    assert!(state.contains("serving"));
    assert!(detail.is_empty());
}

#[test]
fn an_absent_provider_is_not_running_and_says_why() {
    let absent = Provider::absent("x", "nobody owns the name".to_owned());
    assert!(!absent.running);
    let [_, state, detail] = absent.row();
    assert!(state.contains("not running"));
    assert!(detail.contains("nobody owns the name"));
}

#[test]
fn a_configuration_that_does_not_load_says_binaries_will_not_start() {
    let broken = Configuration {
        files: vec![PathBuf::from("/etc/glimpse/config.toml")],
        loads: false,
        panels_only: false,
        warnings: Vec::new(),
        error: Some("expected a table".to_owned()),
    };
    let rendered = broken.section().render();
    assert!(rendered.contains("expected a table"));
    assert!(rendered.contains("start on the default configuration"));
    let panels = Configuration {
        panels_only: true,
        ..broken
    };
    let rendered = panels.section().render();
    assert!(rendered.contains("the other binaries are unaffected"));
    assert!(!rendered.contains("start on the default configuration"));
    assert!(!rendered.contains("exit on it"));
}

use glimpse_dbus::{
    idle::*, night_light::*, notifications::*, settings::*, testing::PrivateBus, weather::*,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn unique_destination(header: Option<zbus::message::Header<'_>>) {
    assert!(
        header
            .unwrap()
            .destination()
            .unwrap()
            .as_str()
            .starts_with(':')
    );
}

struct NightLight(Arc<AtomicBool>);

#[zbus::interface(name = "me.aresa.Glimpse.NightLight1")]
impl NightLight {
    #[zbus(property)]
    fn snapshot(
        &self,
        #[zbus(header)] header: Option<zbus::message::Header<'_>>,
    ) -> NightLightSnapshot {
        unique_destination(header);
        let serving = self.0.load(Ordering::SeqCst);
        NightLightSnapshot {
            schedule: "automatic".into(),
            overridden: false,
            temperature: 6500,
            target: 4200,
            active: false,
            serving,
            reason: if serving {
                String::new()
            } else {
                "gamma unavailable".into()
            },
            configured: "automatic".into(),
            manual: false,
            paused_until: 0,
            pausable: false,
        }
    }
}

struct Weather;

#[zbus::interface(name = "me.aresa.Glimpse.Weather1")]
impl Weather {
    #[zbus(property)]
    fn snapshot(
        &self,
        #[zbus(header)] header: Option<zbus::message::Header<'_>>,
    ) -> WeatherSnapshot {
        unique_destination(header);
        (true, false, String::new(), 0, 0, Vec::new())
    }
}

struct Notifications;

#[zbus::interface(name = "me.aresa.Glimpse.Notifications1")]
impl Notifications {
    #[zbus(property)]
    fn snapshot(
        &self,
        #[zbus(header)] header: Option<zbus::message::Header<'_>>,
    ) -> NotificationsSnapshot {
        unique_destination(header);
        (Vec::new(), (false, 0), true, String::new())
    }
}

struct Settings;

#[zbus::interface(name = "me.aresa.Glimpse.Settings1")]
impl Settings {
    #[zbus(property)]
    fn mode(&self, #[zbus(header)] header: Option<zbus::message::Header<'_>>) -> ModeSnapshot {
        unique_destination(header);
        (true, "external".into(), 0, 1, 0, String::new())
    }
    #[zbus(property)]
    fn geolocation(&self) -> GeolocationSnapshot {
        (false, 0.0, 0.0, "no fix".into())
    }
    #[zbus(property)]
    fn solar(&self) -> SolarSnapshot {
        (false, 0, 0, "no fix".into())
    }
}

struct Idle;

#[zbus::interface(name = "me.aresa.Glimpse.Idle1")]
impl Idle {
    #[zbus(property)]
    fn inhibitors(&self) -> Vec<IdleInhibitorRecord> {
        Vec::new()
    }
    #[zbus(property)]
    fn health(
        &self,
        #[zbus(header)] header: Option<zbus::message::Header<'_>>,
    ) -> InhibitorsHealth {
        unique_destination(header);
        let mut health = InhibitorsHealth::default();
        health.screen_saver.kind = HealthKind::Ready;
        health
    }
}

async fn diagnose(bus: &PrivateBus, config: &std::path::Path, json: bool) -> std::process::Output {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_glimpsectl"));
    command
        .arg("--config-dir")
        .arg(config.parent().unwrap())
        .arg("doctor")
        .env("XDG_CONFIG_HOME", config.parent().unwrap().join("home"))
        .env("DBUS_SESSION_BUS_ADDRESS", bus.address());
    if json {
        command.arg("--json");
    }
    tokio::time::timeout(std::time::Duration::from_secs(8), command.output())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn doctor_reports_failures_in_text_and_json_and_optional_ui_absence_is_informational() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    let serving = Arc::new(AtomicBool::new(true));
    server
        .object_server()
        .at(GLIMPSE_NIGHT_LIGHT_OBJECT_PATH, NightLight(serving.clone()))
        .await
        .unwrap();
    server
        .object_server()
        .at(GLIMPSE_WEATHER_OBJECT_PATH, Weather)
        .await
        .unwrap();
    server
        .object_server()
        .at(GLIMPSE_NOTIFICATIONS_OBJECT_PATH, Notifications)
        .await
        .unwrap();
    server
        .object_server()
        .at(GLIMPSE_SETTINGS_OBJECT_PATH, Settings)
        .await
        .unwrap();
    server
        .object_server()
        .at(GLIMPSE_IDLE_OBJECT_PATH, Idle)
        .await
        .unwrap();
    for name in [
        GLIMPSE_NIGHT_LIGHT_BUS_NAME,
        GLIMPSE_WEATHER_BUS_NAME,
        GLIMPSE_NOTIFICATIONS_BUS_NAME,
        GLIMPSE_SETTINGS_BUS_NAME,
        GLIMPSE_IDLE_BUS_NAME,
    ] {
        glimpse_dbus::own_name(&server, name).await.unwrap();
    }
    let scratch = tempfile::tempdir().unwrap();
    let config = scratch.path().join("config.toml");
    std::fs::write(&config, "").unwrap();
    let healthy = diagnose(&bus, &config, true).await;
    assert!(healthy.status.success(), "{healthy:?}");
    let report: serde_json::Value = serde_json::from_slice(&healthy.stdout).unwrap();
    assert!(report["configuration"]["loads"].as_bool().unwrap());
    assert_eq!(report["providers"].as_array().unwrap().len(), 5);
    for name in [
        "me.aresa.GlimpseLock",
        "me.aresa.GlimpseWallpaper",
        "me.aresa.Glimpse.Shot",
    ] {
        let optional = report["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == name)
            .unwrap();
        assert_eq!(optional["running"], false);
        assert!(optional["version"].is_null());
    }
    serving.store(false, Ordering::SeqCst);
    for json in [false, true] {
        let output = diagnose(&bus, &config, json).await;
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("gamma unavailable"), "{text}");
        if json {
            assert!(
                serde_json::from_str::<serde_json::Value>(&text).unwrap()["providers"].is_array()
            );
        } else {
            assert!(text.contains("degraded"));
        }
    }
    serving.store(true, Ordering::SeqCst);
    server.release_name(GLIMPSE_WEATHER_BUS_NAME).await.unwrap();
    let missing = diagnose(&bus, &config, true).await;
    assert_eq!(missing.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&missing.stdout).unwrap();
    let weather = report["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == GLIMPSE_WEATHER_BUS_NAME)
        .unwrap();
    assert_eq!(weather["running"], false);
    glimpse_dbus::own_name(&server, GLIMPSE_WEATHER_BUS_NAME)
        .await
        .unwrap();
    std::fs::write(&config, "this is not toml").unwrap();
    for json in [false, true] {
        let invalid = diagnose(&bus, &config, json).await;
        assert_eq!(invalid.status.code(), Some(1));
        if json {
            let report: serde_json::Value = serde_json::from_slice(&invalid.stdout).unwrap();
            assert_eq!(report["configuration"]["loads"], false);
            assert!(
                report["providers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|row| row["serving"] == true)
            );
        } else {
            assert!(
                String::from_utf8(invalid.stdout)
                    .unwrap()
                    .contains("start on the default configuration")
            );
        }
    }
}

async fn restore_lock(value: bool) -> glimpsectl::commands::doctor::SessionLock {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let dir = tempfile::tempdir().unwrap();
    let listener = tokio::net::UnixListener::bind(dir.path().join(".socket.sock")).unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut command = vec![0_u8; 256];
            let _ = stream.read(&mut command).await;
            let reply = format!(r#"{{"option":"misc:allow_session_lock_restore","bool":{value}}}"#);
            let _ = stream.write_all(reply.as_bytes()).await;
        }
    });
    glimpsectl::commands::doctor::session_lock(&glimpse_compositors::Compositor::Hyprland(
        glimpse_compositors::Hyprland::at(dir.path()),
    ))
    .await
    .unwrap()
}

#[tokio::test]
async fn a_disabled_lock_restore_is_a_failure_with_the_fix_and_an_enabled_one_is_not() {
    let off = restore_lock(false).await;
    assert!(!off.enabled);
    let text = off.section().render();
    assert!(text.contains("misc:allow_session_lock_restore"));
    assert!(text.contains("misc { allow_session_lock_restore = true }"));
    assert!(restore_lock(true).await.enabled);
}
