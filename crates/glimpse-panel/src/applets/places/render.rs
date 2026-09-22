use std::path::PathBuf;

use gettextrs::{gettext, ngettext};
use glimpse_services::{
    Drive as RemovableDrive, Place, PlacesState, RemovableCapacity, RemovableFailure,
    RemovableState, Volume as RemovableVolume, VolumeId,
};
use glimpse_widgets::{PlacesDrive, PlacesEntry, PlacesTrash, PlacesVolume};

pub const ICON: &str = "folder-symbolic";
pub const TRASH_ID: &str = "trash";
const NAME_CAP: usize = 60;

pub fn cap(name: &str) -> String {
    glimpse_utils::clean(name, NAME_CAP)
}

pub fn chip(places: &PlacesState, removable: &RemovableState) -> bool {
    !places.places.is_empty()
        || !places.bookmarks.is_empty()
        || !places.network.is_empty()
        || places.trash.is_some()
        || !removable.drives.is_empty()
}

pub fn tooltip(places: &PlacesState, removable: &RemovableState, format: Option<&str>) -> String {
    let Some(format) = format else {
        return gettext("Places");
    };
    let bookmarks = places.bookmarks.len().to_string();
    let places_count = places.places.len().to_string();
    let drives = removable.drives.len().to_string();
    crate::applets::tokens::render(format, |token| match token {
        "bookmarks" => Some(bookmarks.as_str()),
        "places" => Some(places_count.as_str()),
        "drives" => Some(drives.as_str()),
        _ => None,
    })
}

pub struct Listing {
    pub entries: Vec<PlacesEntry>,
    pub more: Option<String>,
}

fn shown(cap: usize, expanded: bool) -> usize {
    match expanded {
        true => usize::MAX,
        false => cap,
    }
}

fn more(total: usize, cap: usize, expanded: bool) -> Option<String> {
    let hidden = total.saturating_sub(cap);
    if hidden == 0 {
        return None;
    }
    Some(match expanded {
        true => gettext("Show fewer"),
        false => ngettext("{count} more", "{count} more", hidden as u32)
            .replace("{count}", &hidden.to_string()),
    })
}

fn place_entry(place: &Place) -> PlacesEntry {
    PlacesEntry {
        id: place.id.clone(),
        title: cap(&place.name),
        subtitle: String::new(),
        icon: place.kind.icon_name().to_owned(),
        busy: false,
    }
}

pub fn places(state: &PlacesState) -> Vec<PlacesEntry> {
    state.places.iter().map(place_entry).collect()
}

pub fn bookmarks(state: &PlacesState, cap: usize, expanded: bool) -> Listing {
    let entries = state
        .bookmarks
        .iter()
        .take(shown(cap, expanded))
        .map(place_entry)
        .collect();
    Listing {
        entries,
        more: more(state.bookmarks.len(), cap, expanded),
    }
}

pub fn network(state: &PlacesState) -> Vec<PlacesEntry> {
    state
        .network
        .iter()
        .map(|place| PlacesEntry {
            id: place.id.clone(),
            title: cap(&place.name),
            subtitle: place.path.display().to_string(),
            icon: place.kind.icon_name().to_owned(),
            busy: false,
        })
        .collect()
}

pub fn trash(state: &PlacesState) -> Option<PlacesTrash> {
    state.trash.map(|items| PlacesTrash {
        items: items as u32,
    })
}

fn place_path(state: &PlacesState, id: &str) -> Option<PathBuf> {
    state
        .places
        .iter()
        .chain(&state.bookmarks)
        .chain(&state.network)
        .find(|place| place.id == id)
        .map(|place| place.path.clone())
}

