use std::sync::{Arc, Mutex};

use glimpse_dbus::testing::PrivateBus;
use glimpse_dbus::{Buses, own_name};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::service::ServiceRuntime;
use crate::services::compositor::Compositor;
use crate::{CommandError, NoConfig};

#[test]
fn capability_keeps_inhibitor_blocks_visible_and_disabled() {
    let mapped = capability("inhibitor-blocked");
    assert!(mapped.visible());
    assert!(!mapped.enabled());
    assert_eq!(mapped, Capability::Blocked);
}

#[test]
fn a_polkit_challenge_stays_available() {
    assert_eq!(capability("challenge"), Capability::Available);
    assert!(capability("challenge").enabled());
}

#[test]
fn unsupported_capability_is_absent() {
    assert!(!capability("na").visible());
    assert_eq!(capability("no"), Capability::Hidden);
}

#[test]
fn inhibitors_are_limited_and_text_is_bounded() {
    let entries = (0..10)
        .map(|_| {
            (
                "sleep".to_owned(),
                "x".repeat(200),
                "reason".to_owned(),
                "block".to_owned(),
                0,
                0,
            )
        })
        .collect();
    let listed = inhibitors(entries);
    assert_eq!(listed.len(), INHIBITOR_MAX);
    assert!(listed[0].who.ends_with('…'));
}

#[test]
fn delay_inhibitors_are_kept_alongside_blocks() {
    let listed = inhibitors(vec![
        (
            "sleep".to_owned(),
            "player".to_owned(),
            "video".to_owned(),
            "delay".to_owned(),
            1000,
            1,
        ),
        (
            "shutdown".to_owned(),
            "updater".to_owned(),
            "install".to_owned(),
            "block".to_owned(),
            1000,
            2,
        ),
        (
            "idle".to_owned(),
            "other".to_owned(),
            "ignore".to_owned(),
            "none".to_owned(),
            1000,
            3,
        ),
    ]);
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].mode, "delay");
    assert!(!listed[0].blocks("sleep"));
    assert!(listed[1].blocks("shutdown"));
}

#[test]
fn block_weak_inhibitors_still_block() {
    let listed = inhibitors(vec![(
        "shutdown".to_owned(),
        "saver".to_owned(),
        "save".to_owned(),
        "block-weak".to_owned(),
        1000,
        4,
    )]);
    assert_eq!(listed.len(), 1);
    assert!(listed[0].blocks("shutdown"));
    assert_eq!(listed[0].mode, "block-weak");
}

#[test]
fn inhibitors_strip_hostile_text() {
    let listed = inhibitors(vec![(
        "sleep".to_owned(),
        "editor\u{202e}gpj.exe".to_owned(),
        "save\nwork".to_owned(),
        "block".to_owned(),
        1000,
        42,
    )]);

    assert_eq!(listed[0].who, "editor gpj.exe");
    assert_eq!(listed[0].why, "save work");
}

#[test]
fn a_window_count_does_not_clobber_logind_fields() {
    let mut state = SessionActionsState {
        user: Some("sam".into()),
        hibernate: Capability::Available,
        ..Default::default()
    };
    apply_logind(
        &mut state,
        LogindSnapshot {
            user: Some("sam".into()),
            signed_in_seconds: Some(12),
            inhibitors: Vec::new(),
            suspend: Capability::Available,
            hibernate: Capability::Hidden,
            reboot: Capability::Available,
            power_off: Capability::Available,
        },
    );
    state.windows = Some(4);
    assert_eq!(state.user.as_deref(), Some("sam"));
    assert_eq!(state.windows, Some(4));
    assert_eq!(state.hibernate, Capability::Hidden);
}

fn unit(name: &str, active_state: &str, before: &[&str]) -> BoundUnit {
    BoundUnit {
        name: name.to_owned(),
        active_state: active_state.to_owned(),
        before: before.iter().map(|target| (*target).to_owned()).collect(),
    }
}

