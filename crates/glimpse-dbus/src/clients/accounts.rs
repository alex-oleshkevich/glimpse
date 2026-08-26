use zbus::zvariant::OwnedObjectPath;

#[zbus::proxy(
    interface = "org.freedesktop.Accounts",
    default_service = "org.freedesktop.Accounts",
    default_path = "/org/freedesktop/Accounts"
)]
pub trait Accounts {
    fn find_user_by_name(&self, username: &str) -> zbus::Result<OwnedObjectPath>;
    fn find_user_by_id(&self, uid: i64) -> zbus::Result<OwnedObjectPath>;
    fn list_cached_users(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[zbus::proxy(
    interface = "org.freedesktop.Accounts.User",
    default_service = "org.freedesktop.Accounts"
)]
pub trait AccountsUser {
    #[zbus(property)]
    fn user_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn real_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn icon_file(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn uid(&self) -> zbus::Result<u64>;
}
