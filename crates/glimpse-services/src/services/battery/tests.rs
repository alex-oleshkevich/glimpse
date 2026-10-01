use super::*;
use glimpse_dbus::{Buses, upower::ChargeState};
use tokio_util::sync::CancellationToken;

use crate::ServiceRuntime;

fn battery(
    path: &str,
    present: bool,
    percentage: u8,
    power_supply: bool,
) -> (String, DeviceProperties) {
    (
        path.to_owned(),
        DeviceProperties {
            native_path: Some("BAT1".to_owned()),
            vendor: Some("ASUS".to_owned()),
            model: Some("A32-K55".to_owned()),
            kind: DeviceKind::Battery,
            power_supply,
            is_present: present,
            percentage,
            state: ChargeState::Full,
            icon_name: Some("battery-full-charged-symbolic".to_owned()),
            capacity_pct: Some(97),
            energy_mwh: Some(87_042),
            energy_full_mwh: Some(87_042),
            energy_full_design_mwh: Some(90_005),
            voltage_mv: Some(17_243),
            charge_threshold: Some(ChargeThreshold {
                enabled: false,
                start: Some(75),
                end: Some(80),
            }),
            ..DeviceProperties::default()
        },
    )
}

fn charge(percentage: u8, state: ChargeState) -> Charge {
    Charge {
        percentage,
        state,
        icon_name: "battery-full-charged-symbolic".to_owned(),
        time_to_empty: None,
        time_to_full: None,
        energy_rate_mw: None,
        warning: WarningLevel::None,
    }
}

#[test]
fn the_chip_uses_upower_icon_name_when_it_is_set() {
    let mut charging = charge(40, ChargeState::Charging);
    charging.icon_name = "battery-full-charging-symbolic".to_owned();
    assert_eq!(charging.icon_name(), "battery-full-charging-symbolic");
    charging.icon_name.clear();
    assert_eq!(charging.icon_name(), "battery-level-40-charging-symbolic");

    let mut full = charge(100, ChargeState::Charging);
    full.icon_name.clear();
    assert_eq!(
        full.icon_name(),
        "battery-level-100-charged-symbolic",
        "Adwaita has no battery-level-100-charging-symbolic"
    );

    let mut waiting = charge(55, ChargeState::PendingCharge);
    waiting.icon_name.clear();
    assert_eq!(waiting.icon_name(), "battery-level-50-plugged-in-symbolic");
}

#[test]
fn display_device_is_the_chip_and_not_an_internal_row() {
    let devices = BTreeMap::from([
        battery(upower::DISPLAY_DEVICE, true, 100, true),
        battery(
            "/org/freedesktop/UPower/devices/battery_BAT1",
            true,
            100,
            true,
        ),
        (
            "/org/freedesktop/UPower/devices/line_power_ACAD".to_owned(),
            DeviceProperties {
                kind: DeviceKind::LinePower,
                online: true,
                icon_name: Some("ac-adapter-symbolic".to_owned()),
                ..DeviceProperties::default()
            },
        ),
    ]);

    let (display, internals, peripherals) = assemble(&devices, Some(upower::DISPLAY_DEVICE));
    assert_eq!(display.unwrap().percentage, 100);
    assert_eq!(internals.len(), 1);
    assert_eq!(internals[0].model.as_deref(), Some("A32-K55"));
    assert!(
        internals[0].charge_threshold.is_some(),
        "details come from BAT1, which supports a charge limit"
    );
    assert!(peripherals.is_empty());
}

#[test]
fn a_missing_display_device_hides_the_chip() {
    let devices = BTreeMap::from([battery(
        "/org/freedesktop/UPower/devices/battery_BAT1",
        true,
        80,
        true,
    )]);
    let (display, internals, _) = assemble(&devices, None);
    assert!(display.is_none(), "the chip reads DisplayDevice only");
    assert_eq!(internals.len(), 1);
}

#[test]
fn an_absent_display_device_is_no_chip() {
    let devices = BTreeMap::from([battery(upower::DISPLAY_DEVICE, false, 0, true)]);
    let (display, internals, _) = assemble(&devices, None);
    assert!(display.is_none());
    assert!(internals.is_empty());
}

#[test]
fn a_mouse_is_a_peripheral_and_not_an_internal_supply() {
    let devices = BTreeMap::from([(
        "/org/freedesktop/UPower/devices/mouse_hidpp".to_owned(),
        DeviceProperties {
            kind: DeviceKind::Mouse,
            is_present: true,
            power_supply: false,
            model: Some("MX Master 3S".to_owned()),
            percentage: 41,
            state: ChargeState::Discharging,
            icon_name: Some("input-mouse-symbolic".to_owned()),
            ..DeviceProperties::default()
        },
    )]);
    let (display, internals, peripherals) = assemble(&devices, Some(upower::DISPLAY_DEVICE));
    assert!(display.is_none() && internals.is_empty());
    assert_eq!(peripherals.len(), 1);
    assert_eq!(peripherals[0].name, "MX Master 3S");
    assert_eq!(peripherals[0].charge.percentage, 41);
}

#[tokio::test]
async fn no_system_bus_degrades_and_shows_nothing() {
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Battery>::new(
        NoConfig,
        Buses::unavailable("no bus in tests"),
        cancel.clone(),
    );
    let running = tokio::spawn(async move { runtime.run(Dependencies::default()).await });
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
    let snapshot = handle.snapshot();
    assert!(snapshot.display.is_none());
    assert!(snapshot.internals.is_empty());
    assert!(snapshot.profile.is_none());
    assert!(
        handle.health().borrow().unavailable_reason().is_some(),
        "a missing bus is degraded, not a silent empty bar"
    );
    cancel.cancel();
    let _ = running.await;
}

#[tokio::test]
async fn a_source_contributes_devices_until_it_closes() {
    let phone = Peripheral {
        path: "kdeconnect:a".to_owned(),
        kind: DeviceKind::Phone,
        name: "Pixel".to_owned(),
        icon_name: None,
        charge: charge(76, ChargeState::Discharging),
    };
    let (feed, receiver) = watch::channel(vec![phone.clone()]);
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Battery>::new(
        NoConfig,
        Buses::unavailable("no bus in tests"),
        cancel.clone(),
    );
    let dependencies = Dependencies {
        sources: vec![Source::new(receiver, |devices: &Vec<Peripheral>| {
            devices.clone()
        })],
    };
    let running = tokio::spawn(async move { runtime.run(dependencies).await });
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
    assert_eq!(handle.snapshot().devices, vec![phone]);

    drop(feed);
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
    assert!(
        handle.snapshot().devices.is_empty(),
        "a closed source leaves no stale row"
    );
    cancel.cancel();
    let _ = running.await;
}

#[tokio::test]
async fn a_profile_command_without_a_bus_is_unavailable() {
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Battery>::new(
        NoConfig,
        Buses::unavailable("no bus in tests"),
        cancel.clone(),
    );
    let running = tokio::spawn(async move { runtime.run(Dependencies::default()).await });
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    let error = handle
        .set_profile("power-saver")
        .await
        .expect_err("there is no daemon to write to");
    assert!(matches!(error, CommandError::Unavailable(_)));
    cancel.cancel();
    let _ = running.await;
}
