#[zbus::proxy(
    interface = "org.freedesktop.hostname1",
    default_service = "org.freedesktop.hostname1",
    default_path = "/org/freedesktop/hostname1"
)]
pub trait Hostname1 {
    #[zbus(property)]
    fn chassis(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn hostname(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn pretty_hostname(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn icon_name(&self) -> zbus::Result<String>;
}
