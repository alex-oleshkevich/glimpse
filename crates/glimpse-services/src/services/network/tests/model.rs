use super::*;

#[test]
fn only_a_device_a_user_can_act_on_reaches_the_model() {
    assert!(wanted(&device(2, true)), "wlp99s0");
    assert!(
        wanted(&device(1, true)),
        "an ethernet device, were there one"
    );
    assert!(wanted(&device(8, true)), "a modem, were there one");

    assert!(!wanted(&device(13, true)), "docker0 and the two br-*");
    assert!(!wanted(&device(20, false)), "the two unmanaged veths");
    assert!(!wanted(&device(32, true)), "lo");
    assert!(!wanted(&device(5, true)), "the bluetooth NAP");
    assert!(!wanted(&device(30, true)), "p2p-dev-wlp99s0");
}

#[test]
fn an_unmanaged_device_is_out_even_when_its_type_is_one_we_want() {
    assert!(!wanted(&device(2, false)));
}

#[test]
fn the_connected_beacon_survives_a_stronger_sibling_on_the_same_ssid() {
    let deduped = strongest(
        vec![
            access("Skylink", 70, 3, true),
            access("Skylink", 94, 3, false),
        ]
        .into_iter(),
    );

    assert_eq!(deduped.len(), 1);
    assert!(
        deduped[0].active,
        "the access point in use is often not the strongest, and dropping it for its sibling \
             leaves the bar reporting no connection while NetworkManager still has one"
    );
    assert_eq!(deduped[0].strength, 70);

    let other_way = strongest(
        vec![
            access("Skylink", 94, 3, false),
            access("Skylink", 70, 3, true),
        ]
        .into_iter(),
    );
    assert!(
        other_way[0].active,
        "and it survives whichever order the two arrive in"
    );
}

#[test]
fn access_points_deduplicate_by_ssid_keeping_the_strongest_whole_one() {
    let deduped = strongest(
        vec![
            access("RubinowyKlon1", 60, 3, false),
            access("RubinowyKlon1", 40, 1, false),
            access("UPC6516723", 60, 3, false),
            access("UPC6516723", 45, 3, false),
        ]
        .into_iter(),
    );

    assert_eq!(deduped.len(), 2, "four beacons, two networks");
    let rubinowy = deduped
        .iter()
        .find(|one| one.ssid.as_deref() == Some("RubinowyKlon1"))
        .expect("the network");
    assert_eq!(rubinowy.strength, 60);
    assert_eq!(
        rubinowy.security,
        nm::Security::read(3, 0, 392),
        "the whole stronger AP wins; merging flags would invent a security level"
    );
}

#[test]
fn the_connected_network_is_placed_first_even_when_it_is_not_the_strongest() {
    let ordered = strongest(
        vec![
            access("PLAY internet 2.4GHz_2938", 94, 3, false),
            access("Skylink", 70, 3, true),
        ]
        .into_iter(),
    );

    assert_eq!(
        ordered.first().and_then(|one| one.ssid.as_deref()),
        Some("Skylink")
    );
    assert!(ordered[0].active);
}

#[test]
fn an_unnamed_beacon_survives_dedup_as_its_own_row() {
    let deduped = strongest(
        vec![
            access("", 42, 1, false),
            access("", 30, 1, false),
            access("Skylink", 70, 3, false),
        ]
        .into_iter(),
    );

    let unnamed = deduped.iter().filter(|one| one.ssid.is_none()).count();
    assert_eq!(unnamed, 2, "two unnamed beacons are two networks, not one");
}

#[test]
fn the_relevance_allowlist_wakes_on_what_the_model_reads_and_not_on_churn() {
    assert!(relevant(&properties(vec![("Strength", owned(70u8))]), &[]));
    assert!(relevant(&properties(vec![("State", owned(100u32))]), &[]));
    assert!(relevant(
        &properties(vec![("Connectivity", owned(4u32))]),
        &[]
    ));

    assert!(
        !relevant(&properties(vec![("Bitrate", owned(960700u32))]), &[]),
        "Bitrate churned once in thirty idle seconds and feeds nothing"
    );
    assert!(
        !relevant(
            &properties(vec![("RxBytes", owned(1u64)), ("TxBytes", owned(2u64))]),
            &[]
        ),
        "Device.Statistics is writable by any client on the bus"
    );
}

#[test]
fn an_invalidated_property_the_model_reads_still_wakes_it() {
    assert!(relevant(&Properties::new(), &["Ssid".to_owned()]));
    assert!(!relevant(&Properties::new(), &["Bitrate".to_owned()]));
}

#[test]
fn a_radio_reports_blocked_only_when_the_hardware_says_so() {
    assert!(
        !Radio {
            enabled: false,
            hardware_enabled: true
        }
        .blocked(),
        "soft off is not blocked; the switch still works"
    );
    assert!(
        Radio {
            enabled: false,
            hardware_enabled: false
        }
        .blocked()
    );
}

#[test]
fn a_portal_does_not_reach_the_internet_but_an_unknown_connectivity_does() {
    let portal = NetworkState {
        connectivity: nm::Connectivity::Portal,
        ..NetworkState::default()
    };
    assert!(!portal.reaches_the_internet());

    let unchecked = NetworkState {
        connectivity: nm::Connectivity::Unknown,
        ..NetworkState::default()
    };
    assert!(
        unchecked.reaches_the_internet(),
        "a disabled connectivity check must not render as no internet"
    );
}
