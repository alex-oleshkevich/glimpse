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
    fn get_unit(&self, name: &str) -> zbus::Result<OwnedObjectPath>;
    fn list_units_by_names(&self, names: &[&str]) -> zbus::Result<Vec<Systemd1UnitEntry>>;
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

#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Service",
    default_service = "org.freedesktop.systemd1"
)]
pub trait Systemd1Service {
    #[zbus(property, name = "MainPID")]
    fn main_pid(&self) -> zbus::Result<u32>;
}

#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Scope",
    default_service = "org.freedesktop.systemd1"
)]
pub trait Systemd1Scope {
    #[zbus(property(emits_changed_signal = "false"))]
    fn memory_current(&self) -> zbus::Result<u64>;
}

#[cfg(test)]
mod tests {
    use zbus::zvariant::{Type, Value};

    fn signature<T: Type + ?Sized>() -> String {
        T::SIGNATURE.to_string()
    }

    #[test]
    fn manager_and_scope_signatures_match_introspection() {
        let name = signature::<&str>();
        let properties = signature::<&[(&str, Value<'_>)]>();
        let aux = signature::<&[(&str, &[(&str, Value<'_>)])]>();
        let states = signature::<&[&str]>();
        assert_eq!(format!("{name}{name}{properties}{aux}"), "ssa(sv)a(sa(sv))");
        assert_eq!(signature::<zbus::zvariant::OwnedObjectPath>(), "o");
        assert_eq!(format!("{states}{states}"), "asas");
        assert_eq!(
            signature::<Vec<super::Systemd1UnitEntry>>(),
            "a(ssssssouso)"
        );
        assert_eq!(signature::<Vec<(String, u32, String)>>(), "a(sus)");
        assert_eq!(format!("{name}{name}{}", signature::<i32>()), "ssi");
        assert_eq!(signature::<u64>(), "t");
    }
}
