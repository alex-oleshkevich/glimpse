use std::path::PathBuf;

use gettextrs::{gettext, ngettext};
use glimpse_services::{
    Drive as ServiceDrive, RemovableCapacity, RemovableFailure, RemovableState,
    Volume as ServiceVolume, VolumeId,
};
use glimpse_widgets::{RemovableDrive, RemovableSpace, RemovableVolume};

pub const ICON: &str = "drive-removable-media-symbolic";
const NAME_CAP: usize = 60;

fn shown(drive: &ServiceDrive) -> bool {
    drive.media_available || !drive.volumes.is_empty()
}

fn visible(removable: &RemovableState) -> impl Iterator<Item = &ServiceDrive> {
    removable.drives.iter().filter(|drive| shown(drive))
}

pub fn chip(removable: &RemovableState) -> bool {
    visible(removable).next().is_some()
}

pub fn tooltip(removable: &RemovableState, format: Option<&str>) -> String {
    let Some(format) = format else {
        return gettext("Removable");
    };
    let drives = visible(removable).count().to_string();
    let volumes = visible(removable)
        .map(|drive| drive.volumes.len())
        .sum::<usize>()
        .to_string();
    crate::applets::tokens::render(format, |token| match token {
        "drives" => Some(drives.as_str()),
        "volumes" => Some(volumes.as_str()),
        _ => None,
    })
}

fn more(total: usize, cap: usize) -> Option<String> {
    let hidden = total.saturating_sub(cap);
    if hidden == 0 {
        return None;
    }
    Some(
        ngettext("{count} more", "{count} more", hidden as u32)
            .replace("{count}", &hidden.to_string()),
    )
}

pub enum Activation {
    Open(PathBuf),
    Mount(VolumeId),
}

pub fn activation(state: &RemovableState, id: &str) -> Option<Activation> {
    let volume = state
        .drives
        .iter()
        .flat_map(|drive| &drive.volumes)
        .find(|volume| volume.id.as_str() == id)?;
    Some(match &volume.mount {
        Some(mount) => Activation::Open(mount.at.clone()),
        None => Activation::Mount(volume.id.clone()),
    })
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

fn free_text(capacity: RemovableCapacity) -> String {
    gettext("{available} free").replace(
        "{available}",
        &glimpse_utils::size::bytes(capacity.available),
    )
}

fn full(volume: &ServiceVolume) -> bool {
    volume
        .mount
        .as_ref()
        .and_then(|mount| mount.capacity)
        .is_some_and(|capacity| capacity.available == 0)
}

fn fraction(capacity: Option<RemovableCapacity>) -> Option<f64> {
    let capacity = capacity?;
    if capacity.total == 0 {
        return None;
    }
    Some(1.0 - (capacity.available as f64 / capacity.total as f64))
}

pub fn main_volume(drive: &ServiceDrive) -> Option<&ServiceVolume> {
    drive.volumes.iter().reduce(|best, volume| {
        if volume.size > best.size {
            volume
        } else {
            best
        }
    })
}

fn space(drive: &ServiceDrive, main: &ServiceVolume) -> Option<RemovableSpace> {
    let capacity = main.mount.as_ref()?.capacity?;
    let title = match glimpse_utils::clean(&main.label, NAME_CAP) {
        label if drive.volumes.len() > 1 && !label.is_empty() => label,
        _ => gettext("Space"),
    };
    Some(RemovableSpace {
        title,
        fraction: fraction(Some(capacity))?,
        text: free_text(capacity),
    })
}

fn extra(volume: &ServiceVolume) -> RemovableVolume {
    let size = glimpse_utils::size::bytes(volume.size);
    let label = glimpse_utils::clean(&volume.label, NAME_CAP);
    let title = match (
        label.is_empty(),
        volume.fs.as_deref().filter(|fs| !fs.is_empty()),
    ) {
        (false, _) => label,
        (true, Some(fs)) => glimpse_utils::clean(fs, NAME_CAP),
        (true, None) => size.clone(),
    };
    RemovableVolume {
        id: volume.id.as_str().to_owned(),
        title,
        size,
        mounted: volume.mount.is_some(),
    }
}

fn problem(drive: &ServiceDrive, main: Option<&ServiceVolume>) -> String {
    if drive.failure == Some(RemovableFailure::Busy) {
        return gettext("In use by an app");
    }
    match main.is_some_and(full) {
        true => gettext("No space left"),
        false => String::new(),
    }
}

fn drive_row(drive: &ServiceDrive) -> RemovableDrive {
    let main = main_volume(drive);
    RemovableDrive {
        id: drive.id.as_str().to_owned(),
        title: glimpse_utils::clean(&drive.name, NAME_CAP),
        icon: drive_icon(drive).to_owned(),
        busy: drive.busy.is_some() || drive.volumes.iter().any(|volume| volume.busy.is_some()),
        mounted: main.is_some_and(|main| main.mount.is_some()),
        problem: problem(drive, main),
        space: main.and_then(|main| space(drive, main)),
        volumes: drive
            .volumes
            .iter()
            .filter(|volume| Some(volume.id.as_str()) != main.map(|main| main.id.as_str()))
            .map(extra)
            .collect(),
        open: main.map(|main| main.id.as_str().to_owned()),
        removable: drive.ejectable
            || drive.can_power_off
            || drive.volumes.iter().any(|volume| volume.mount.is_some()),
    }
}

pub struct Devices {
    pub drives: Vec<RemovableDrive>,
    pub rest: Vec<RemovableDrive>,
    pub more: Option<String>,
}

pub fn devices(state: &RemovableState, cap: usize) -> Devices {
    let mut drives: Vec<RemovableDrive> = visible(state).map(drive_row).collect();
    let total = drives.len();
    let rest = drives.split_off(cap.min(total));
    Devices {
        drives,
        rest,
        more: more(total, cap),
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
