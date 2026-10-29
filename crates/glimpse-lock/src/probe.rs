use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use rustix::fs::StatVfsMountFlags;

const CHKPWD_DIRS: [&str; 4] = ["/run/wrappers/bin", "/usr/bin", "/usr/sbin", "/sbin"];
const PAM_DIRS: [&str; 2] = ["/etc/pam.d", "/usr/lib/pam.d"];
const SHADOW: &str = "/etc/shadow";
const DAC_CAPS: u64 = (1 << 1) | (1 << 2);
const SECBIT_NOROOT: i32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub outcome: Result<Option<String>, String>,
}

impl Check {
    fn new(name: impl Into<String>, outcome: Result<Option<String>, String>) -> Self {
        Self {
            name: name.into(),
            outcome,
        }
    }

    pub fn passed(&self) -> bool {
        self.outcome.is_ok()
    }

    pub fn line(&self) -> String {
        match &self.outcome {
            Ok(None) => format!("{}: ok", self.name),
            Ok(Some(note)) => format!("{}: ok ({note})", self.name),
            Err(reason) => format!("{}: fail ({reason})", self.name),
        }
    }
}

pub fn uid_map(text: &str) -> Result<(), String> {
    let lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.split_whitespace()
                .map(str::parse::<u64>)
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "uid_map is not numeric".to_owned())?;
    match lines.as_slice() {
        [line] if line.as_slice() == [0, 0, 4_294_967_295] => Ok(()),
        _ => Err(format!(
            "a user namespace: {}",
            glimpse_utils::clean(text, 80)
        )),
    }
}

pub fn no_new_privs(status: &str) -> Result<(), String> {
    let value = status
        .lines()
        .find_map(|line| line.strip_prefix("NoNewPrivs:"))
        .map(str::trim)
        .ok_or_else(|| "no NoNewPrivs field".to_owned())?;
    match value {
        "0" => Ok(()),
        "1" => Err("NoNewPrivs is 1, so setuid is refused".to_owned()),
        other => Err(format!(
            "NoNewPrivs reads {:?}",
            glimpse_utils::clean(other, 16)
        )),
    }
}

pub fn shadow_needs_caps(mode: Option<u32>) -> bool {
    mode.is_some_and(|mode| mode & 0o400 == 0)
}

pub fn bounding_set(status: &str) -> Result<(), String> {
    let value = status
        .lines()
        .find_map(|line| line.strip_prefix("CapBnd:"))
        .map(str::trim)
        .ok_or_else(|| "no CapBnd field".to_owned())?;
    let set = u64::from_str_radix(value, 16)
        .map_err(|_| format!("CapBnd reads {:?}", glimpse_utils::clean(value, 32)))?;
    match set & DAC_CAPS {
        0 => Err(
            "CapBnd lacks CAP_DAC_OVERRIDE and CAP_DAC_READ_SEARCH, so unix_chkpwd can't read /etc/shadow"
                .to_owned(),
        ),
        _ => Ok(()),
    }
}

pub fn securebits(bits: i32) -> Result<(), String> {
    if bits < 0 {
        Err("securebits are unreadable".to_owned())
    } else if bits & SECBIT_NOROOT != 0 {
        Err("securebits set NOROOT, so setuid root gives unix_chkpwd no capabilities".to_owned())
    } else {
        Ok(())
    }
}

pub fn chkpwd(owner: u32, mode: u32, nosuid: bool) -> Result<(), String> {
    if owner != 0 {
        Err(format!("owned by uid {owner}, not root"))
    } else if mode & 0o6000 == 0 {
        Err("neither setuid nor setgid".to_owned())
    } else if nosuid {
        Err("its filesystem is mounted nosuid".to_owned())
    } else {
        Ok(())
    }
}

pub fn pam_file(text: Option<&str>) -> Result<(), String> {
    let text = text.ok_or_else(|| "missing, so PAM falls through to `other`".to_owned())?;
    let permits = text.lines().any(|line| {
        line.split('#')
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .any(|word| word == "pam_permit.so" || word.ends_with("/pam_permit.so"))
    });
    if permits {
        Err("carries pam_permit.so, which accepts any password".to_owned())
    } else {
        Ok(())
    }
}

pub fn pam_includes(text: &str) -> Vec<String> {
    text.replace("\\\n", " ")
        .lines()
        .filter_map(|line| {
            let mut words = line
                .split('#')
                .next()
                .unwrap_or_default()
                .split_whitespace();
            let first = words.next()?;
            if first == "@include" {
                return words.next().map(str::to_owned);
            }
            let control = words.next()?;
            if control.starts_with('[') {
                return None;
            }
            matches!(control, "include" | "substack")
                .then(|| words.next().map(str::to_owned))
                .flatten()
        })
        .collect()
}

