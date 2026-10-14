use super::*;

#[test]
fn a_built_in_panel_is_never_a_ddc_candidate() {
    assert!(built_in("card1-eDP-1"));
    assert!(built_in("card0-LVDS-1"));
    assert!(built_in("card0-DSI-1"));
    assert!(!built_in("card1-DP-2"));
    assert!(!built_in("card1-HDMI-A-1"));
}

#[test]
fn an_id_outside_the_ddc_prefix_is_not_a_bus() {
    assert_eq!(bus_of("amdgpu_bl1"), None);
    assert_eq!(bus_of("ddc:"), None);
    assert_eq!(bus_of("ddc:nope"), None);
}

#[test]
fn a_well_formed_id_round_trips_the_bus_number() {
    assert_eq!(bus_of("ddc:5"), Some(5));
}

#[test]
fn the_connector_owned_aux_bus_is_tried_before_the_legacy_ddc_symlink() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let card = dir.path().join("card1-DP-2");
    std::fs::create_dir_all(&card).expect("creates the connector directory");

    std::fs::create_dir_all(dir.path().join("i2c-6")).expect("creates the legacy bus dir");
    std::os::unix::fs::symlink("../i2c-6", card.join("ddc")).expect("creates the ddc symlink");

    let aux = card.join("i2c-15");
    std::fs::create_dir_all(&aux).expect("creates the aux bus dir");
    std::fs::write(aux.join("name"), "AMDGPU DM aux hw bus 2\n").expect("writes its name");

    assert_eq!(candidate_buses(&card).collect::<Vec<_>>(), vec![15, 6]);
}

#[test]
fn a_connector_with_no_aux_child_falls_back_to_the_legacy_bus_alone() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let card = dir.path().join("card0-HDMI-A-1");
    std::fs::create_dir_all(&card).expect("creates the connector directory");
    std::fs::create_dir_all(dir.path().join("i2c-2")).expect("creates the legacy bus dir");
    std::os::unix::fs::symlink("../i2c-2", card.join("ddc")).expect("creates the ddc symlink");

    assert_eq!(candidate_buses(&card).collect::<Vec<_>>(), vec![2]);
}

#[test]
fn a_sibling_i2c_directory_with_a_non_aux_name_is_not_mistaken_for_the_aux_bus() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let card = dir.path().join("card1-DP-2");
    std::fs::create_dir_all(&card).expect("creates the connector directory");
    std::fs::create_dir_all(dir.path().join("i2c-6")).expect("creates the legacy bus dir");
    std::os::unix::fs::symlink("../i2c-6", card.join("ddc")).expect("creates the ddc symlink");

    let other = card.join("i2c-99");
    std::fs::create_dir_all(&other).expect("creates an unrelated i2c child");
    std::fs::write(other.join("name"), "Some Other Bus\n").expect("writes its name");

    assert_eq!(candidate_buses(&card).collect::<Vec<_>>(), vec![6]);
}

#[test]
fn a_disconnected_or_non_card_entry_is_skipped_without_touching_i2c() {
    let dir = tempfile::tempdir().expect("a temporary directory");

    assert!(connector_entry(dir.path(), "renderD128").is_none());

    let card = dir.path().join("card1-DP-1");
    std::fs::create_dir_all(&card).expect("creates the connector directory");
    std::fs::write(card.join("status"), "disconnected\n").expect("writes status");
    assert!(connector_entry(&card, "card1-DP-1").is_none());
}

#[test]
#[ignore = "reads the live /sys/class/drm and /dev/i2c-*, and needs a DDC/CI monitor plus \
                permission on those nodes (see the ddcutil udev rule); run under \
                just test-crate-compositor"]
fn enumerate_reads_a_real_ddc_monitor() {
    let entries = enumerate_blocking().entries;

    assert!(
        !entries.is_empty(),
        "this host has no connected, DDC/CI-capable external monitor, or lacks permission \
             on its /dev/i2c-* node"
    );
    let entry = entries.first().expect("at least one DDC entry");
    assert!(entry.id.starts_with(ID_PREFIX));
    assert!(entry.max_brightness > 0);

    let bus = bus_of(&entry.id).expect("the id round-trips its own bus number");
    write_blocking(bus, entry.brightness).expect("writing the current level back succeeds");
}

#[test]
fn a_silent_bus_on_a_connected_connector_is_unavailable_not_gone() {
    let dir = tempfile::tempdir().expect("temporary connector root");
    let root = dir.path();
    let connector = root.join("card1-DP-2");
    std::fs::create_dir_all(connector.join("i2c-15")).unwrap();
    std::fs::write(connector.join("i2c-15/name"), "AMDGPU DM aux hw bus 2").unwrap();
    std::fs::write(connector.join("status"), "connected\n").unwrap();
    assert!(bus_is_connected(root, 15));
    assert!(!bus_is_connected(root, 16));
    std::fs::write(connector.join("status"), "disconnected\n").unwrap();
    assert!(!bus_is_connected(root, 15));
}
