use glimpse_dbus::udisks2::Media;
use glimpse_services::{DriveId, RemovableMount};

use super::*;

fn drive(id: &str, volumes: Vec<ServiceVolume>) -> ServiceDrive {
    ServiceDrive {
        id: DriveId::new(id),
        name: "Removable drive".to_owned(),
        media: Media::Other,
        size: 64_000_000_000,
        ejectable: true,
        can_power_off: false,
        media_available: true,
        busy: None,
        failure: None,
        volumes,
    }
}

fn volume(id: &str, label: &str, fs: &str, mount: Option<RemovableMount>) -> ServiceVolume {
    ServiceVolume {
        id: VolumeId::new(id),
        label: label.to_owned(),
        fs: Some(fs.to_owned()),
        size: 64_000_000_000,
        read_only: false,
        encryption: glimpse_services::RemovableEncryption::None,
        mount,
        busy: None,
        failure: None,
    }
}

#[test]
fn no_drives_shows_no_chip_and_one_drive_does() {
    assert!(!chip(&RemovableState::default()));

    let mut state = RemovableState::default();
    state.drives.push(drive("/drive0", vec![]));
    assert!(chip(&state));
}

#[test]
fn a_hostile_volume_label_is_capped_before_it_reaches_a_dto() {
    let long: String = std::iter::repeat_n('é', 400).collect();
    let mut state = RemovableState::default();
    state
        .drives
        .push(drive("/drive0", vec![volume("/v1", &long, "exfat", None)]));

    let listing = devices(&state, 6);

    assert_eq!(listing.drives[0].volumes.len(), 1);
    assert!(
        listing.drives[0].volumes[0].title.chars().count() <= NAME_CAP + 1,
        "a multibyte label must be capped by character count"
    );
}

#[test]
fn volumes_past_the_cap_go_behind_an_overflow_row() {
    let mut state = RemovableState::default();
    for index in 0..8 {
        state.drives.push(drive(&format!("/drive{index}"), vec![]));
    }

    let capped = devices(&state, 6);
    assert_eq!(capped.drives.len(), 6);
    assert_eq!(capped.rest.len(), 2);
    assert_eq!(capped.more.as_deref(), Some("2 more"));
}

#[test]
fn the_cap_counts_volume_rows_and_may_split_a_drive() {
    let mut state = RemovableState::default();
    for index in 0..8 {
        state.drives.push(drive(
            &format!("/drive{index}"),
            vec![
                volume("/a", "A", "exfat", None),
                volume("/b", "B", "exfat", None),
                volume("/c", "C", "exfat", None),
            ],
        ));
    }

    let capped = devices(&state, 6);
    assert_eq!(capped.drives.len(), 2);
    assert_eq!(capped.rest.len(), 6);
    assert_eq!(capped.more.as_deref(), Some("18 more"));

    let split = devices(&state, 4);
    assert_eq!(split.drives.len(), 2);
    assert_eq!(split.drives[1].volumes.len(), 1);
    assert_eq!(split.rest[0].volumes.len(), 2);
    assert_eq!(split.rest[0].id, split.drives[1].id);
    assert_eq!(split.rest[0].volumes[0].id, "/b");
}

#[test]
fn a_capacity_readout_reads_free_of_total_with_no_percentage() {
    let capacity = RemovableCapacity {
        available: 24_000_000_000,
        total: 64_000_000_000,
    };
    let text = capacity_text(capacity);

    assert!(!text.contains('%'));
    assert!(text.contains("free of"));
}

#[test]
fn an_unmounted_volume_reads_not_mounted_then_size_and_keeps_its_filesystem_in_the_card() {
    let unmounted = volume("/v1", "Backup", "exfat", None);
    assert_eq!(
        volume_facts(&unmounted),
        [
            Fact::new("Status", "Not mounted"),
            Fact::new("Size", "64 GB"),
            Fact::new("Filesystem", "exfat"),
        ]
    );
}

