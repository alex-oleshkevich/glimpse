use std::os::fd::OwnedFd;

use async_trait::async_trait;

use glimpse_core::dbus::login1::Login1ManagerProxy;

use crate::dbus::idle_inhibitor_screen_saver::Login1InhibitTaker;

/// Real implementation of `Login1InhibitTaker` that wraps the Login1Manager
/// D-Bus proxy. The fd is held until dropped, at which point the kernel
/// closes it and logind releases the corresponding inhibit.
pub struct RealLogin1Inhibit {
    pub proxy: Login1ManagerProxy<'static>,
}

#[async_trait]
impl Login1InhibitTaker for RealLogin1Inhibit {
    async fn take(&self, what: &str, who: &str, why: &str) -> Result<OwnedFd, zbus::Error> {
        let fd = self.proxy.inhibit(what, who, why, "block").await?;
        Ok(zbus_fd_into_std(fd))
    }
}

/// Convert zbus's `zvariant::OwnedFd` into `std::os::fd::OwnedFd`. The two
/// types are distinct in zbus 5 (zvariant wraps `std::os::fd::OwnedFd`);
/// zvariant provides a direct `From` impl that unwraps the inner fd.
pub fn zbus_fd_into_std(fd: zbus::zvariant::OwnedFd) -> std::os::fd::OwnedFd {
    fd.into()
}

/// Resolve a D-Bus unique sender name to a best-effort process name via
/// `GetConnectionUnixProcessID` + `/proc/<pid>/comm`. Returns `None` on any
/// failure (caller treats this as "stay unresolved").
pub async fn resolve_process_name(conn: &zbus::Connection, bus_name: &str) -> Option<String> {
    use zbus::fdo::DBusProxy;
    use zbus::names::BusName;
    let dbus = DBusProxy::new(conn).await.ok()?;
    let bn = BusName::try_from(bus_name).ok()?;
    let pid = dbus.get_connection_unix_process_id(bn).await.ok()?;
    tokio::task::spawn_blocking(move || {
        std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .ok()
            .map(|s| s.trim().to_string())
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zbus_fd_into_std_preserves_fd() {
        use std::os::fd::AsRawFd;
        let file = std::fs::File::open("/dev/null").unwrap();
        let std_fd: std::os::fd::OwnedFd = file.into();
        let zfd: zbus::zvariant::OwnedFd = std_fd.into();
        let round = zbus_fd_into_std(zfd);
        assert!(round.as_raw_fd() >= 0);
    }
}
