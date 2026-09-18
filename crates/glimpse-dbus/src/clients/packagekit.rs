use zbus::zvariant::OwnedObjectPath;

pub const FILTER_NONE: u64 = 0;

#[zbus::proxy(
    interface = "org.freedesktop.PackageKit",
    default_service = "org.freedesktop.PackageKit",
    default_path = "/org/freedesktop/PackageKit"
)]
pub trait PackageKit {
    fn create_transaction(&self) -> zbus::Result<OwnedObjectPath>;

    #[zbus(signal)]
    fn updates_changed(&self) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.PackageKit.Transaction",
    default_service = "org.freedesktop.PackageKit",
    assume_defaults = false
)]
pub trait PackageKitTransaction {
    fn get_updates(&self, filters: u64) -> zbus::Result<()>;

    #[zbus(signal)]
    fn package(&self, info: u32, package_id: &str, summary: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    fn finished(&self, exit: u32, runtime: u32) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::FILTER_NONE;

    #[test]
    fn get_updates_uses_a_bitfield_and_none_is_zero() {
        assert_eq!(FILTER_NONE, 0);
    }
}
