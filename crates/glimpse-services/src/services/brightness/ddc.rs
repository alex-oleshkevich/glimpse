use std::path::Path;

use ddc::{Ddc, FeatureCode};
use futures_util::future::BoxFuture;
use tokio::task::spawn_blocking;

use super::{Backlight, Entry, Kind, ReadOutcome};

const DRM_ROOT: &str = "/sys/class/drm";
const BRIGHTNESS_FEATURE: FeatureCode = 0x10;
const ID_PREFIX: &str = "ddc:";

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
            match spawn_blocking(move || read_blocking(bus)).await {
                Ok(Some(entry)) => ReadOutcome::Found(entry),
                Ok(None) => ReadOutcome::Gone,
                Err(_) => ReadOutcome::Unavailable,
            }
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
    if !name.starts_with("card") || !name.contains('-') {
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

fn read_blocking(bus: u32) -> Option<Entry> {
    probe(bus)
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
mod tests {
    use super::*;

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
        let entries = enumerate_blocking();

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
}
