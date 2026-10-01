use std::path::Path;

use ddc::{Ddc, FeatureCode};
use futures_util::future::BoxFuture;
use tokio::task::spawn_blocking;

use super::{Backlight, Entry, Kind, ReadOutcome};

const DRM_ROOT: &str = "/sys/class/drm";
const BRIGHTNESS_FEATURE: FeatureCode = 0x10;
const ID_PREFIX: &str = "ddc:";
const BUILT_IN_CONNECTORS: [&str; 3] = ["eDP-", "LVDS-", "DSI-"];

/// External-monitor brightness over DDC/CI, talked natively through `/dev/i2c-*` rather than
/// through the `ddcutil` binary — glimpse only depends on that package for the udev rule it
/// installs, never as a subprocess. The I2C bus for a connector comes from
/// `/sys/class/drm/cardN-<connector>/ddc`, a structural symlink the kernel already maintains, so
/// enumeration never has to probe a bus to find out what is on it. Every DDC/CI operation blocks
/// on real ioctls with protocol-mandated delays, so all of it runs on `spawn_blocking`.
#[derive(Debug, Clone, Copy, Default)]
pub struct DdcBacklight;

impl DdcBacklight {
    pub fn new() -> Self {
        Self
    }
}

impl Backlight for DdcBacklight {
    fn enumerate(&self) -> BoxFuture<'_, Vec<Entry>> {
        Box::pin(async move { spawn_blocking(enumerate_blocking).await.unwrap_or_default() })
    }

    fn read(&self, id: String) -> BoxFuture<'_, ReadOutcome> {
        Box::pin(async move {
            let Some(bus) = bus_of(&id) else {
                return ReadOutcome::Gone;
            };
            spawn_blocking(move || read_blocking(bus))
                .await
                .unwrap_or(ReadOutcome::Unavailable)
        })
    }

    fn write(&self, id: String, value: u32) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            let bus = bus_of(&id).ok_or_else(|| format!("not a DDC/CI source: {id}"))?;
            spawn_blocking(move || write_blocking(bus, value))
                .await
                .unwrap_or_else(|error| Err(error.to_string()))
        })
    }
}

fn bus_of(id: &str) -> Option<u32> {
    id.strip_prefix(ID_PREFIX)?.parse().ok()
}

fn enumerate_blocking() -> Vec<Entry> {
    let Ok(dir) = std::fs::read_dir(DRM_ROOT) else {
        return Vec::new();
    };
    dir.flatten()
        .filter_map(|item| connector_entry(&item.path(), &item.file_name().to_string_lossy()))
        .collect()
}

fn connector_entry(card_dir: &Path, name: &str) -> Option<Entry> {
    if !name.starts_with("card") || !name.contains('-') || built_in(name) {
        return None;
    }
    let status = std::fs::read_to_string(card_dir.join("status")).ok()?;
    if status.trim() != "connected" {
        return None;
    }
    let mut entry = candidate_buses(card_dir).find_map(probe)?;
    entry.device_link = Some(format!("../../{name}"));
    Some(entry)
}

fn built_in(name: &str) -> bool {
    name.split_once('-').is_some_and(|(_, connector)| {
        BUILT_IN_CONNECTORS
            .iter()
            .any(|prefix| connector.starts_with(prefix))
    })
}

/// Which I2C bus actually carries DDC/CI for this connector is not settled by one symlink. A
/// DisplayPort connector's `ddc` symlink names its legacy, pin-based hardware bus — but on this
/// hardware a real DisplayPort link bridges DDC/CI over the AUX channel instead, exposed as a
/// second, connector-owned `i2c-N` child directory whose device name contains "aux"; the legacy
/// bus answers nothing at all there. Older connector types with no AUX channel (VGA, DVI, plain
/// HDMI) have no such child and only ever had the one bus. Rather than commit to either as *the*
/// answer, both structurally-plausible candidates are tried, aux first since it is the one
/// measured to work, and `probe` decides by whether the bus actually answers DDC/CI.
fn candidate_buses(card_dir: &Path) -> impl Iterator<Item = u32> {
    aux_bus_number(card_dir)
        .into_iter()
        .chain(ddc_symlink_bus_number(card_dir))
}

fn aux_bus_number(card_dir: &Path) -> Option<u32> {
    std::fs::read_dir(card_dir).ok()?.find_map(|item| {
        let item = item.ok()?;
        let bus = item
            .file_name()
            .to_str()?
            .strip_prefix("i2c-")?
            .parse()
            .ok()?;
        let device_name = std::fs::read_to_string(item.path().join("name")).ok()?;
        device_name.to_lowercase().contains("aux").then_some(bus)
    })
}

fn ddc_symlink_bus_number(card_dir: &Path) -> Option<u32> {
    let resolved = std::fs::canonicalize(card_dir.join("ddc")).ok()?;
    resolved
        .file_name()?
        .to_str()?
        .strip_prefix("i2c-")?
        .parse()
        .ok()
}

fn read_blocking(bus: u32) -> ReadOutcome {
    match probe(bus) {
        Some(entry) => ReadOutcome::Found(entry),
        None if bus_is_connected(Path::new(DRM_ROOT), bus) => ReadOutcome::Unavailable,
        None => ReadOutcome::Gone,
    }
}

fn bus_is_connected(root: &Path, bus: u32) -> bool {
    let Ok(dir) = std::fs::read_dir(root) else {
        return false;
    };
    dir.flatten().any(|item| {
        let name = item.file_name().to_string_lossy().into_owned();
        let path = item.path();
        name.starts_with("card")
            && name.contains('-')
            && !built_in(&name)
            && std::fs::read_to_string(path.join("status")).is_ok_and(|s| s.trim() == "connected")
            && candidate_buses(&path).any(|candidate| candidate == bus)
    })
}

fn probe(bus: u32) -> Option<Entry> {
    let mut handle = ddc_i2c::from_i2c_device(format!("/dev/i2c-{bus}")).ok()?;
    let value = handle.get_vcp_feature(BRIGHTNESS_FEATURE).ok()?;
    let max_brightness = u32::from(value.maximum());
    if max_brightness == 0 {
        return None;
    }
    Some(Entry {
        id: format!("{ID_PREFIX}{bus}"),
        device_link: None,
        type_name: "ddc".to_owned(),
        brightness: u32::from(value.value()),
        max_brightness,
        kind: Kind::Display,
    })
}

fn write_blocking(bus: u32, value: u32) -> Result<(), String> {
    let mut handle =
        ddc_i2c::from_i2c_device(format!("/dev/i2c-{bus}")).map_err(|error| error.to_string())?;
    let clamped = value.min(u32::from(u16::MAX)) as u16;
    handle
        .set_vcp_feature(BRIGHTNESS_FEATURE, clamped)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
