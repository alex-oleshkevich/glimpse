use std::collections::HashMap;

use zbus::zvariant::Value;

#[zbus::proxy(
    interface = "org.freedesktop.PolicyKit1.Authority",
    default_service = "org.freedesktop.PolicyKit1",
    default_path = "/org/freedesktop/PolicyKit1/Authority"
)]
pub trait Authority {
    fn register_authentication_agent(
        &self,
        subject: (&str, HashMap<&str, Value<'_>>),
        locale: &str,
        object_path: &str,
    ) -> zbus::Result<()>;

    fn unregister_authentication_agent(
        &self,
        subject: (&str, HashMap<&str, Value<'_>>),
        object_path: &str,
    ) -> zbus::Result<()>;
}
