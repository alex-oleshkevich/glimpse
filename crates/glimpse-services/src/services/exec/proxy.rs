use std::ffi::OsString;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};

use glimpse_config::DbusGrant;

pub const PROXY_PROGRAM: &str = "xdg-dbus-proxy";
const READY_MS: i32 = 2000;
const READY_FD: i32 = 3;

pub struct ProxyDir(PathBuf);

impl ProxyDir {
    pub fn create() -> Result<ProxyDir, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = root()?.join(format!("{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .map_err(|error| format!("cannot create the D-Bus proxy directory: {error}"))?;
        Ok(ProxyDir(dir))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ProxyDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub struct BusProxy {
    _child: tokio::process::Child,
    _ready: OwnedFd,
    dir: ProxyDir,
}

impl BusProxy {
    pub async fn start(
        program: &Path,
        grant: &DbusGrant,
        dir: ProxyDir,
    ) -> Result<BusProxy, String> {
        let (read, write) = pipe()?;
        let mut command = tokio::process::Command::new(program);
        command
            .arg(format!("--fd={READY_FD}"))
            .arg(upstream())
            .arg(dir.path().join("bus"))
            .arg("--filter")
            .args(grant.talk.iter().map(|name| format!("--talk={name}")))
            .args(grant.call.iter().map(|rule| format!("--call={rule}")))
            .args(
                grant
                    .broadcast
                    .iter()
                    .map(|rule| format!("--broadcast={rule}")),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let raw = write.as_raw_fd();
        unsafe {
            command.pre_exec(move || {
                if raw == READY_FD {
                    if libc::fcntl(raw, libc::F_SETFD, 0) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                } else if libc::dup2(raw, READY_FD) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "sandbox.dbus needs xdg-dbus-proxy".to_owned()
            } else {
                format!("cannot start the D-Bus proxy: {error}")
            }
        })?;
        drop(write);
        let (read, ready) = tokio::task::spawn_blocking(move || {
            let ready = ready_byte(&read);
            (read, ready)
        })
        .await
        .map_err(|error| format!("D-Bus proxy did not start: {error}"))?;
        if !ready {
            return Err("D-Bus proxy did not start".to_owned());
        }
        Ok(BusProxy {
            _child: child,
            _ready: read,
            dir,
        })
    }

    pub fn address(&self) -> String {
        format!("unix:path={}", self.dir.path().join("bus").display())
    }
}

pub fn prune_dead() {
    let Ok(root) = root() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|name| name.split('-').next()) else {
            continue;
        };
        if pid.parse::<u32>().is_ok() && !Path::new("/proc").join(pid).exists() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

fn root() -> Result<PathBuf, String> {
    glimpse_config::runtime_dir()
        .map(|dir| dir.join("exec"))
        .ok_or_else(|| "cannot determine the runtime directory".to_owned())
}

fn upstream() -> OsString {
    if let Some(address) = std::env::var_os("DBUS_SESSION_BUS_ADDRESS") {
        return address;
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").unwrap_or_default();
    let mut address = OsString::from("unix:path=");
    address.push(Path::new(&runtime).join("bus"));
    address
}

fn pipe() -> Result<(OwnedFd, OwnedFd), String> {
    let mut fds = [0; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } < 0 {
        return Err(format!(
            "cannot create the D-Bus proxy pipe: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

fn ready_byte(read: &OwnedFd) -> bool {
    let mut poll = libc::pollfd {
        fd: read.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    if unsafe { libc::poll(&mut poll, 1, READY_MS) } <= 0 {
        return false;
    }
    let mut byte = 0u8;
    unsafe { libc::read(read.as_raw_fd(), (&mut byte as *mut u8).cast(), 1) == 1 }
}
