use std::collections::HashMap;

use zbus::zvariant::Value;

pub const SERVICE: &str = "org.freedesktop.UDisks2";
pub const ROOT: &str = "/org/freedesktop/UDisks2";

pub const BLOCK1: &str = "org.freedesktop.UDisks2.Block";
pub const DRIVE1: &str = "org.freedesktop.UDisks2.Drive";
pub const FILESYSTEM1: &str = "org.freedesktop.UDisks2.Filesystem";
pub const PARTITION1: &str = "org.freedesktop.UDisks2.Partition";
pub const ENCRYPTED1: &str = "org.freedesktop.UDisks2.Encrypted";

#[zbus::proxy(
    interface = "org.freedesktop.UDisks2.Filesystem",
    default_service = "org.freedesktop.UDisks2"
)]
pub trait Filesystem {
    fn mount(&self, options: HashMap<&str, Value<'_>>) -> zbus::Result<String>;
    fn unmount(&self, options: HashMap<&str, Value<'_>>) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.UDisks2.Drive",
    default_service = "org.freedesktop.UDisks2"
)]
pub trait Drive {
    fn eject(&self, options: HashMap<&str, Value<'_>>) -> zbus::Result<()>;
    fn power_off(&self, options: HashMap<&str, Value<'_>>) -> zbus::Result<()>;
}

use super::{Properties, flag, number, text};

const NAME: usize = 64;
const LABEL: usize = 24;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConnectionBus {
    #[default]
    Unknown,
    Ata,
    Usb,
    Ieee1394,
    Scsi,
    Sdio,
}

impl ConnectionBus {
    pub fn parse(value: &str) -> Option<Self> {
        (!value.is_empty()).then_some(match value {
            "ata" => ConnectionBus::Ata,
            "usb" => ConnectionBus::Usb,
            "ieee1394" => ConnectionBus::Ieee1394,
            "scsi" => ConnectionBus::Scsi,
            "sdio" => ConnectionBus::Sdio,
            _ => ConnectionBus::Unknown,
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Media {
    Thumb,
    Flash,
    FlashCf,
    FlashMmc,
    FlashMs,
    FlashSd,
    FlashSdhc,
    FlashSdio,
    FlashSdxc,
    FlashSm,
    Floppy,
    FloppyJaz,
    FloppyZip,
    OpticalCd,
    OpticalCdR,
    OpticalCdRw,
    OpticalDvd,
    OpticalDvdR,
    OpticalDvdRw,
    OpticalDvdRam,
    OpticalDvdPlusR,
    OpticalDvdPlusRDl,
    OpticalDvdPlusRw,
    OpticalDvdPlusRwDl,
    OpticalBd,
    OpticalBdR,
    OpticalBdRe,
    OpticalHddvd,
    OpticalHddvdR,
    OpticalHddvdRw,
    OpticalMo,
    OpticalMrw,
    OpticalMrwW,
    #[default]
    Other,
}

impl Media {
    pub fn parse(value: &str) -> Option<Self> {
        (!value.is_empty()).then_some(match value {
            "thumb" => Media::Thumb,
            "flash" => Media::Flash,
            "flash_cf" => Media::FlashCf,
            "flash_mmc" => Media::FlashMmc,
            "flash_ms" => Media::FlashMs,
            "flash_sd" => Media::FlashSd,
            "flash_sdhc" => Media::FlashSdhc,
            "flash_sdio" => Media::FlashSdio,
            "flash_sdxc" => Media::FlashSdxc,
            "flash_sm" => Media::FlashSm,
            "floppy" => Media::Floppy,
            "floppy_jaz" => Media::FloppyJaz,
            "floppy_zip" => Media::FloppyZip,
            "optical_cd" => Media::OpticalCd,
            "optical_cd_r" => Media::OpticalCdR,
            "optical_cd_rw" => Media::OpticalCdRw,
            "optical_dvd" => Media::OpticalDvd,
            "optical_dvd_r" => Media::OpticalDvdR,
            "optical_dvd_rw" => Media::OpticalDvdRw,
            "optical_dvd_ram" => Media::OpticalDvdRam,
            "optical_dvd_plus_r" => Media::OpticalDvdPlusR,
            "optical_dvd_plus_r_dl" => Media::OpticalDvdPlusRDl,
            "optical_dvd_plus_rw" => Media::OpticalDvdPlusRw,
            "optical_dvd_plus_rw_dl" => Media::OpticalDvdPlusRwDl,
            "optical_bd" => Media::OpticalBd,
            "optical_bd_r" => Media::OpticalBdR,
            "optical_bd_re" => Media::OpticalBdRe,
            "optical_hddvd" => Media::OpticalHddvd,
            "optical_hddvd_r" => Media::OpticalHddvdR,
            "optical_hddvd_rw" => Media::OpticalHddvdRw,
            "optical_mo" => Media::OpticalMo,
            "optical_mrw" => Media::OpticalMrw,
            "optical_mrw_w" => Media::OpticalMrwW,
            _ => Media::Other,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DriveProperties {
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub revision: Option<String>,
    pub size: Option<u64>,
    pub media: Option<Media>,
    pub media_available: Option<bool>,
    pub media_removable: Option<bool>,
    pub removable: Option<bool>,
    pub ejectable: Option<bool>,
    pub can_power_off: Option<bool>,
    pub connection_bus: Option<ConnectionBus>,
    pub optical: Option<bool>,
    pub sort_key: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockProperties {
    pub device: Option<String>,
    pub preferred_device: Option<String>,
    pub drive: Option<String>,
    pub crypto_backing_device: Option<String>,
    pub id_label: Option<String>,
    pub id_type: Option<String>,
    pub id_uuid: Option<String>,
    pub id_usage: Option<String>,
    pub size: Option<u64>,
    pub read_only: Option<bool>,
    pub hint_auto: Option<bool>,
    pub hint_ignore: Option<bool>,
    pub hint_system: Option<bool>,
    pub hint_name: Option<String>,
    pub hint_icon_name: Option<String>,
    pub hint_symbolic_icon_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilesystemProperties {
    pub mount_points: Option<Vec<String>>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartitionProperties {
    pub number: Option<u32>,
    pub name: Option<String>,
    pub table: Option<String>,
    pub type_: Option<String>,
}

fn path(properties: &Properties, key: &str) -> Option<String> {
    match &**properties.get(key)? {
        Value::ObjectPath(value) if value.as_str() != "/" => Some(value.as_str().to_owned()),
        _ => None,
    }
}

fn bytes(properties: &Properties, key: &str) -> Option<String> {
    let mut raw = Vec::<u8>::try_from(properties.get(key)?.clone()).ok()?;
    if raw.last() == Some(&0) {
        raw.pop();
    }
    (!raw.is_empty()).then(|| String::from_utf8_lossy(&raw).into_owned())
}

fn paths(properties: &Properties, key: &str) -> Option<Vec<String>> {
    let raw = Vec::<Vec<u8>>::try_from(properties.get(key)?.clone()).ok()?;
    let values: Vec<String> = raw
        .into_iter()
        .filter_map(|mut entry| {
            if entry.last() == Some(&0) {
                entry.pop();
            }
            (!entry.is_empty()).then(|| String::from_utf8_lossy(&entry).into_owned())
        })
        .collect();
    (!values.is_empty()).then_some(values)
}

fn size(properties: &Properties, key: &str) -> Option<u64> {
    u64::try_from(properties.get(key)?).ok()
}

pub fn decode_drive(properties: &Properties) -> DriveProperties {
    DriveProperties {
        vendor: text(properties, "Vendor", NAME),
        model: text(properties, "Model", NAME),
        serial: text(properties, "Serial", NAME),
        revision: text(properties, "Revision", NAME),
        size: size(properties, "Size"),
        media: properties
            .get("Media")
            .and_then(|value| <&str>::try_from(value).ok())
            .and_then(Media::parse),
        media_available: flag(properties, "MediaAvailable"),
        media_removable: flag(properties, "MediaRemovable"),
        removable: flag(properties, "Removable"),
        ejectable: flag(properties, "Ejectable"),
        can_power_off: flag(properties, "CanPowerOff"),
        connection_bus: properties
            .get("ConnectionBus")
            .and_then(|value| <&str>::try_from(value).ok())
            .and_then(ConnectionBus::parse),
        optical: flag(properties, "Optical"),
        sort_key: text(properties, "SortKey", NAME),
    }
}

pub fn decode_block(properties: &Properties) -> BlockProperties {
    BlockProperties {
        device: bytes(properties, "Device"),
        preferred_device: bytes(properties, "PreferredDevice"),
        drive: path(properties, "Drive"),
        crypto_backing_device: path(properties, "CryptoBackingDevice"),
        id_label: text(properties, "IdLabel", LABEL),
        id_type: text(properties, "IdType", NAME),
        id_uuid: text(properties, "IdUUID", NAME),
        id_usage: text(properties, "IdUsage", NAME),
        size: size(properties, "Size"),
        read_only: flag(properties, "ReadOnly"),
        hint_auto: flag(properties, "HintAuto"),
        hint_ignore: flag(properties, "HintIgnore"),
        hint_system: flag(properties, "HintSystem"),
        hint_name: text(properties, "HintName", NAME),
        hint_icon_name: text(properties, "HintIconName", NAME),
        hint_symbolic_icon_name: text(properties, "HintSymbolicIconName", NAME),
    }
}

pub fn decode_filesystem(properties: &Properties) -> FilesystemProperties {
    FilesystemProperties {
        mount_points: paths(properties, "MountPoints"),
        size: size(properties, "Size"),
    }
}

pub fn decode_partition(properties: &Properties) -> PartitionProperties {
    PartitionProperties {
        number: number(properties, "Number"),
        name: text(properties, "Name", NAME),
        table: path(properties, "Table"),
        type_: text(properties, "Type", NAME),
    }
}

#[cfg(test)]
mod tests;