pub enum Activation {
    Open(PathBuf),
    OpenUri(&'static str),
    Mount(VolumeId),
}

pub fn activation(
    places: &PlacesState,
    removable: &RemovableState,
    id: &str,
) -> Option<Activation> {
    if id == TRASH_ID {
        return Some(Activation::OpenUri("trash:///"));
    }
    if let Some(path) = place_path(places, id) {
        return Some(Activation::Open(path));
    }
    locate_volume(removable, id).map(|volume| match &volume.mount {
        Some(mount) => Activation::Open(mount.at.clone()),
        None => Activation::Mount(volume.id.clone()),
    })
}

fn locate_volume<'a>(state: &'a RemovableState, id: &str) -> Option<&'a RemovableVolume> {
    for drive in &state.drives {
        match drive.volumes.as_slice() {
            [volume] if drive.id.as_str() == id => return Some(volume),
            [] => {}
            volumes => {
                if let Some(volume) = volumes.iter().find(|volume| compound(drive, volume) == id) {
                    return Some(volume);
                }
            }
        }
    }
    None
}

fn compound(drive: &RemovableDrive, volume: &RemovableVolume) -> String {
    format!("{}/{}", drive.id.as_str(), volume.id.as_str())
}

fn media_icon(media: glimpse_dbus::udisks2::Media) -> &'static str {
    use glimpse_dbus::udisks2::Media;
    match media {
        Media::OpticalCd
        | Media::OpticalCdR
        | Media::OpticalCdRw
        | Media::OpticalDvd
        | Media::OpticalDvdR
        | Media::OpticalDvdRw
        | Media::OpticalDvdRam
        | Media::OpticalDvdPlusR
        | Media::OpticalDvdPlusRDl
        | Media::OpticalDvdPlusRw
        | Media::OpticalDvdPlusRwDl
        | Media::OpticalBd
        | Media::OpticalBdR
        | Media::OpticalBdRe
        | Media::OpticalHddvd
        | Media::OpticalHddvdR
        | Media::OpticalHddvdRw
        | Media::OpticalMo
        | Media::OpticalMrw
        | Media::OpticalMrwW => "drive-optical-symbolic",
        Media::Thumb
        | Media::Flash
        | Media::FlashCf
        | Media::FlashMmc
        | Media::FlashMs
        | Media::FlashSd
        | Media::FlashSdhc
        | Media::FlashSdio
        | Media::FlashSdxc
        | Media::FlashSm => "media-flash-symbolic",
        Media::Floppy | Media::FloppyJaz | Media::FloppyZip => "media-floppy-symbolic",
        Media::Other => "drive-harddisk-usb-symbolic",
    }
}

fn drive_icon(drive: &RemovableDrive) -> &'static str {
    if drive.volumes.len() > 1 {
        return "drive-multidisk-symbolic";
    }
    media_icon(drive.media)
}

fn drive_subtitle(drive: &RemovableDrive) -> String {
    match drive.volumes.len() {
        0 if !drive.media_available => gettext("No media"),
        0 => glimpse_utils::size::bytes(drive.size),
        1 => String::new(),
        count => ngettext("{count} volume", "{count} volumes", count as u32)
            .replace("{count}", &count.to_string()),
    }
}

fn join(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|part| !part.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" · ")
}

fn capacity_text(capacity: RemovableCapacity) -> String {
    gettext("{available} free of {total}")
        .replace(
            "{available}",
            &glimpse_utils::size::bytes(capacity.available),
        )
        .replace("{total}", &glimpse_utils::size::bytes(capacity.total))
}

fn volume_subtitle(volume: &RemovableVolume) -> String {
    let fs = volume.fs.as_deref().unwrap_or_default();
    match &volume.mount {
        Some(mount) => match mount.capacity {
            Some(capacity) if capacity.available == 0 => join(&[&gettext("No space left"), fs]),
            Some(capacity) => join(&[&capacity_text(capacity), fs]),
            None => fs.to_owned(),
        },
        None => join(&[
            &gettext("Not mounted"),
            &glimpse_utils::size::bytes(volume.size),
            fs,
        ]),
    }
}

