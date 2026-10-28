use zbus::zvariant::ObjectPath;

pub const SERVICE: &str = "org.bluez.obex";
pub const MANAGER: &str = "/org/bluez/obex";

#[zbus::proxy(
    interface = "org.bluez.obex.AgentManager1",
    default_service = "org.bluez.obex",
    default_path = "/org/bluez/obex"
)]
pub trait AgentManager1 {
    fn register_agent(&self, agent: &ObjectPath<'_>) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.bluez.obex.Transfer1",
    default_service = "org.bluez.obex"
)]
pub trait Transfer1 {
    fn cancel(&self) -> zbus::Result<()>;

    #[zbus(property)]
    fn status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn filename(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn size(&self) -> zbus::Result<u64>;
    #[zbus(property)]
    fn transferred(&self) -> zbus::Result<u64>;
    #[zbus(property)]
    fn session(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

#[zbus::proxy(
    interface = "org.bluez.obex.Session1",
    default_service = "org.bluez.obex"
)]
pub trait Session1 {
    #[zbus(property)]
    fn destination(&self) -> zbus::Result<String>;
}
