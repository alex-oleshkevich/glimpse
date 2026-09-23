use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use rustix::fs::StatVfsMountFlags;

const CHKPWD_DIRS: [&str; 3] = ["/usr/bin", "/usr/sbin", "/sbin"];
const PAM_DIRS: [&str; 2] = ["/etc/pam.d", "/usr/lib/pam.d"];
const NOT_A_SERVICE: &str = "not a PAM service name";

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
    let name = format!("pam.d/{}", glimpse_utils::clean(service, 64));
    if service.is_empty()
        || service.starts_with('.')
        || service.contains('/')
        || service.contains('\0')
    {
        return Check::new(name, Err(NOT_A_SERVICE.to_owned()));
    }
    let path = pam_path(service, dirs, &exists);
    let text = path.as_deref().and_then(&read);
    let outcome = pam_file(text.as_deref())
        .and_then(|()| {
            pam_includes_resolve(text.as_deref().unwrap_or_default(), dirs, &exists, &read)
        })
        .map(|()| path.map(|path| path.display().to_string()));
    Check::new(name, outcome)
}

pub fn startup(pam_service: &str) -> Vec<Check> {
    let mut checks = own();
    checks.push(pam_stack(pam_service));
    checks
}

pub fn process(root: &Path, prefix: &str) -> Vec<Check> {
    vec![
        Check::new(
            format!("{prefix}uid_map"),
            read(&root.join("uid_map")).and_then(|text| uid_map(&text).map(|()| None)),
        ),
        Check::new(
            format!("{prefix}NoNewPrivs"),
            read(&root.join("status")).and_then(|text| no_new_privs(&text).map(|()| None)),
        ),
    ]
}

