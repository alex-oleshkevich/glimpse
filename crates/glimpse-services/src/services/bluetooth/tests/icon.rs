use super::*;

#[test]
fn a_machine_with_no_radio_reports_no_icon() {
    assert!(BluetoothState::default().icon_name().is_none());
}

#[test]
fn the_icon_follows_the_power_state() {
    assert_eq!(state(Power::Off, false, vec![]).icon_name().unwrap(), OFF);
    assert_eq!(
        state(Power::Blocked, false, vec![]).icon_name().unwrap(),
        BLOCKED
    );
    assert_eq!(
        state(Power::Enabling, false, vec![]).icon_name().unwrap(),
        IDLE,
        "a transition shows the state being entered"
    );
    assert_eq!(
        state(Power::Disabling, false, vec![]).icon_name().unwrap(),
        OFF
    );
    assert_eq!(
        state(Power::On, true, vec![]).icon_name().unwrap(),
        SCANNING
    );
    assert_eq!(state(Power::On, false, vec![]).icon_name().unwrap(), IDLE);
}

#[test]
fn a_disconnected_device_does_not_light_the_icon() {
    let some = state(
        Power::On,
        false,
        vec![device("Buds", true), device("Mouse", false)],
    );
    assert_eq!(some.icon_name().unwrap(), ACTIVE);

    let none = state(Power::On, false, vec![device("Mouse", false)]);
    assert_eq!(none.icon_name().unwrap(), IDLE);
}
