use std::fs;
use std::path::{Path, PathBuf};

use super::GpuSample;

/// Reads AMD GPU stats from the kernel's amdgpu sysfs interface
/// (`/sys/class/drm/card*/device/`). All values are best-effort — files
/// can be absent on integrated GPUs or older drivers, in which case the
/// corresponding `Option<_>` in `GpuSample` stays `None` and the renderer
/// hides the token rather than emitting a misleading zero.
#[derive(Debug, Default)]
pub struct AmdgpuSampler {
    /// `/sys/class/drm/cardN/device/` — set once at construction. `None`
    /// when no AMD card is present; the sampler then becomes a no-op.
    card_dir: Option<PathBuf>,
    /// Resolved sibling `hwmon/hwmonN/` — also set once at construction.
    /// Some files (temp, power, freq) live here rather than at `card_dir`.
    hwmon_dir: Option<PathBuf>,
    name: Option<String>,
}

impl AmdgpuSampler {
    pub fn new() -> Self {
        let card_dir = detect_first_amd_card(Path::new("/sys/class/drm"));
        let hwmon_dir = card_dir.as_ref().and_then(|d| first_hwmon_in(d.join("hwmon")));
        let name = card_dir.as_ref().and_then(|d| {
            fs::read_to_string(d.join("product_name"))
                .ok()
                .map(|s| s.trim().to_owned())
        });
        Self {
            card_dir,
            hwmon_dir,
            name,
        }
    }

    pub fn is_available(&self) -> bool {
        self.card_dir.is_some()
    }

    pub fn sample(&self) -> Option<GpuSample> {
        let card = self.card_dir.as_ref()?;
        let busy = read_pct(card.join("gpu_busy_percent"));
        let vram_used = read_u64(card.join("mem_info_vram_used"));
        let vram_total = read_u64(card.join("mem_info_vram_total"));
        let mem_util = match (vram_used, vram_total) {
            (Some(u), Some(t)) if t > 0 => Some(u as f64 / t as f64),
            _ => None,
        };
        let (temp, power, freq) = match self.hwmon_dir.as_ref() {
            Some(h) => (
                read_milli_to_unit(h.join("temp1_input"), 1000.0),
                read_milli_to_unit(h.join("power1_average"), 1_000_000.0),
                read_milli_to_unit(h.join("freq1_input"), 1_000_000.0),
            ),
            None => (None, None, None),
        };
        Some(GpuSample {
            name: self.name.clone(),
            util: busy,
            temp_c: temp,
            mem_used_bytes: vram_used,
            mem_total_bytes: vram_total,
            mem_util,
            freq_mhz: freq,
            power_w: power,
        })
    }
}

fn detect_first_amd_card(drm_root: &Path) -> Option<PathBuf> {
    // We scan in cardN order so the panel's "GPU" indicator on a laptop
    // with an iGPU + dGPU prefers card0 deterministically. Multi-card hosts
    // wanting a specific one will get a `card = N` config knob later.
    let mut cards: Vec<PathBuf> = fs::read_dir(drm_root)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|f| f.to_str())
                .map(|s| s.starts_with("card") && !s.contains('-'))
                .unwrap_or(false)
        })
        .collect();
    cards.sort();
    cards.into_iter().find_map(|card| {
        let device = card.join("device");
        let vendor = fs::read_to_string(device.join("vendor")).ok()?;
        if vendor.trim() == "0x1002" {
            Some(device)
        } else {
            None
        }
    })
}

fn first_hwmon_in(hwmon_root: PathBuf) -> Option<PathBuf> {
    fs::read_dir(&hwmon_root)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.is_dir())
}

fn read_pct<P: AsRef<Path>>(path: P) -> Option<f64> {
    let raw: u64 = fs::read_to_string(path).ok()?.trim().parse().ok()?;
    Some((raw as f64) / 100.0)
}

fn read_u64<P: AsRef<Path>>(path: P) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Reads an integer-encoded value in micro-units and divides by `unit`
/// to get the SI form. amdgpu exposes temperature in millidegrees,
/// power in microwatts, frequency in Hz — different scale factors, same
/// integer-text-on-disk pattern.
fn read_milli_to_unit<P: AsRef<Path>>(path: P, unit: f64) -> Option<f64> {
    let raw: i64 = fs::read_to_string(path).ok()?.trim().parse().ok()?;
    Some(raw as f64 / unit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// AMD vendor ID resolution — must skip Intel/Nvidia cards in a
    /// synthetic DRM tree and pick the AMD one even when it's `card1`.
    #[test]
    fn detect_first_amd_card_skips_non_amd_vendors() {
        let root = tempdir();
        // card0: Intel
        make_card(&root, "card0", "0x8086");
        // card1: AMD
        make_card(&root, "card1", "0x1002");
        // card2: AMD too — picker must take card1 (sort order).
        make_card(&root, "card2", "0x1002");

        let device = detect_first_amd_card(&root).expect("an AMD card");
        assert!(device.ends_with("card1/device"));
    }

    /// No AMD card → None, not a panic. Caller treats sampler as no-op.
    #[test]
    fn detect_returns_none_when_no_amd_card_present() {
        let root = tempdir();
        make_card(&root, "card0", "0x8086");
        assert!(detect_first_amd_card(&root).is_none());
    }

    /// Helper paths with `-` (e.g. `card1-DP-1`) are sub-connectors,
    /// not GPU cards. They must be filtered out so the picker isn't
    /// confused by a monitor connector that has no `vendor` file.
    #[test]
    fn detect_ignores_connector_subdirs() {
        let root = tempdir();
        fs::create_dir_all(root.join("card0-DP-1")).unwrap();
        make_card(&root, "card1", "0x1002");
        let device = detect_first_amd_card(&root).expect("AMD card found");
        assert!(device.ends_with("card1/device"));
    }

    fn tempdir() -> PathBuf {
        let n: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        let path = std::env::temp_dir().join(format!("sysmonitor-amdgpu-{n}-{}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn make_card(root: &Path, name: &str, vendor: &str) {
        let device = root.join(name).join("device");
        fs::create_dir_all(&device).unwrap();
        fs::write(device.join("vendor"), format!("{vendor}\n")).unwrap();
    }
}
