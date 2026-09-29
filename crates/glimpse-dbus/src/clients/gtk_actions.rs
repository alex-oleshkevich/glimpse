use std::collections::HashMap;

use zbus::zvariant::{OwnedValue, Signature, Value};

#[zbus::proxy(interface = "org.gtk.Actions", assume_defaults = false)]
pub trait GtkActions {
    fn list(&self) -> zbus::Result<Vec<String>>;
    fn describe_all(&self) -> zbus::Result<HashMap<String, (bool, Signature, Vec<OwnedValue>)>>;
    fn activate(
        &self,
        action_name: &str,
        parameter: Vec<Value<'_>>,
        platform_data: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    fn changed(
        &self,
        removals: Vec<String>,
        enable_changes: HashMap<String, bool>,
        state_changes: HashMap<String, OwnedValue>,
        additions: HashMap<String, (bool, Signature, Vec<OwnedValue>)>,
    ) -> zbus::Result<()>;
}

pub fn object_path(name: &str) -> String {
    format!("/{}", name.replace('.', "/").replace('-', "_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_names_become_object_paths() {
        assert_eq!(
            object_path("me.aresa.GlimpsePanel"),
            "/me/aresa/GlimpsePanel"
        );
        assert_eq!(object_path("a.b-c"), "/a/b_c");
    }
}
