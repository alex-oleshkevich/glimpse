use std::fs;
use std::path::{Path, PathBuf};

/// Generic temperature reading. Distinct from CPU/GPU package temps — the
/// `temp` indicator kind lets users name any hwmon sensor (NVMe controller,
/// VRM, ambient, etc.) by `<hwmon_name>` or `<hwmon_name>/<label>`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TempSample {
    pub temp_c: f64,
    pub sensor_label: Option<String>,
}

#[derive(Debug, Default)]
pub struct TempSampler;

impl TempSampler {
    pub fn new() -> Self {
        Self
    }

    /// Resolves a sensor spec to a temperature reading.
    ///
    /// Spec forms:
    /// * `"<hwmon_name>"` — first `temp*_input` in the hwmon dir whose
    ///   `name` matches.
    /// * `"<hwmon_name>/<label>"` — the `temp*_input` whose sibling
    ///   `temp*_label` matches `<label>` exactly. Use this for
    ///   multi-sensor drivers like Intel coretemp where you want
    ///   "Package id 0" vs. "Core 0".
    ///
    /// Returns `None` when the named hwmon is absent (driver not loaded)
    /// or the named label is missing. The applet then omits the
    /// indicator rather than producing a misleading 0°C.
    pub fn sample(&self, spec: &str) -> Option<TempSample> {
        let (name, label) = match spec.split_once('/') {
            Some((n, l)) => (n.trim(), Some(l.trim())),
            None => (spec.trim(), None),
        };
        let dir = find_hwmon_dir(name)?;
        let (path, resolved_label) = match label {
            Some(l) => find_temp_input_by_label(&dir, l)?,
            None => find_first_temp_input(&dir)?,
        };
        let milli: i64 = fs::read_to_string(&path).ok()?.trim().parse().ok()?;
        Some(TempSample {
            temp_c: milli as f64 / 1000.0,
            sensor_label: resolved_label,
        })
    }
}

/// Every hwmon temperature input on the host, encoded as the same
/// `"<name>"` / `"<name>/<label>"` spec form the per-sample resolver
/// accepts. Used to seed the popover's temperature grid when the user
/// hasn't configured `kind = "temp"` indicators.
pub fn discover_sensors() -> Vec<String> {
    let dir = match fs::read_dir("/sys/class/hwmon") {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for entry in dir.flatten() {
        let path = entry.path();
        let name = match fs::read_to_string(path.join("name")) {
            Ok(s) => s.trim().to_owned(),
            Err(_) => continue,
        };
        let mut found_any_label = false;
        for n in 1..=64u32 {
            let input = path.join(format!("temp{n}_input"));
            if !input.exists() {
                continue;
            }
            let label_path = path.join(format!("temp{n}_label"));
            if let Ok(label) = fs::read_to_string(&label_path) {
                let trimmed = label.trim();
                if !trimmed.is_empty() {
                    out.push(format!("{name}/{trimmed}"));
                    found_any_label = true;
                }
            }
        }
        // hwmon nodes without labelled sub-sensors (e.g. nvme single-temp)
        // still want a name-only spec so they appear in the popover.
        if !found_any_label
            && (1..=64u32).any(|n| path.join(format!("temp{n}_input")).exists())
        {
            out.push(name);
        }
    }
    out.sort();
    out
}

fn find_hwmon_dir(name: &str) -> Option<PathBuf> {
    for entry in fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        let path = entry.path();
        if let Ok(actual) = fs::read_to_string(path.join("name"))
            && actual.trim() == name
        {
            return Some(path);
        }
    }
    None
}

fn find_temp_input_by_label(dir: &Path, target: &str) -> Option<(PathBuf, Option<String>)> {
    for n in 1..=64u32 {
        let label_path = dir.join(format!("temp{n}_label"));
        if let Ok(label) = fs::read_to_string(&label_path)
            && label.trim() == target
        {
            return Some((dir.join(format!("temp{n}_input")), Some(target.to_owned())));
        }
    }
    None
}

fn find_first_temp_input(dir: &Path) -> Option<(PathBuf, Option<String>)> {
    for n in 1..=64u32 {
        let input = dir.join(format!("temp{n}_input"));
        if input.exists() {
            let label = fs::read_to_string(dir.join(format!("temp{n}_label")))
                .ok()
                .map(|s| s.trim().to_owned());
            return Some((input, label));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Synthetic hwmon dir with two sensors. `find_temp_input_by_label`
    /// must pick the input file matched to the requested label, not the
    /// first one it scans.
    #[test]
    fn label_lookup_matches_label_not_position() {
        let dir = tempdir();
        write(&dir, "temp1_label", "Core 0\n");
        write(&dir, "temp1_input", "42000\n");
        write(&dir, "temp2_label", "Package id 0\n");
        write(&dir, "temp2_input", "55000\n");

        let (input, label) =
            find_temp_input_by_label(&dir, "Package id 0").expect("label resolves");
        let raw: String = fs::read_to_string(&input).unwrap();
        assert_eq!(raw.trim(), "55000");
        assert_eq!(label.as_deref(), Some("Package id 0"));
    }

    /// Missing label → None, not the first sensor as a fallback. A user
    /// who typed the label wrong should see the indicator disappear (loud
    /// fail) rather than silently report a different sensor's temp.
    #[test]
    fn missing_label_returns_none() {
        let dir = tempdir();
        write(&dir, "temp1_label", "Core 0\n");
        write(&dir, "temp1_input", "42000\n");
        assert!(find_temp_input_by_label(&dir, "Nonexistent").is_none());
    }

    /// `find_first_temp_input` skips gaps. Some hwmon drivers expose
    /// temp1_*, temp3_*, temp4_* but not temp2_*. The iterator just keeps
    /// scanning the numbered probes.
    #[test]
    fn first_temp_input_skips_gaps() {
        let dir = tempdir();
        // temp1 deliberately missing — first existing is temp3.
        write(&dir, "temp3_input", "30000\n");
        write(&dir, "temp3_label", "Ambient\n");
        let (input, label) = find_first_temp_input(&dir).expect("a sensor exists");
        assert!(input.ends_with("temp3_input"));
        assert_eq!(label.as_deref(), Some("Ambient"));
    }

    fn tempdir() -> PathBuf {
        let n: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        let path = std::env::temp_dir().join(format!("sysmonitor-temp-{n}-{}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write(dir: &Path, name: &str, contents: &str) {
        fs::write(dir.join(name), contents).unwrap();
    }
}