#[test]
fn an_active_compositor_service_ordered_before_the_target_stops_it() {
    assert!(stops_graphical_session(&[unit(
        "niri.service",
        "active",
        &["graphical-session.target", "xdg-desktop-autostart.target"]
    )]));
}

#[test]
fn a_uwsm_compositor_unit_stops_the_target() {
    assert!(stops_graphical_session(&[unit(
        "wayland-wm@hyprland.desktop.service",
        "active",
        &["graphical-session.target"]
    )]));
}

#[test]
fn a_sidecar_service_alone_does_not_stop_the_target() {
    assert!(!stops_graphical_session(&[unit(
        "xwayland-satellite.service",
        "active",
        &[]
    )]));
}

#[test]
fn a_compositor_beside_a_sidecar_stops_the_target() {
    assert!(stops_graphical_session(&[
        unit("xwayland-satellite.service", "active", &[]),
        unit("niri.service", "active", &["graphical-session.target"]),
    ]));
}

#[test]
fn an_activating_compositor_does_not_stop_the_target() {
    assert!(!stops_graphical_session(&[unit(
        "niri.service",
        "activating",
        &["graphical-session.target"]
    )]));
}

#[test]
fn a_bound_target_does_not_stop_the_target() {
    assert!(!stops_graphical_session(&[unit(
        "hyprland-session.target",
        "active",
        &["graphical-session.target"]
    )]));
}

#[test]
fn nothing_bound_does_not_stop_the_target() {
    assert!(!stops_graphical_session(&[]));
}

#[test]
fn admission_errors_keep_the_agents_refusal() {
    let invalid = admission_error(method_error(
        "org.freedesktop.DBus.Error.InvalidArgs",
        "unknown action",
    ));
    assert!(matches!(
        invalid,
        CommandError::InvalidArgument(message) if message == "unknown action"
    ));
    let failed = admission_error(method_error(
        "org.freedesktop.DBus.Error.Failed",
        "lock state is unknown",
    ));
    assert!(matches!(
        failed,
        CommandError::Unavailable(message) if message == "lock state is unknown"
    ));
}

struct Recording(Arc<Mutex<Vec<String>>>);

#[zbus::interface(name = "me.aresa.Glimpse.Session")]
impl Recording {
    async fn request_action(&self, action: &str) {
        self.0.lock().expect("calls").push(action.to_owned());
    }
}

struct Refusing(Arc<Mutex<Vec<String>>>);

#[zbus::interface(name = "me.aresa.Glimpse.Session")]
impl Refusing {
    async fn request_action(&self, action: &str) -> zbus::fdo::Result<()> {
        self.0.lock().expect("calls").push(action.to_owned());
        match action {
            "power-off" => Err(zbus::fdo::Error::LimitsExceeded("busy".into())),
            _ => Err(zbus::fdo::Error::AccessDenied("locked".into())),
        }
    }
}

struct Login1(Arc<Mutex<Vec<String>>>);

#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl Login1 {
    async fn suspend(&self, interactive: bool) {
        self.0
            .lock()
            .expect("calls")
            .push(format!("suspend:{interactive}"));
    }

    async fn hibernate(&self, interactive: bool) {
        self.0
            .lock()
            .expect("calls")
            .push(format!("hibernate:{interactive}"));
    }

    async fn reboot(&self, interactive: bool) {
        self.0
            .lock()
            .expect("calls")
            .push(format!("reboot:{interactive}"));
    }

    async fn power_off(&self, interactive: bool) {
        self.0
            .lock()
            .expect("calls")
            .push(format!("power-off:{interactive}"));
    }
}

fn method_error(name: &str, detail: &str) -> zbus::Error {
    zbus::Error::MethodError(
        zbus::names::OwnedErrorName::try_from(name.to_owned()).expect("error name"),
        Some(detail.to_owned()),
        zbus::message::Message::method_call("/me/aresa/Glimpse/Session", "RequestAction")
            .expect("call")
            .build(&())
            .expect("message"),
    )
}