#[test]
fn a_mounted_volume_with_no_free_space_says_so_instead_of_zero_free() {
    let mount = RemovableMount {
        at: PathBuf::from("/run/media/alex/Full"),
        capacity: Some(RemovableCapacity {
            available: 0,
            total: 64_000_000_000,
        }),
    };
    let mut full = volume("/v1", "Full", "exfat", Some(mount));
    full.read_only = true;
    let row = volume_row(&full, &drive("/drive0", vec![full.clone()]), ICON);
    assert_eq!(
        row.problem, "No space left",
        "a full volume is a problem, so its row says so in amber"
    );
    assert_eq!(
        row.facts,
        [
            Fact::new("Filesystem", "exfat"),
            Fact::new("Mounted at", "/run/media/alex/Full"),
            Fact::new("Access", "Read-only"),
        ]
    );
    assert_eq!(row.fraction, Some(1.0));
    assert!(row.space.starts_with("0 B free of"));
}

#[test]
fn a_mounted_volume_carries_its_capacity_in_the_meter_not_in_the_facts() {
    let mount = RemovableMount {
        at: PathBuf::from("/run/media/alex/Photos"),
        capacity: Some(RemovableCapacity {
            available: 24_000_000_000,
            total: 64_000_000_000,
        }),
    };
    let photos = volume("/v1", "Photos", "exfat", Some(mount));
    let row = volume_row(&photos, &drive("/drive0", vec![photos.clone()]), ICON);
    assert_eq!(
        row.space,
        capacity_text(photos.mount.as_ref().unwrap().capacity.unwrap())
    );
    assert!(row.facts.iter().all(|fact| fact.label != "Space"));
}

#[test]
fn an_unlabeled_volume_falls_back_to_its_drives_name() {
    let drive = drive("/drive0", vec![volume("/v1", "", "exfat", None)]);
    let title = volume_title(&drive.volumes[0], &drive);
    assert_eq!(title, "Removable drive");
}

#[test]
fn a_multi_volume_drive_groups_its_volumes_under_a_multidisk_icon() {
    let drive = drive(
        "/drive0",
        vec![
            volume("/v1", "Photos", "exfat", None),
            volume("/v2", "Backup", "exfat", None),
        ],
    );

    assert_eq!(drive_icon(&drive), "drive-multidisk-symbolic");
    let row = drive_row(&drive);
    assert_eq!(row.volumes.len(), 2);
    assert_eq!(row.volumes[0].title, "Removable drive \u{b7} Photos");
    assert_eq!(row.volumes[1].title, "Removable drive \u{b7} Backup");
}

#[test]
fn activating_an_unmounted_volume_asks_to_mount_it() {
    let mut state = RemovableState::default();
    state.drives.push(drive(
        "/drive0",
        vec![volume("/v1", "Backup", "exfat", None)],
    ));

    match activation(&state, "/drive0//v1") {
        Some(Activation::Mount(id)) => assert_eq!(id.as_str(), "/v1"),
        _ => panic!("expected a mount request"),
    }
}

#[test]
fn activating_a_mounted_volume_opens_its_mount_path() {
    let mount = RemovableMount {
        at: PathBuf::from("/run/media/alex/Photos"),
        capacity: None,
    };
    let mut state = RemovableState::default();
    state.drives.push(drive(
        "/drive0",
        vec![volume("/v1", "Photos", "exfat", Some(mount))],
    ));

    match activation(&state, "/drive0//v1") {
        Some(Activation::Open(path)) => {
            assert_eq!(path, PathBuf::from("/run/media/alex/Photos"))
        }
        _ => panic!("expected an open request"),
    }
}

#[test]
fn a_single_volume_drive_keeps_the_bare_volume_label() {
    let single = drive("/drive1", vec![volume("/v3", "Photos", "exfat", None)]);
    assert_eq!(drive_row(&single).volumes[0].title, "Photos");
}
