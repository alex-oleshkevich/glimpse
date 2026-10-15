use zbus::zvariant::{OwnedObjectPath, Value};

pub type Systemd1UnitEntry = (
    String,
    String,
    String,
    String,
    String,
    String,
    OwnedObjectPath,
    u32,
    String,
    OwnedObjectPath,
);

#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
pub trait Systemd1Manager {
    fn start_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn restart_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn try_restart_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn get_unit(&self, name: &str) -> zbus::Result<OwnedObjectPath>;
    fn list_units_by_names(&self, names: &[&str]) -> zbus::Result<Vec<Systemd1UnitEntry>>;
    fn set_environment(&self, assignments: &[&str]) -> zbus::Result<()>;
    fn start_transient_unit(
        &self,
        name: &str,
        mode: &str,
        properties: &[(&str, Value<'_>)],
        aux: &[(&str, &[(&str, Value<'_>)])],
    ) -> zbus::Result<OwnedObjectPath>;
    fn list_units_by_patterns(
        &self,
        states: &[&str],
        patterns: &[&str],
    ) -> zbus::Result<Vec<Systemd1UnitEntry>>;
    fn get_unit_processes(&self, name: &str) -> zbus::Result<Vec<(String, u32, String)>>;
    fn kill_unit(&self, name: &str, whom: &str, signal: i32) -> zbus::Result<()>;
    fn subscribe(&self) -> zbus::Result<()>;
    fn reset_failed_unit(&self, name: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    fn job_removed(
        &self,
        id: u32,
        job: OwnedObjectPath,
        unit: String,
        result: String,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn unit_removed(&self, id: String, unit: OwnedObjectPath) -> zbus::Result<()>;
    #[zbus(property(emits_changed_signal = "false"))]
    fn environment(&self) -> zbus::Result<Vec<String>>;
}

#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Unit",
    default_service = "org.freedesktop.systemd1"
)]
pub trait Systemd1Unit {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn active_state(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn sub_state(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn active_enter_timestamp(&self) -> zbus::Result<u64>;
}

#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Service",
    default_service = "org.freedesktop.systemd1"
)]
pub trait Systemd1Service {
    #[zbus(property, name = "MainPID")]
    fn main_pid(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn result(&self) -> zbus::Result<String>;
    #[zbus(property, name = "NRestarts")]
    fn n_restarts(&self) -> zbus::Result<u32>;
    #[zbus(property(emits_changed_signal = "false"))]
    fn memory_current(&self) -> zbus::Result<u64>;
}

#[cfg(test)]
mod tests;
