use std::ffi::{CString, OsStr};
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use glimpse_config::AppletSandbox;

const VERSION: u32 = 1;
const PATH_BENEATH: u32 = 1;
const EXECUTE: u64 = 1 << 0;
const WRITE_FILE: u64 = 1 << 1;
const READ_FILE: u64 = 1 << 2;
const READ_DIR: u64 = 1 << 3;
const TRUNCATE: u64 = 1 << 14;
const IOCTL_DEV: u64 = 1 << 15;
const RESOLVE_UNIX: u64 = 1 << 16;
const ALL_FS: u64 = (1 << 17) - 1;
const ALL_NET: u64 = (1 << 4) - 1;
const ALL_SCOPES: u64 = (1 << 2) - 1;
const READ: u64 = EXECUTE | READ_FILE | READ_DIR;
const WRITE: u64 = ALL_FS & !EXECUTE;
const FILE_WRITE: u64 = READ_FILE | WRITE_FILE | TRUNCATE | IOCTL_DEV;

#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
    handled_access_net: u64,
    scoped: u64,
    quiet_access_fs: u64,
    quiet_access_net: u64,
    quiet_scoped: u64,
}

#[repr(C, packed)]
struct PathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

pub(super) fn abi() -> i64 {
    (unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<u8>(),
            0,
            VERSION,
        )
    }) as i64
}

fn require_abi(version: i64) -> Result<(), String> {
    if version < 10 {
        Err("kernel lacks Landlock ABI 10; set sandbox.unconfined to run it".to_owned())
    } else {
        Ok(())
    }
}

fn expanded(path: &str) -> Result<PathBuf, String> {
    let path = if let Some(rest) = path.strip_prefix("~/") {
        dirs::home_dir()
            .map(|home| home.join(rest))
            .ok_or_else(|| "cannot resolve home directory for sandbox path".to_owned())
    } else {
        Ok(PathBuf::from(path))
    }?;
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(format!("sandbox path is not absolute: {}", path.display()))
    }
}

fn broad(path: &Path, home: &Path) -> bool {
    path == Path::new("/") || home.starts_with(path)
}

fn path_entries(path_var: &OsStr, home: &Path) -> Vec<PathBuf> {
    std::env::split_paths(path_var)
        .filter(|path| path.is_absolute() && !broad(path, home) && !path.starts_with(home))
        .collect()
}

fn canonical(path: &Path) -> Result<Option<PathBuf>, String> {
    match std::fs::canonicalize(path) {
        Ok(path) => Ok(Some(path)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "cannot resolve sandbox path {}: {error}",
            path.display()
        )),
    }
}

fn grant(ruleset: &OwnedFd, path: &Path, home: &Path, access: u64) -> Result<(), String> {
    let Some(target) = canonical(path)? else {
        return Ok(());
    };
    if broad(&target, home) && path != target {
        return Err(format!(
            "sandbox path resolves too broadly: {}",
            path.display()
        ));
    }
    add_path(ruleset, &target, access)
}

