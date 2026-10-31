use super::*;

fn device(id: &str, default: bool) -> Device {
    Device {
        id: DeviceId::new(id),
        index: 0,
        name: id.to_owned(),
        port: None,
        icon_name: None,
        form_factor: None,
        volume: 100,
        muted: false,
        default,
    }
}

#[test]
fn default_output_returns_flagged_device() {
    let state = AudioState {
        outputs: vec![device("analog", false), device("headset", true)],
        ..Default::default()
    };

    assert_eq!(state.default_output().unwrap().id, DeviceId::new("headset"));
}

#[test]
fn app_role_is_none_for_missing_direction() {
    let app = App {
        id: AppId::new("firefox"),
        name: "Firefox".to_owned(),
        icon_name: None,
        app_id: None,
        binary: None,
        playback: Some(Role {
            volume: 80,
            muted: false,
            adjustable: true,
            device: DeviceId::new("headset"),
            corked: false,
            streams: Vec::new(),
        }),
        capture: None,
    };

    assert!(app.role(Direction::Output).is_some());
    assert!(app.role(Direction::Input).is_none());
}

#[test]
fn device_does_not_cross_directions() {
    let state = AudioState {
        outputs: vec![device("headset", true)],
        inputs: vec![device("microphone", true)],
        ..Default::default()
    };

    assert!(
        state
            .device(Direction::Input, &DeviceId::new("headset"))
            .is_none()
    );
}

#[test]
fn scaled_preserves_mix() {
    let role = Role {
        volume: 80,
        muted: false,
        adjustable: true,
        device: DeviceId::new("headset"),
        corked: false,
        streams: vec![
            StreamRef {
                index: 1,
                volume: 20,
                device: DeviceId::new("headset"),
            },
            StreamRef {
                index: 2,
                volume: 80,
                device: DeviceId::new("headset"),
            },
        ],
    };

    assert_eq!(role.scaled(40), vec![(1, 10), (2, 40)]);
}

#[test]
fn scaled_does_not_divide_by_zero() {
    let role = Role {
        volume: 0,
        muted: true,
        adjustable: true,
        device: DeviceId::new("headset"),
        corked: false,
        streams: vec![StreamRef {
            index: 1,
            volume: 0,
            device: DeviceId::new("headset"),
        }],
    };

    assert_eq!(role.scaled(40), vec![(1, 40)]);
}