fn volume_title(volume: &RemovableVolume, drive: &RemovableDrive) -> String {
    let label = cap(&volume.label);
    match label.is_empty() {
        true => cap(&drive.name),
        false => label,
    }
}

fn fraction(capacity: Option<RemovableCapacity>) -> Option<f64> {
    let capacity = capacity?;
    if capacity.total == 0 {
        return None;
    }
    Some(1.0 - (capacity.available as f64 / capacity.total as f64))
}

fn volume_row(volume: &RemovableVolume, drive: &RemovableDrive, icon: &str) -> PlacesVolume {
    let capacity = volume.mount.as_ref().and_then(|mount| mount.capacity);
    PlacesVolume {
        id: volume.id.as_str().to_owned(),
        title: volume_title(volume, drive),
        subtitle: volume_subtitle(volume),
        icon: icon.to_owned(),
        value: String::new(),
        fraction: fraction(capacity),
        activatable: true,
        busy: drive.busy.is_some() || volume.busy.is_some(),
        read_only: volume.read_only,
        mounted: volume.mount.is_some(),
    }
}

fn drive_row(drive: &RemovableDrive) -> PlacesDrive {
    let icon = drive_icon(drive);
    PlacesDrive {
        id: drive.id.as_str().to_owned(),
        title: cap(&drive.name),
        subtitle: drive_subtitle(drive),
        icon: icon.to_owned(),
        value: String::new(),
        ejectable: drive.ejectable,
        busy: drive.busy.is_some(),
        activatable: false,
        dimmed: !drive.media_available && drive.volumes.is_empty(),
        volumes: drive
            .volumes
            .iter()
            .map(|volume| volume_row(volume, drive, icon))
            .collect(),
    }
}

pub fn devices(state: &RemovableState, cap: usize, expanded: bool) -> Devices {
    let take = shown(cap, expanded);
    let drives = state.drives.iter().take(take).map(drive_row).collect();
    Devices {
        drives,
        more: more(state.drives.len(), cap, expanded),
    }
}

pub struct Devices {
    pub drives: Vec<PlacesDrive>,
    pub more: Option<String>,
}

pub fn wording(failure: RemovableFailure) -> String {
    match failure {
        RemovableFailure::NotMounted => gettext("That drive is not mounted."),
        RemovableFailure::Busy => gettext("The drive is busy with something else."),
        RemovableFailure::MountedElsewhere => gettext("That drive is already mounted."),
        RemovableFailure::NotAuthorized => gettext("This computer is not authorized to do that."),
        RemovableFailure::NotSupported => gettext("That is not supported for this drive."),
        RemovableFailure::OptionNotPermitted => gettext("That option is not permitted."),
        RemovableFailure::TimedOut => gettext("The drive did not respond in time."),
        RemovableFailure::WouldWakeup => gettext("That would wake a sleeping drive."),
        RemovableFailure::Canceled => gettext("The action was cancelled."),
        RemovableFailure::Unknown => gettext("The drive could not complete that."),
    }
}

#[cfg(test)]
mod tests {
    use glimpse_dbus::udisks2::Media;
    use glimpse_services::{DriveId, RemovableMount};

    use super::*;

    fn place(id: &str, name: &str, kind: glimpse_services::PlacesKind, path: &str) -> Place {
        Place {
            id: id.to_owned(),
            name: name.to_owned(),
            path: PathBuf::from(path),
            kind,
        }
    }

    #[test]
    fn nothing_read_yet_shows_no_chip_and_a_known_empty_state_does() {
        assert!(!chip(&PlacesState::default(), &RemovableState::default()));

        let known = PlacesState {
            trash: Some(0),
            ..Default::default()
        };
        assert!(chip(&known, &RemovableState::default()));
    }