fn add_path(ruleset: &OwnedFd, path: &Path, access: u64) -> Result<(), String> {
    let name = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| format!("invalid sandbox path: {}", path.display()))?;
    let raw = unsafe { libc::open(name.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
    if raw < 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::NotFound {
            return Ok(());
        }
        return Err(format!(
            "cannot open sandbox path {}: {error}",
            path.display()
        ));
    }
    let file = std::fs::File::from(unsafe { OwnedFd::from_raw_fd(raw) });
    let metadata = file
        .metadata()
        .map_err(|error| format!("cannot inspect sandbox path {}: {error}", path.display()))?;
    let allowed_access = if metadata.is_dir() {
        access
    } else {
        access & (EXECUTE | FILE_WRITE)
    };
    let attr = PathBeneathAttr {
        allowed_access,
        parent_fd: std::os::fd::AsRawFd::as_raw_fd(&file),
    };
    let result = unsafe {
        libc::syscall(
            libc::SYS_landlock_add_rule,
            std::os::fd::AsRawFd::as_raw_fd(ruleset),
            PATH_BENEATH,
            &attr as *const PathBeneathAttr,
            0,
        )
    };
    if result < 0 {
        return Err(format!(
            "cannot grant sandbox path {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn protect_state_dir(state: &Path) -> Result<(), String> {
    std::fs::create_dir_all(state)
        .map_err(|error| format!("cannot create applet state directory: {error}"))?;
    std::fs::set_permissions(state, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("cannot protect applet state directory: {error}"))
}

pub fn exec_state_dir(base: &Path, id: &str) -> Result<PathBuf, String> {
    let state = base.join("applets").join(id);
    protect_state_dir(&state)?;
    Ok(state)
}

pub fn exec_confine(ruleset: Option<RawFd>, parent: rustix::process::Pid) -> std::io::Result<()> {
    rustix::process::set_parent_process_death_signal(Some(rustix::process::Signal::KILL))?;
    if rustix::process::getppid() != Some(parent) {
        return Err(rustix::io::Errno::SRCH.into());
    }
    if let Some(fd) = ruleset {
        if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if unsafe { libc::syscall(libc::SYS_landlock_restrict_self, fd, 0) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

pub fn exec_sandbox(
    sandbox: &AppletSandbox,
    id: &str,
    state: &Path,
) -> Result<Option<OwnedFd>, String> {
    protect_state_dir(state)?;
    if sandbox.unconfined {
        return Ok(None);
    }
    require_abi(abi())?;
    let home = dirs::home_dir()
        .ok_or_else(|| "cannot resolve home directory for sandbox path".to_owned())?;
    let home = std::fs::canonicalize(&home)
        .map_err(|error| format!("cannot resolve home directory for sandbox path: {error}"))?;
    let attr = RulesetAttr {
        handled_access_fs: ALL_FS,
        handled_access_net: if sandbox.network { 0 } else { ALL_NET },
        scoped: ALL_SCOPES,
        quiet_access_fs: 0,
        quiet_access_net: 0,
        quiet_scoped: 0,
    };
    let raw = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            &attr as *const RulesetAttr,
            std::mem::size_of::<RulesetAttr>(),
            0,
        )
    };
    if raw < 0 {
        return Err(format!(
            "cannot create Landlock ruleset: {}",
            std::io::Error::last_os_error()
        ));
    }
    let ruleset = unsafe { OwnedFd::from_raw_fd(raw as i32) };
    for path in [
        "/usr",
        "/bin",
        "/sbin",
        "/lib",
        "/lib32",
        "/lib64",
        "/etc",
        "/opt",
        "/nix",
        "/run/current-system",
        "/proc",
        "/sys",
    ] {
        add_path(&ruleset, Path::new(path), READ)?;
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for path in path_entries(&paths, &home) {
            if let Some(target) = canonical(&path)?
                && !broad(&target, &home)
                && !target.starts_with(&home)
            {
                add_path(&ruleset, &target, READ)?;
            }
        }
    }
    if let Some(path) = glimpse_config::applet_dir(id) {
        add_path(&ruleset, &path, READ)?;
    }
    for path in &sandbox.read {
        grant(&ruleset, &expanded(path)?, &home, READ)?;
    }
    for path in [
        "/dev/null",
        "/dev/zero",
        "/dev/full",
        "/dev/random",
        "/dev/urandom",
    ] {
        add_path(&ruleset, Path::new(path), FILE_WRITE)?;
    }
    add_path(&ruleset, state, WRITE)?;
    for path in &sandbox.write {
        grant(&ruleset, &expanded(path)?, &home, WRITE)?;
    }
    if sandbox.network {
        add_path(
            &ruleset,
            Path::new("/run/systemd/resolve"),
            READ_FILE | READ_DIR | RESOLVE_UNIX,
        )?;
    }
    Ok(Some(ruleset))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use super::{exec_state_dir, expanded, path_entries, require_abi};

    #[test]
    fn state_dir_is_private() {
        let base = tempfile::tempdir().expect("tempdir");
        let state = exec_state_dir(base.path(), "me.x.A").expect("state dir");
        assert_eq!(state, base.path().join("applets/me.x.A"));
        assert_eq!(
            std::fs::metadata(state)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }

    #[test]
    fn old_kernel_requires_unconfined() {
        assert_eq!(
            require_abi(9),
            Err("kernel lacks Landlock ABI 10; set sandbox.unconfined to run it".to_owned())
        );
        assert!(require_abi(10).is_ok());
    }

    #[test]
    fn path_entries_exclude_home_and_ancestors() {
        assert_eq!(
            path_entries(
                OsStr::new("/:/home:/home/test:/home/test/bin:/usr/bin:/opt/bin:relative"),
                Path::new("/home/test")
            ),
            vec![PathBuf::from("/usr/bin"), PathBuf::from("/opt/bin")]
        );
    }

    #[test]
    fn sandbox_grants_must_be_absolute() {
        assert!(expanded("relative/path").is_err());
    }
}