pub fn pam_path(service: &str, dirs: &[&str], exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    dirs.iter()
        .map(|dir| Path::new(dir).join(service))
        .find(|path| exists(path))
}

fn resolve(target: &str, dirs: &[&str], exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    match Path::new(target).is_absolute() {
        true => exists(Path::new(target)).then(|| PathBuf::from(target)),
        false => pam_path(target, dirs, exists),
    }
}

const INCLUDE_FILES_MAX: usize = 32;

fn pam_includes_resolve(
    text: &str,
    dirs: &[&str],
    exists: impl Fn(&Path) -> bool,
    read: impl Fn(&Path) -> Option<String>,
) -> Result<(), String> {
    let mut pending = pam_includes(text);
    let mut seen = std::collections::HashSet::new();
    while let Some(target) = pending.pop() {
        let Some(path) = resolve(&target, dirs, &exists) else {
            return Err(format!(
                "includes {:?}, which is in none of {}",
                glimpse_utils::clean(&target, 64),
                dirs.join(", ")
            ));
        };
        if seen.len() < INCLUDE_FILES_MAX
            && seen.insert(path.clone())
            && let Some(included) = read(&path)
        {
            pending.extend(pam_includes(&included));
        }
    }
    Ok(())
}

pub fn pam_stack(service: &str) -> Check {
    pam_stack_in(service, &PAM_DIRS, Path::exists, |path| {
        std::fs::read_to_string(path).ok()
    })
}

fn pam_stack_in(
    service: &str,
    dirs: &[&str],
    exists: impl Fn(&Path) -> bool,
    read: impl Fn(&Path) -> Option<String>,
) -> Check {
    let name = format!("pam.d/{service}");
    let path = pam_path(service, dirs, &exists);
    let text = path.as_deref().and_then(&read);
    let outcome = pam_file(text.as_deref())
        .and_then(|()| {
            pam_includes_resolve(text.as_deref().unwrap_or_default(), dirs, &exists, &read)
        })
        .map(|()| path.map(|path| path.display().to_string()));
    Check::new(name, outcome)
}

pub fn startup() -> Vec<Check> {
    let mut checks = own();
    checks.push(pam_stack(crate::auth::SERVICE));
    checks
}

pub fn process(root: &Path, prefix: &str) -> Vec<Check> {
    let status = read(&root.join("status"));
    let mut checks = vec![
        Check::new(
            format!("{prefix}uid_map"),
            read(&root.join("uid_map")).and_then(|text| uid_map(&text).map(|()| None)),
        ),
        Check::new(
            format!("{prefix}NoNewPrivs"),
            status
                .clone()
                .and_then(|text| no_new_privs(&text).map(|()| None)),
        ),
    ];
    if shadow_needs_caps(shadow_mode()) {
        checks.push(Check::new(
            format!("{prefix}CapBnd"),
            status.and_then(|text| bounding_set(&text).map(|()| None)),
        ));
    }
    checks
}

pub fn own() -> Vec<Check> {
    let mut checks = process(Path::new("/proc/self"), "");
    if shadow_needs_caps(shadow_mode()) {
        let bits = unsafe { libc::prctl(libc::PR_GET_SECUREBITS) };
        checks.push(Check::new("securebits", securebits(bits).map(|()| None)));
    }
    checks.push(Check::new("unix_chkpwd", own_chkpwd()));
    checks
}

fn shadow_mode() -> Option<u32> {
    std::fs::metadata(SHADOW)
        .ok()
        .map(|metadata| metadata.mode())
}

pub fn failures(checks: &[Check]) -> Vec<String> {
    checks
        .iter()
        .filter(|check| !check.passed())
        .map(Check::line)
        .collect()
}

fn own_chkpwd() -> Result<Option<String>, String> {
    let Some(path) = CHKPWD_DIRS
        .iter()
        .map(|dir| PathBuf::from(dir).join("unix_chkpwd"))
        .find(|path| path.exists())
    else {
        return Ok(Some("not installed; skipped".to_owned()));
    };
    let metadata =
        std::fs::metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let flags = rustix::fs::statvfs(&path)
        .map_err(|error| format!("statvfs {}: {error}", path.display()))?
        .f_flag;
    chkpwd(
        metadata.uid(),
        metadata.mode(),
        flags.contains(StatVfsMountFlags::NOSUID),
    )
    .map(|()| None)
    .map_err(|reason| format!("{}: {reason}", path.display()))
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests;
