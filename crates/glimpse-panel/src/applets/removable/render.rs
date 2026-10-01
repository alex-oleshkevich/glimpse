use std::path::PathBuf;

use gettextrs::{gettext, ngettext};
use glimpse_services::{
    Drive as ServiceDrive, RemovableCapacity, RemovableFailure, RemovableState,
    Volume as ServiceVolume, VolumeId,
};
use glimpse_widgets::{Fact, RemovableDrive, RemovableVolume};

pub const ICON: &str = "drive-removable-media-symbolic";
const NAME_CAP: usize = 60;

fn cap(name: &str) -> String {
    glimpse_utils::clean(name, NAME_CAP)
}

pub fn chip(removable: &RemovableState) -> bool {
    !removable.drives.is_empty()
}

pub fn tooltip(removable: &RemovableState, format: Option<&str>) -> String {
    let Some(format) = format else {
        return gettext("Removable");
    };
    let drives = removable.drives.len().to_string();
    let volumes = removable
        .drives
        .iter()
        .map(|drive| drive.volumes.len())
        .sum::<usize>()
        .to_string();
    crate::applets::tokens::render(format, |token| match token {
        "drives" => Some(drives.as_str()),
        "volumes" => Some(volumes.as_str()),
        _ => None,
    })
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

pub enum Activation {
    Open(PathBuf),
    Mount(VolumeId),
}

pub fn activation(state: &RemovableState, id: &str) -> Option<Activation> {
    locate_volume(state, id).map(|volume| match &volume.mount {
        Some(mount) => Activation::Open(mount.at.clone()),
        None => Activation::Mount(volume.id.clone()),
    })
}

fn locate_volume<'a>(state: &'a RemovableState, id: &str) -> Option<&'a ServiceVolume> {
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

fn compound(drive: &ServiceDrive, volume: &ServiceVolume) -> String {
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

fn drive_icon(drive: &ServiceDrive) -> &'static str {
    if drive.volumes.len() > 1 {
        return "drive-multidisk-symbolic";
    }
    media_icon(drive.media)
}

fn drive_facts(drive: &ServiceDrive) -> Vec<Fact> {
    if !drive.media_available {
        return vec![Fact::new(gettext("Status"), gettext("No media"))];
    }
    let mut facts = vec![Fact::new(
        gettext("Size"),
        glimpse_utils::size::bytes(drive.size),
    )];
    if drive.volumes.len() > 1 {
        facts.push(Fact::new(
            gettext("Volumes"),
            drive.volumes.len().to_string(),
        ));
    }
    facts
}

fn capacity_text(capacity: RemovableCapacity) -> String {
    gettext("{available} free of {total}")
        .replace(
            "{available}",
            &glimpse_utils::size::bytes(capacity.available),
        )
        .replace("{total}", &glimpse_utils::size::bytes(capacity.total))
}

fn full(volume: &ServiceVolume) -> bool {
    volume
        .mount
        .as_ref()
        .and_then(|mount| mount.capacity)
        .is_some_and(|capacity| capacity.available == 0)
}

fn volume_status(volume: &ServiceVolume) -> Vec<Fact> {
    match &volume.mount {
        Some(_) if full(volume) => Vec::new(),
        Some(mount) => mount
            .capacity
            .map(|capacity| Fact::new(gettext("Space"), capacity_text(capacity)))
            .into_iter()
            .collect(),
        None => vec![
            Fact::new(gettext("Status"), gettext("Not mounted")),
            Fact::new(gettext("Size"), glimpse_utils::size::bytes(volume.size)),
        ],
    }
}

fn volume_facts(volume: &ServiceVolume) -> Vec<Fact> {
    let filesystem = volume
        .fs
        .as_deref()
        .filter(|fs| !fs.is_empty())
        .map(|fs| Fact::new(gettext("Filesystem"), cap(fs)));
    let mounted_at = volume
        .mount
        .as_ref()
        .map(|mount| Fact::new(gettext("Mounted at"), cap(&mount.at.to_string_lossy())));
    let read_only = volume
        .read_only
        .then(|| Fact::new(gettext("Access"), gettext("Read-only")));
    volume_status(volume)
        .into_iter()
        .chain(filesystem)
        .chain(mounted_at)
        .chain(read_only)
        .collect()
}

fn volume_title(volume: &ServiceVolume, drive: &ServiceDrive) -> String {
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

fn volume_row(volume: &ServiceVolume, drive: &ServiceDrive, icon: &str) -> RemovableVolume {
    let capacity = volume.mount.as_ref().and_then(|mount| mount.capacity);
    RemovableVolume {
        id: volume.id.as_str().to_owned(),
        title: volume_title(volume, drive),
        icon: icon.to_owned(),
        fraction: fraction(capacity),
        busy: drive.busy.is_some() || volume.busy.is_some(),
        mounted: volume.mount.is_some(),
        problem: match full(volume) {
            true => gettext("No space left"),
            false => String::new(),
        },
        facts: volume_facts(volume),
    }
}

fn drive_row(drive: &ServiceDrive) -> RemovableDrive {
    let icon = drive_icon(drive);
    RemovableDrive {
        id: drive.id.as_str().to_owned(),
        title: cap(&drive.name),
        icon: icon.to_owned(),
        ejectable: drive.ejectable,
        busy: drive.busy.is_some(),
        dimmed: !drive.media_available && drive.volumes.is_empty(),
        facts: drive_facts(drive),
        volumes: drive
            .volumes
            .iter()
            .map(|volume| volume_row(volume, drive, icon))
            .collect(),
    }
}

pub struct Devices {
    pub drives: Vec<RemovableDrive>,
    pub more: Option<String>,
}

pub fn devices(state: &RemovableState, cap: usize, expanded: bool) -> Devices {
    let take = shown(cap, expanded);
    let drives = state.drives.iter().take(take).map(drive_row).collect();
    Devices {
        drives,
        more: more(state.drives.len(), cap, expanded),
    }
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
mod tests;