pub fn own() -> Vec<Check> {
    let mut checks = process(Path::new("/proc/self"), "");
    checks.push(Check::new("unix_chkpwd", own_chkpwd()));
    checks
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
mod tests {
    use super::*;

    #[test]
    fn only_the_padded_identity_map_is_the_initial_namespace() {
        assert_eq!(uid_map("         0          0 4294967295\n"), Ok(()));
        assert!(uid_map("      1000       1000          1\n").is_err());
        assert!(uid_map("         0          0 4294967294\n").is_err());
        assert!(uid_map("0 0 4294967295\n0 0 1\n").is_err());
        assert!(uid_map("").is_err());
        assert!(uid_map("0 0 x\n").is_err());
    }

    #[test]
    fn no_new_privs_must_read_zero() {
        let status = |value: &str| {
            format!(
                "Name:\tglimpse-lock\nUid:\t1000\t1000\t1000\t1000\nNoNewPrivs:\t{value}\nSeccomp:\t0\n"
            )
        };
        assert_eq!(no_new_privs(&status("0")), Ok(()));
        assert!(no_new_privs(&status("1")).is_err());
        assert!(no_new_privs("Name:\tx\n").is_err());
    }

    #[test]
    fn unix_chkpwd_must_be_setuid_or_setgid_root_on_a_suid_filesystem() {
        assert_eq!(chkpwd(0, 0o106755, false), Ok(()));
        assert_eq!(chkpwd(0, 0o104755, false), Ok(()));
        assert_eq!(
            chkpwd(0, 0o102755, false),
            Ok(()),
            "Debian ships it 2755 root:shadow"
        );
        assert!(chkpwd(0, 0o100755, false).is_err());
        assert!(chkpwd(1000, 0o104755, false).is_err());
        assert!(chkpwd(65534, 0o106755, false).is_err());
        assert!(chkpwd(0, 0o106755, true).is_err());
        assert!(chkpwd(0, 0o102755, true).is_err());
    }

    #[test]
    fn a_pam_file_must_exist_and_permit_nothing() {
        let shipped = include_str!("../../../data/pam.d/glimpse-lock");
        assert_eq!(pam_file(Some(shipped)), Ok(()));
        let debian = include_str!("../../../data/pam.d/debian/glimpse-lock");
        assert_eq!(pam_file(Some(debian)), Ok(()));
        assert!(pam_file(None).is_err());
        assert!(pam_file(Some("auth required pam_permit.so\n")).is_err());
        assert!(pam_file(Some("auth sufficient /usr/lib/security/pam_permit.so\n")).is_err());
        assert_eq!(pam_file(Some("#auth required pam_permit.so\n")), Ok(()));
        assert_eq!(
            pam_file(Some("auth include system-auth # pam_permit.so\n")),
            Ok(())
        );
    }

    #[test]
    fn the_pam_stack_is_looked_up_in_etc_before_usr_lib() {
        let both = |_: &Path| true;
        assert_eq!(
            pam_path("glimpse-lock", &PAM_DIRS, both),
            Some(PathBuf::from("/etc/pam.d/glimpse-lock"))
        );
        let vendor = |path: &Path| path.starts_with("/usr/lib/pam.d");
        assert_eq!(
            pam_path("glimpse-lock", &PAM_DIRS, vendor),
            Some(PathBuf::from("/usr/lib/pam.d/glimpse-lock"))
        );
        assert_eq!(pam_path("glimpse-lock", &PAM_DIRS, |_| false), None);
    }

    #[test]
    fn a_missing_pam_service_refuses_at_startup() {
        let missing = "glimpse-lock-no-such-service-for-tests";
        assert!(!failures(&startup(missing)).is_empty());
        assert!(!pam_stack(missing).passed());
    }

    #[test]
    fn a_malformed_pam_service_is_refused_for_its_name_alone() {
        let shipped = include_str!("../../../data/pam.d/glimpse-lock");
        let stack =
            |name: &str| pam_stack_in(name, &PAM_DIRS, |_| true, |_| Some(shipped.to_owned()));
        assert_eq!(
            stack("glimpse-lock").outcome,
            Ok(Some("/etc/pam.d/glimpse-lock".to_owned()))
        );
        for name in [".hidden", "a\0b", "..", ".", "/abs", "a/b", "../passwd", ""] {
            assert_eq!(
                stack(name).outcome,
                Err(NOT_A_SERVICE.to_owned()),
                "{name:?} must be refused as a name"
            );
        }
    }

    #[test]
    fn every_include_substack_and_debian_include_is_found() {
        let shipped = include_str!("../../../data/pam.d/glimpse-lock");
        assert_eq!(pam_includes(shipped), ["system-auth", "system-auth"]);
        let debian = include_str!("../../../data/pam.d/debian/glimpse-lock");
        assert_eq!(pam_includes(debian), ["common-auth", "common-account"]);
        let fedora = include_str!("../../../data/pam.d/fedora/glimpse-lock");
        assert_eq!(pam_includes(fedora), ["password-auth", "password-auth"]);
        assert_eq!(
            pam_includes(
                "-auth substack login\n\
                 auth [success=1 default=ignore] pam_unix.so\n\
                 # auth include commented\n\
                 auth required pam_deny.so # include trailing\n\
                 account include\n\
                 auth \\\n  include continued\n"
            ),
            ["login", "continued"]
        );
    }

    #[test]
    fn a_missing_file_behind_a_nested_include_fails() {
        let dirs = ["/etc/pam.d"];
        let exists = |path: &Path| {
            path == Path::new("/etc/pam.d/glimpse-lock")
                || path == Path::new("/etc/pam.d/system-auth")
        };
        let read = |path: &Path| {
            Some(match path == Path::new("/etc/pam.d/system-auth") {
                true => "auth include gone\nauth include system-auth\n".to_owned(),
                false => "auth include system-auth\n".to_owned(),
            })
        };
        let outcome = pam_stack_in("glimpse-lock", &dirs, exists, read).outcome;
        assert!(
            outcome
                .as_ref()
                .is_err_and(|reason| reason.contains("\"gone\"")),
            "{outcome:?}"
        );
    }

    #[test]
    fn a_stack_including_a_missing_file_fails() {
        let dirs = ["/etc/pam.d", "/usr/lib/pam.d"];
        let shipped = include_str!("../../../data/pam.d/glimpse-lock");
        let read = |_: &Path| Some(shipped.to_owned());
        let only_ours = |path: &Path| path == Path::new("/etc/pam.d/glimpse-lock");
        let outcome = pam_stack_in("glimpse-lock", &dirs, only_ours, read).outcome;
        assert!(
            outcome
                .as_ref()
                .is_err_and(|reason| reason.contains("\"system-auth\"")),
            "{outcome:?}"
        );
        let vendor_auth = |path: &Path| {
            path == Path::new("/etc/pam.d/glimpse-lock")
                || path == Path::new("/usr/lib/pam.d/system-auth")
        };
        assert_eq!(
            pam_stack_in("glimpse-lock", &dirs, vendor_auth, read).outcome,
            Ok(Some("/etc/pam.d/glimpse-lock".to_owned())),
            "an include found in the second directory resolves"
        );
        let absolute = |path: &Path| path == Path::new("/etc/pam.d/glimpse-lock");
        let text = |_: &Path| Some("auth include /opt/pam/auth\n".to_owned());
        assert!(
            pam_stack_in("glimpse-lock", &dirs, absolute, text)
                .outcome
                .is_err()
        );
    }

    #[test]
    fn a_report_is_one_line_per_check() {
        assert_eq!(Check::new("uid_map", Ok(None)).line(), "uid_map: ok");
        assert_eq!(
            Check::new("unix_chkpwd", Ok(Some("not installed; skipped".into()))).line(),
            "unix_chkpwd: ok (not installed; skipped)"
        );
        let failed = Check::new("NoNewPrivs", Err("NoNewPrivs is 1".into()));
        assert_eq!(failed.line(), "NoNewPrivs: fail (NoNewPrivs is 1)");
        assert_eq!(
            failures(&[Check::new("a", Ok(None)), failed]),
            vec!["NoNewPrivs: fail (NoNewPrivs is 1)".to_owned()]
        );
    }
}
