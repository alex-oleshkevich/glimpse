use super::service::{enumerate, harness};
use super::*;
use zbus::zvariant::ObjectPath;

const DEVICE: &str = "/org/freedesktop/NetworkManager/Devices/3";
const POINT: &str = "/org/freedesktop/NetworkManager/AccessPoint/7";

fn object(path: &str) -> OwnedValue {
    owned(ObjectPath::try_from(path).expect("a path"))
}

fn wifi_in(state: u32) -> Objects {
    Objects::from([
        (
            nm::MANAGER.to_owned(),
            HashMap::from([(
                nm::MANAGER1.to_owned(),
                properties(vec![
                    ("NetworkingEnabled", owned(true)),
                    ("WirelessEnabled", owned(true)),
                    ("WirelessHardwareEnabled", owned(true)),
                    ("Connectivity", owned(1u32)),
                ]),
            )]),
        ),
        (
            DEVICE.to_owned(),
            HashMap::from([
                (
                    nm::DEVICE1.to_owned(),
                    properties(vec![
                        ("DeviceType", owned(2u32)),
                        ("Managed", owned(true)),
                        ("State", owned(state)),
                        ("Interface", owned("wlan0")),
                    ]),
                ),
                (
                    nm::WIRELESS1.to_owned(),
                    properties(vec![
                        (
                            "AccessPoints",
                            owned(vec![ObjectPath::try_from(POINT).expect("a path")]),
                        ),
                        ("ActiveAccessPoint", object(POINT)),
                    ]),
                ),
            ]),
        ),
        (
            POINT.to_owned(),
            HashMap::from([(
                nm::ACCESS_POINT1.to_owned(),
                properties(vec![
                    ("Ssid", owned(b"Cafe".to_vec())),
                    ("Strength", owned(70u8)),
                    ("Flags", owned(1u32)),
                    ("RsnFlags", owned(0x100u32)),
                ]),
            )]),
        ),
    ])
}

async fn state_in(code: u32) -> NetworkState {
    let mut harness = harness().await;
    enumerate(&mut harness, wifi_in(code)).await;
    harness.state.borrow_and_update().clone()
}

#[tokio::test]
async fn review_a_network_still_authenticating_is_not_reported_connected() {
    let state = state_in(60).await;
    assert_eq!(state.networks.len(), 1);
    assert!(state.connected().is_none());
    assert_eq!(state.networks[0].busy, Some(Busy::Connecting));
}

#[tokio::test]
async fn an_activated_device_reports_its_network_connected() {
    let state = state_in(100).await;
    assert!(state.networks[0].active);
    assert_eq!(state.networks[0].busy, None);
}

#[tokio::test]
async fn a_failed_device_leaves_its_network_neither_active_nor_busy() {
    let state = state_in(120).await;
    assert!(!state.networks[0].active);
    assert_eq!(state.networks[0].busy, None);
}
