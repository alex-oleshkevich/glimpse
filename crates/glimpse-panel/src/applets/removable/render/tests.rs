use glimpse_dbus::udisks2::Media;
use glimpse_services::{DriveId, RemovableMount};

use super::*;

fn mounted() -> RemovableMount {
    RemovableMount {
        at: PathBuf::from("/run/media/alex/Photos"),
        capacity: Some(RemovableCapacity {
            available: 24_000_000_000,
            total: 64_000_000_000,
        }),
    }
}

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
    sized(id, label, fs, 64_000_000_000, mount)
}

fn sized(
    id: &str,
    label: &str,
    fs: &str,
    size: u64,
    mount: Option<RemovableMount>,
) -> ServiceVolume {
    ServiceVolume {
        id: VolumeId::new(id),
        label: label.to_owned(),
        fs: Some(fs.to_owned()),
        size,
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
fn an_empty_card_reader_is_hidden_and_takes_the_chip_with_it() {
    let mut reader = drive("/reader", vec![]);
    reader.media_available = false;
    let mut state = RemovableState::default();
    state.drives.push(reader);

    assert!(!chip(&state));
    let listing = devices(&state, 6);
    assert!(listing.drives.is_empty() && listing.rest.is_empty());
    assert_eq!(listing.more, None);
}

#[test]
fn a_hostile_volume_label_is_capped_before_it_reaches_a_dto() {
    let long: String = std::iter::repeat_n('é', 400).collect();
    let mut state = RemovableState::default();
    state.drives.push(drive(
        "/drive0",
        vec![
            volume("/v1", "Main", "exfat", None),
            sized("/v2", &long, "exfat", 1_000, None),
        ],
    ));

    let listing = devices(&state, 6);

    assert!(
        listing.drives[0].volumes[0].title.chars().count() <= NAME_CAP + 1,
        "a multibyte label must be capped by character count"
    );
}

#[test]
fn devices_past_the_cap_go_behind_an_overflow_row() {
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
fn the_cap_counts_devices_not_volumes() {
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
    assert_eq!(capped.drives.len(), 6);
    assert_eq!(capped.rest.len(), 2);
    assert_eq!(capped.more.as_deref(), Some("2 more"));
}

#[test]
fn the_main_volume_is_the_largest_and_a_tie_goes_to_the_first() {
    let ventoy = drive(
        "/d",
        vec![
            sized("/efi", "VTOYEFI", "vfat", 32_000_000, None),
            sized("/data", "Ventoy", "exfat", 57_700_000_000, None),
        ],
    );
    assert_eq!(main_volume(&ventoy).unwrap().id.as_str(), "/data");
    let row = drive_row(&ventoy);
    assert_eq!(row.open.as_deref(), Some("/data"));
    assert_eq!(row.volumes.len(), 1);
    assert_eq!(row.volumes[0].id, "/efi");

    let tied = drive(
        "/d",
        vec![
            sized("/first", "A", "exfat", 1_000, None),
            sized("/second", "B", "exfat", 1_000, None),
        ],
    );
    assert_eq!(main_volume(&tied).unwrap().id.as_str(), "/first");
    assert_eq!(drive_row(&tied).volumes[0].id, "/second");
}

#[test]
fn space_reads_the_short_free_form_and_names_the_main_volume_only_beside_others() {
    let single = drive("/d", vec![volume("/v", "Photos", "exfat", Some(mounted()))]);
    let row = drive_row(&single);
    let space = row.space.expect("a meter");
    assert_eq!(space.title, "Space");
    assert_eq!(space.text, "24 GB free");
    assert!(row.mounted);

    let pair = drive(
        "/d",
        vec![
            volume("/v", "Ventoy", "exfat", Some(mounted())),
            sized("/e", "EFI", "vfat", 32_000_000, None),
        ],
    );
    assert_eq!(drive_row(&pair).space.expect("a meter").title, "Ventoy");

    let unlabeled = drive(
        "/d",
        vec![
            volume("/v", "", "exfat", Some(mounted())),
            sized("/e", "EFI", "vfat", 32_000_000, None),
        ],
    );
    assert_eq!(drive_row(&unlabeled).space.expect("a meter").title, "Space");
}

#[test]
fn an_unmounted_main_volume_shows_no_meter_and_no_check() {
    let row = drive_row(&drive("/d", vec![volume("/v", "Backup", "exfat", None)]));
    assert!(row.space.is_none());
    assert!(!row.mounted);
    assert_eq!(row.open.as_deref(), Some("/v"));
}

#[test]
fn an_extra_volume_reads_its_label_or_filesystem_with_its_size() {
    let labeled = extra(&sized("/e", "VTOYEFI", "vfat", 32_000_000, None));
    assert_eq!(labeled.title, "VTOYEFI");
    assert_eq!(labeled.size, "32 MB");
    assert!(!labeled.mounted);
    let bare = extra(&sized("/e", "", "vfat", 32_000_000, Some(mounted())));
    assert_eq!(bare.title, "vfat");
    assert!(bare.mounted);
}

#[test]
fn a_full_main_volume_says_so_instead_of_zero_free() {
    let full = RemovableMount {
        at: PathBuf::from("/run/media/alex/Full"),
        capacity: Some(RemovableCapacity {
            available: 0,
            total: 64_000_000_000,
        }),
    };
    let row = drive_row(&drive(
        "/d",
        vec![volume("/v", "Full", "exfat", Some(full))],
    ));
    assert_eq!(row.problem, "No space left");
    assert_eq!(row.space.expect("a meter").fraction, 1.0);
}

#[test]
fn a_refused_eject_reads_in_use_by_an_app() {
    let mut busy = drive("/d", vec![volume("/v", "Media", "exfat", Some(mounted()))]);
    busy.failure = Some(RemovableFailure::Busy);
    assert_eq!(drive_row(&busy).problem, "In use by an app");
    busy.failure = Some(RemovableFailure::TimedOut);
    assert_eq!(drive_row(&busy).problem, "");
}

#[test]
fn work_on_any_volume_spins_the_device_row() {
    let mut working = volume("/v", "Media", "exfat", None);
    working.busy = Some(glimpse_services::RemovableBusy::Mounting);
    assert!(drive_row(&drive("/d", vec![working])).busy);
}

#[test]
fn removal_is_offered_when_the_drive_can_leave_or_a_volume_is_mounted() {
    let mut stick = drive("/d", vec![volume("/v", "Media", "exfat", None)]);
    stick.ejectable = false;
    assert!(!drive_row(&stick).removable);
    stick.can_power_off = true;
    assert!(drive_row(&stick).removable);
    stick.can_power_off = false;
    stick.volumes[0].mount = Some(mounted());
    assert!(drive_row(&stick).removable);
}

#[test]
fn a_multi_volume_drive_wears_the_multidisk_icon() {
    let drive = drive(
        "/drive0",
        vec![
            volume("/v1", "Photos", "exfat", None),
            volume("/v2", "Backup", "exfat", None),
        ],
    );
    assert_eq!(drive_icon(&drive), "drive-multidisk-symbolic");
}

#[test]
fn activating_an_unmounted_volume_asks_to_mount_it() {
    let mut state = RemovableState::default();
    state.drives.push(drive(
        "/drive0",
        vec![volume("/v1", "Backup", "exfat", None)],
    ));

    match activation(&state, "/v1") {
        Some(Activation::Mount(id)) => assert_eq!(id.as_str(), "/v1"),
        _ => panic!("expected a mount request"),
    }
}

#[test]
fn activating_a_mounted_volume_opens_its_mount_path() {
    let mut state = RemovableState::default();
    state.drives.push(drive(
        "/drive0",
        vec![volume("/v1", "Photos", "exfat", Some(mounted()))],
    ));

    match activation(&state, "/v1") {
        Some(Activation::Open(path)) => {
            assert_eq!(path, PathBuf::from("/run/media/alex/Photos"))
        }
        _ => panic!("expected an open request"),
    }
}
