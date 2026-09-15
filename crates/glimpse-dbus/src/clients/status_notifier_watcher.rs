pub const WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";
pub const WATCHER_PATH: &str = "/StatusNotifierWatcher";
/// A minority of toolkits look for the freedesktop spelling instead, at the same path.
pub const WATCHER_ALIAS: &str = "org.freedesktop.StatusNotifierWatcher";
pub const DEFAULT_ITEM_PATH: &str = "/StatusNotifierItem";

#[zbus::proxy(
    interface = "org.kde.StatusNotifierWatcher",
    default_service = "org.kde.StatusNotifierWatcher",
    default_path = "/StatusNotifierWatcher"
)]
pub trait StatusNotifierWatcher {
    fn register_status_notifier_item(&self, service: &str) -> zbus::Result<()>;
    fn register_status_notifier_host(&self, service: &str) -> zbus::Result<()>;
    fn unregister_status_notifier_item(&self, service: &str) -> zbus::Result<()>;

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> zbus::Result<Vec<String>>;
    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn protocol_version(&self) -> zbus::Result<i32>;

    #[zbus(signal)]
    fn status_notifier_item_registered(&self, service: String) -> zbus::Result<()>;
    #[zbus(signal)]
    fn status_notifier_item_unregistered(&self, service: String) -> zbus::Result<()>;
    #[zbus(signal)]
    fn status_notifier_host_registered(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn status_notifier_host_unregistered(&self) -> zbus::Result<()>;
}

/// An item registers with either a bus name or an object path, and some send neither usefully, so
/// the sender from the message header is the authoritative half. A bus name never contains `/`.
pub fn canonical_key(sender: &str, argument: &str) -> String {
    let path = if argument.starts_with('/') {
        argument
    } else {
        DEFAULT_ITEM_PATH
    };
    format!("{sender}{path}")
}

/// Splits a key back into the owner's bus name and the object path on it.
pub fn split_key(key: &str) -> Option<(&str, &str)> {
    let at = key.find('/')?;
    Some((&key[..at], &key[at..]))
}

/// Who has registered, in registration order. The watcher interface is a shell over this: keeping
/// the rules here is what lets them be tested without a bus.
#[derive(Debug, Default)]
pub struct Registry {
    items: Vec<String>,
    hosts: Vec<String>,
}

impl Registry {
    /// `None` when the item was already registered — a repeat is idempotent, not an error, because
    /// an application that re-registers after a host restart would otherwise appear twice.
    pub fn register(&mut self, sender: &str, argument: &str) -> Option<String> {
        // Without a sender the key has no bus name in it, so every later `destination()` fails:
        // the item would hold a slot nothing could read or activate. Refuse it instead.
        if sender.is_empty() {
            return None;
        }
        let key = canonical_key(sender, argument);
        if self.items.contains(&key) {
            return None;
        }
        self.items.push(key.clone());
        Some(key)
    }

    pub fn unregister(&mut self, sender: &str, argument: &str) -> Option<String> {
        let key = canonical_key(sender, argument);
        let at = self.items.iter().position(|item| *item == key)?;
        Some(self.items.remove(at))
    }

    pub fn register_host(&mut self, sender: &str) -> bool {
        if self.hosts.iter().any(|host| host == sender) {
            return false;
        }
        self.hosts.push(sender.to_owned());
        true
    }

    /// Everything an owner held, for a `NameOwnerChanged` that reports it gone. Most applications
    /// never unregister, so this is how a dead chip actually leaves the bar.
    pub fn evict_owner(&mut self, owner: &str) -> Vec<String> {
        let gone: Vec<String> = self
            .items
            .iter()
            .filter(|item| split_key(item).is_some_and(|(name, _)| name == owner))
            .cloned()
            .collect();
        self.items.retain(|item| !gone.contains(item));
        self.hosts.retain(|host| host != owner);
        gone
    }

    /// Drop a key the host itself decided is gone — an item that stopped answering never sends an
    /// unregistration, and its owner may still hold the bus name.
    pub fn unregister_key(&mut self, key: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|item| item != key);
        before != self.items.len()
    }

    pub fn items(&self) -> &[String] {
        &self.items
    }

    pub fn host_registered(&self) -> bool {
        !self.hosts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{Registry, canonical_key, split_key};

    #[test]
    fn a_bus_name_argument_falls_back_to_the_default_path() {
        assert_eq!(
            canonical_key(":1.42", "org.kde.StatusNotifierItem-8123-1"),
            ":1.42/StatusNotifierItem"
        );
    }

    #[test]
    fn a_path_argument_is_taken_as_the_path_and_the_sender_as_the_owner() {
        assert_eq!(
            canonical_key(":1.42", "/org/ayatana/NotificationItem/fake"),
            ":1.42/org/ayatana/NotificationItem/fake"
        );
    }

    #[test]
    fn a_key_splits_at_the_first_slash_because_a_bus_name_has_none() {
        assert_eq!(
            split_key(":1.42/org/ayatana/NotificationItem/fake"),
            Some((":1.42", "/org/ayatana/NotificationItem/fake"))
        );
        assert_eq!(split_key("nothing-here"), None);
    }

    #[test]
    fn registering_twice_adds_one_entry() {
        let mut registry = Registry::default();
        assert!(registry.register(":1.1", "/StatusNotifierItem").is_some());
        assert!(
            registry
                .register(":1.1", "org.kde.StatusNotifierItem-1-1")
                .is_none()
        );
        assert_eq!(registry.items(), [":1.1/StatusNotifierItem"]);
    }

    #[test]
    fn unregister_removes_exactly_one_and_reports_nothing_for_a_stranger() {
        let mut registry = Registry::default();
        registry.register(":1.1", "/StatusNotifierItem");
        registry.register(":1.2", "/StatusNotifierItem");
        assert_eq!(
            registry
                .unregister(":1.1", "/StatusNotifierItem")
                .as_deref(),
            Some(":1.1/StatusNotifierItem")
        );
        assert_eq!(registry.items(), [":1.2/StatusNotifierItem"]);
        assert!(registry.unregister(":1.9", "/StatusNotifierItem").is_none());
    }

    #[test]
    fn one_owner_may_hold_several_items_and_loses_all_of_them_together() {
        let mut registry = Registry::default();
        registry.register(":1.1", "/StatusNotifierItem");
        registry.register(":1.1", "/org/ayatana/NotificationItem/second");
        registry.register(":1.2", "/StatusNotifierItem");

        assert_eq!(
            registry.evict_owner(":1.1"),
            [
                ":1.1/StatusNotifierItem",
                ":1.1/org/ayatana/NotificationItem/second"
            ]
        );
        assert_eq!(registry.items(), [":1.2/StatusNotifierItem"]);
    }

    #[test]
    fn a_host_is_registered_once_and_leaves_with_its_owner() {
        let mut registry = Registry::default();
        assert!(registry.register_host(":1.5"));
        assert!(!registry.register_host(":1.5"));
        assert!(registry.host_registered());
        registry.evict_owner(":1.5");
        assert!(!registry.host_registered());
    }
}