    #[test]
    fn a_hostile_bookmark_label_is_capped_before_it_reaches_a_dto() {
        let long: String = std::iter::repeat_n('é', 400).collect();
        let mut state = PlacesState::default();
        state.bookmarks.push(place(
            "bookmark",
            &long,
            glimpse_services::PlacesKind::Bookmark,
            "/home/alex/projects",
        ));

        let listing = bookmarks(&state, 8, false);

        assert_eq!(listing.entries.len(), 1);
        assert!(
            listing.entries[0].title.chars().count() <= NAME_CAP + 1,
            "a multibyte label must be capped by character count"
        );
    }

    #[test]
    fn bookmarks_past_the_cap_go_behind_an_overflow_row() {
        let mut state = PlacesState::default();
        for index in 0..10 {
            state.bookmarks.push(place(
                &format!("b{index}"),
                &format!("bookmark{index}"),
                glimpse_services::PlacesKind::Bookmark,
                "/home/alex",
            ));
        }

        let capped = bookmarks(&state, 8, false);
        assert_eq!(capped.entries.len(), 8);
        assert!(capped.more.is_some());

        let opened = bookmarks(&state, 8, true);
        assert_eq!(opened.entries.len(), 10);
        assert_eq!(opened.more.as_deref(), Some("Show fewer"));
    }

    fn drive(id: &str, volumes: Vec<RemovableVolume>) -> RemovableDrive {
        RemovableDrive {
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

    fn volume(id: &str, label: &str, fs: &str, mount: Option<RemovableMount>) -> RemovableVolume {
        RemovableVolume {
            id: glimpse_services::VolumeId::new(id),
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
    fn volumes_past_the_cap_go_behind_an_overflow_row() {
        let mut state = RemovableState::default();
        for index in 0..8 {
            state.drives.push(drive(&format!("/drive{index}"), vec![]));
        }

        let capped = devices(&state, 6, false);
        assert_eq!(capped.drives.len(), 6);
        assert!(capped.more.is_some());

        let opened = devices(&state, 6, true);
        assert_eq!(opened.drives.len(), 8);
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
    fn an_unmounted_volume_reads_not_mounted_then_size_then_filesystem() {
        let subtitle = volume_subtitle(&volume("/v1", "Backup", "exfat", None));
        assert_eq!(subtitle, "Not mounted · 64 GB · exfat");
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
        let subtitle = volume_subtitle(&volume("/v1", "Full", "exfat", Some(mount)));
        assert_eq!(subtitle, "No space left · exfat");
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
    }

    #[test]
    fn activating_an_unmounted_volume_asks_to_mount_it() {
        let mut removable = RemovableState::default();
        removable.drives.push(drive(
            "/drive0",
            vec![volume("/v1", "Backup", "exfat", None)],
        ));

        let places = PlacesState::default();
        match activation(&places, &removable, "/drive0") {
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
        let mut removable = RemovableState::default();
        removable.drives.push(drive(
            "/drive0",
            vec![volume("/v1", "Photos", "exfat", Some(mount))],
        ));

        let places = PlacesState::default();
        match activation(&places, &removable, "/drive0") {
            Some(Activation::Open(path)) => {
                assert_eq!(path, PathBuf::from("/run/media/alex/Photos"))
            }
            _ => panic!("expected an open request"),
        }
    }

    #[test]
    fn activating_a_place_opens_its_path() {
        let mut places = PlacesState::default();
        places.places.push(place(
            "home",
            "Home",
            glimpse_services::PlacesKind::Home,
            "/home/alex",
        ));

        match activation(&places, &RemovableState::default(), "home") {
            Some(Activation::Open(path)) => assert_eq!(path, PathBuf::from("/home/alex")),
            _ => panic!("expected an open request"),
        }
    }

    #[test]
    fn activating_trash_opens_the_trash_uri() {
        match activation(
            &PlacesState::default(),
            &RemovableState::default(),
            TRASH_ID,
        ) {
            Some(Activation::OpenUri(uri)) => assert_eq!(uri, "trash:///"),
            _ => panic!("expected an open request"),
        }
    }
}
