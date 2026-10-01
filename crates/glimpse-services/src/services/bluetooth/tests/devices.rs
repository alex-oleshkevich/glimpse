use super::*;

#[test]
fn only_one_segment_below_the_adapter_is_a_device() {
    assert!(is_device_path(ADAPTER, HEADSET));
    assert!(!is_device_path(ADAPTER, &format!("{HEADSET}/fd0")));
    assert!(!is_device_path(ADAPTER, &format!("{HEADSET}/sep1")));
    assert!(!is_device_path(ADAPTER, ADAPTER));
    assert!(!is_device_path(
        ADAPTER,
        "/org/bluez/hci1/dev_F8_4E_17_BC_EE_D5"
    ));
}

#[tokio::test]
async fn a_transport_feeds_a_codec_without_becoming_a_device() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    let mut objects = session();
    objects.insert(
        format!("{HEADSET}/fd0"),
        HashMap::from([(
            bluez::MEDIA_TRANSPORT1.to_owned(),
            properties(vec![
                (
                    "Device",
                    Value::ObjectPath(HEADSET.try_into().expect("a path")),
                ),
                ("Codec", 255u8.into()),
                (
                    "Configuration",
                    Value::from(vec![0x2du8, 0x01, 0x00, 0x00, 0xaa, 0x00, 0x04, 0x01]),
                ),
            ]),
        )]),
    );
    objects.insert(format!("{HEADSET}/sep1"), HashMap::new());

    enumerated(&mut service, &ctx, objects).await;

    let published = state.borrow();
    assert_eq!(published.devices.len(), 2);
    assert!(
        published
            .devices
            .iter()
            .all(|device| !device.id.as_str().contains("/fd0"))
    );
    assert_eq!(published.devices[0].codec, Some(Codec::Ldac));
}

#[tokio::test]
async fn devices_publish_connected_then_bonded_then_by_descending_signal() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    let mut objects = session();
    objects.insert(
        "/org/bluez/hci0/dev_00_00_00_00_00_01".to_owned(),
        HashMap::from([(
            bluez::DEVICE1.to_owned(),
            properties(vec![
                ("Address", "00:00:00:00:00:01".into()),
                ("Alias", "closer".into()),
                ("RSSI", (-40i16).into()),
            ]),
        )]),
    );
    objects.insert(
        "/org/bluez/hci0/dev_00_00_00_00_00_02".to_owned(),
        HashMap::from([(
            bluez::DEVICE1.to_owned(),
            properties(vec![
                ("Address", "00:00:00:00:00:02".into()),
                ("Alias", "bonded but away".into()),
                ("Bonded", true.into()),
            ]),
        )]),
    );

    enumerated(&mut service, &ctx, objects).await;

    let published = state.borrow();
    let names: Vec<&str> = published
        .devices
        .iter()
        .map(|device| device.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "WH-1000XM4",
            "bonded but away",
            "closer",
            "[TV] Samsung AU7172"
        ]
    );
}

#[tokio::test]
async fn an_unnamed_nearby_device_is_hidden_and_a_bonded_one_is_not() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    let mut objects = session();
    objects.insert("/org/bluez/hci0/dev_6B_2C_11_04_9A_71".to_owned(), beacon());
    let mut bonded = beacon();
    bonded.get_mut(bluez::DEVICE1).expect("a device").insert(
        "Bonded".to_owned(),
        OwnedValue::try_from(Value::from(true)).expect("a flag"),
    );
    objects.insert("/org/bluez/hci0/dev_11_11_11_11_11_11".to_owned(), bonded);

    enumerated(&mut service, &ctx, objects).await;

    let published = state.borrow();
    let names: Vec<&str> = published
        .devices
        .iter()
        .map(|device| device.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["WH-1000XM4", "6B-2C-11-04-9A-71", "[TV] Samsung AU7172"],
        "an unnamed device that is bonded is still the user's own"
    );
}