fn compositor(cancel: &CancellationToken) -> crate::CompositorHandle {
    let (runtime, handle) = ServiceRuntime::<Compositor>::new(
        NoConfig,
        Buses::unavailable("no bus in tests"),
        cancel.clone(),
    );
    drop(runtime);
    handle
}

fn started(buses: Buses) -> (CancellationToken, SessionActionsHandle) {
    let cancel = CancellationToken::new();
    let (mut runtime, handle) =
        ServiceRuntime::<SessionActions>::new(NoConfig, buses, cancel.clone());
    let compositor = compositor(&cancel);
    tokio::spawn(async move {
        runtime.run(Dependencies { compositor }).await;
    });
    (cancel, handle)
}

#[tokio::test]
async fn reboot_and_power_off_reach_agents_without_a_system_bus() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    let calls = Arc::new(Mutex::new(Vec::new()));
    server
        .object_server()
        .at(
            glimpse_dbus::session::SESSION_PATH,
            Recording(calls.clone()),
        )
        .await
        .expect("export");
    own_name(&server, glimpse_dbus::session::AGENTS_BUS_NAME)
        .await
        .expect("name");
    let (_cancel, handle) = started(Buses::for_session(bus.connection().await));

    handle.run(Action::Reboot).await.expect("reboot");
    handle.run(Action::PowerOff).await.expect("power-off");
    let suspend = handle.run(Action::Suspend).await.expect_err("suspend");

    assert!(suspend.to_string().contains("no system bus"), "{suspend}");
    assert_eq!(
        calls.lock().expect("calls").as_slice(),
        ["reboot", "power-off"]
    );
}

#[tokio::test]
async fn a_refusing_agents_service_is_the_error_and_is_not_retried_elsewhere() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    let calls = Arc::new(Mutex::new(Vec::new()));
    server
        .object_server()
        .at(glimpse_dbus::session::SESSION_PATH, Refusing(calls.clone()))
        .await
        .expect("export");
    own_name(&server, glimpse_dbus::session::AGENTS_BUS_NAME)
        .await
        .expect("name");
    let (_cancel, handle) = started(Buses::for_session(bus.connection().await));

    let reboot = handle.run(Action::Reboot).await.expect_err("reboot");
    let power = handle.run(Action::PowerOff).await.expect_err("power-off");

    assert!(
        matches!(&reboot, CommandError::Unavailable(message) if message == "locked"),
        "{reboot}"
    );
    assert!(
        matches!(&power, CommandError::LimitExceeded(message) if message == "busy"),
        "{power}"
    );
    assert_eq!(
        calls.lock().expect("calls").as_slice(),
        ["reboot", "power-off"]
    );
}

#[tokio::test]
async fn suspend_calls_logind_and_power_actions_do_not() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    let calls = Arc::new(Mutex::new(Vec::new()));
    server
        .object_server()
        .at("/org/freedesktop/login1", Login1(calls.clone()))
        .await
        .expect("export");
    own_name(&server, "org.freedesktop.login1")
        .await
        .expect("name");
    let (_cancel, handle) = started(Buses::for_system(bus.connection().await));

    handle.run(Action::Suspend).await.expect("suspend");
    handle.run(Action::Hibernate).await.expect("hibernate");
    let reboot = handle.run(Action::Reboot).await.expect_err("reboot");
    let power = handle.run(Action::PowerOff).await.expect_err("power-off");

    assert!(reboot.to_string().contains("no session bus"), "{reboot}");
    assert!(power.to_string().contains("no session bus"), "{power}");
    assert_eq!(
        calls.lock().expect("calls").as_slice(),
        ["suspend:true", "hibernate:true"]
    );
}

#[tokio::test]
async fn a_missing_agents_name_fails_before_any_system_bus_requirement() {
    let bus = PrivateBus::start();
    let (_cancel, handle) = started(Buses::for_session(bus.connection().await));

    let error = handle.run(Action::PowerOff).await.expect_err("power-off");

    assert!(!error.to_string().contains("no system bus"), "{error}");
    assert!(!error.to_string().contains("no session bus"), "{error}");
}
