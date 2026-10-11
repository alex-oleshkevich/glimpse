use super::*;
use zbus::zvariant::{ObjectPath, OwnedValue};

fn owned<'a, T: Into<Value<'a>>>(value: T) -> OwnedValue {
    OwnedValue::try_from(value.into()).expect("a representable value")
}

fn properties(pairs: Vec<(&str, OwnedValue)>) -> Properties {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

#[test]
fn a_partially_populated_drive_decodes_absent_fields_as_none() {
    let decoded = decode_drive(&properties(vec![("Vendor", owned("Samsung"))]));

    assert_eq!(decoded.vendor.as_deref(), Some("Samsung"));
    assert_eq!(decoded.model, None);
    assert_eq!(decoded.size, None);
    assert_eq!(decoded.media, None);
    assert_eq!(decoded.connection_bus, None);
}

#[test]
fn a_wrong_typed_property_decodes_as_absent_never_an_error() {
    let decoded = decode_drive(&properties(vec![("Removable", owned("not-a-bool"))]));

    assert_eq!(decoded.removable, None);
}

#[test]
fn a_nul_terminated_device_path_is_stripped() {
    let decoded = decode_block(&properties(vec![(
        "Device",
        owned(b"/dev/sdb1\0".to_vec()),
    )]));

    assert_eq!(decoded.device.as_deref(), Some("/dev/sdb1"));
}

#[test]
fn invalid_utf8_bytes_decode_lossily_and_never_panic() {
    let decoded = decode_block(&properties(vec![(
        "Device",
        owned(vec![0xffu8, 0xfe, b'h', b'i', 0]),
    )]));

    assert!(decoded.device.is_some());
}

#[test]
fn three_mount_points_one_empty_decode_to_two_stripped_entries() {
    let decoded = decode_filesystem(&properties(vec![(
        "MountPoints",
        owned(vec![
            b"/boot/efi\0".to_vec(),
            b"\0".to_vec(),
            b"/home\0".to_vec(),
        ]),
    )]));

    assert_eq!(
        decoded.mount_points,
        Some(vec!["/boot/efi".to_owned(), "/home".to_owned()])
    );
}

#[test]
fn a_root_object_path_is_absent_not_some_slash() {
    let decoded = decode_block(&properties(vec![
        ("Drive", owned(ObjectPath::try_from("/").unwrap())),
        (
            "CryptoBackingDevice",
            owned(ObjectPath::try_from("/").unwrap()),
        ),
    ]));

    assert_eq!(decoded.drive, None);
    assert_eq!(decoded.crypto_backing_device, None);

    let mdraid_absent = properties(vec![("MDRaid", owned(ObjectPath::try_from("/").unwrap()))]);
    assert_eq!(path(&mdraid_absent, "MDRaid"), None);
}

#[test]
fn every_media_value_decodes_to_a_distinct_variant() {
    const VALUES: &[(&str, Media)] = &[
        ("thumb", Media::Thumb),
        ("flash", Media::Flash),
        ("flash_cf", Media::FlashCf),
        ("flash_mmc", Media::FlashMmc),
        ("flash_ms", Media::FlashMs),
        ("flash_sd", Media::FlashSd),
        ("flash_sdhc", Media::FlashSdhc),
        ("flash_sdio", Media::FlashSdio),
        ("flash_sdxc", Media::FlashSdxc),
        ("flash_sm", Media::FlashSm),
        ("floppy", Media::Floppy),
        ("floppy_jaz", Media::FloppyJaz),
        ("floppy_zip", Media::FloppyZip),
        ("optical_cd", Media::OpticalCd),
        ("optical_cd_r", Media::OpticalCdR),
        ("optical_cd_rw", Media::OpticalCdRw),
        ("optical_dvd", Media::OpticalDvd),
        ("optical_dvd_r", Media::OpticalDvdR),
        ("optical_dvd_rw", Media::OpticalDvdRw),
        ("optical_dvd_ram", Media::OpticalDvdRam),
        ("optical_dvd_plus_r", Media::OpticalDvdPlusR),
        ("optical_dvd_plus_r_dl", Media::OpticalDvdPlusRDl),
        ("optical_dvd_plus_rw", Media::OpticalDvdPlusRw),
        ("optical_dvd_plus_rw_dl", Media::OpticalDvdPlusRwDl),
        ("optical_bd", Media::OpticalBd),
        ("optical_bd_r", Media::OpticalBdR),
        ("optical_bd_re", Media::OpticalBdRe),
        ("optical_hddvd", Media::OpticalHddvd),
        ("optical_hddvd_r", Media::OpticalHddvdR),
        ("optical_hddvd_rw", Media::OpticalHddvdRw),
        ("optical_mo", Media::OpticalMo),
        ("optical_mrw", Media::OpticalMrw),
        ("optical_mrw_w", Media::OpticalMrwW),
    ];

    for (value, expected) in VALUES {
        assert_eq!(
            Media::parse(value),
            Some(*expected),
            "{value} must decode to its own variant"
        );
    }

    let mut seen = std::collections::HashSet::new();
    for (_, variant) in VALUES {
        assert!(seen.insert(*variant), "{variant:?} decoded twice");
    }
}

#[test]
fn an_unrecognised_media_string_falls_back_rather_than_erroring() {
    assert_eq!(Media::parse("optical_holodeck"), Some(Media::Other));
    assert_eq!(Media::parse(""), None);
}

#[test]
fn a_hostile_multibyte_label_is_capped_by_characters_not_bytes() {
    let label = "文".repeat(200);
    let decoded = decode_block(&properties(vec![("IdLabel", owned(label))]));

    let label = decoded.id_label.expect("a label");
    assert!(label.chars().count() <= LABEL + 1);
}

#[test]
fn a_size_beyond_u32_survives_as_u64() {
    let decoded = decode_block(&properties(vec![("Size", owned(1_000_204_886_016u64))]));

    assert_eq!(decoded.size, Some(1_000_204_886_016));
}

#[test]
fn every_connection_bus_falls_back_when_unrecognised() {
    assert_eq!(ConnectionBus::parse("usb"), Some(ConnectionBus::Usb));
    assert_eq!(ConnectionBus::parse("sdio"), Some(ConnectionBus::Sdio));
    assert_eq!(
        ConnectionBus::parse("something-new"),
        Some(ConnectionBus::Unknown)
    );
    assert_eq!(ConnectionBus::parse(""), None);
}

#[test]
fn a_fixed_nvme_drive_decodes_its_sample_properties() {
    let decoded = decode_drive(&properties(vec![
        ("Removable", owned(false)),
        ("MediaRemovable", owned(false)),
        ("Ejectable", owned(false)),
        ("CanPowerOff", owned(false)),
        ("ConnectionBus", owned("")),
        ("Media", owned("")),
        ("MediaAvailable", owned(true)),
        ("SortKey", owned("00coldplug/00fixed/nvme1")),
        ("Size", owned(1_000_204_886_016u64)),
    ]));

    assert_eq!(decoded.removable, Some(false));
    assert_eq!(decoded.connection_bus, None);
    assert_eq!(decoded.media, None);
    assert_eq!(decoded.media_available, Some(true));
    assert_eq!(decoded.size, Some(1_000_204_886_016));
}

#[test]
fn empty_mount_points_are_present_but_absent_or_malformed_properties_are_not() {
    let decoded = decode_filesystem(&properties(vec![(
        "MountPoints", owned(Vec::<Vec<u8>>::new()),
    )]));
    assert_eq!(decoded.mount_points, Some(Vec::new()));
    assert_eq!(decode_filesystem(&Properties::new()).mount_points, None);
    assert_eq!(decode_filesystem(&properties(vec![("MountPoints", owned(false))])).mount_points, None);
}
