use zbus::zvariant::OwnedObjectPath;

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
    fn get_unit(&self, name: &str) -> zbus::Result<OwnedObjectPath>;
    fn list_units_by_names(&self, names: &[&str]) -> zbus::Result<Vec<Systemd1UnitEntry>>;
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
}
