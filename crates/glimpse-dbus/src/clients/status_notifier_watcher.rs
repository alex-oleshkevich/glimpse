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

    /// Take the whole set from what another watcher reports, in the order it reports them, and
    /// answer with the keys it no longer lists. A host reading someone else's watcher cannot
    /// reconcile signal by signal: a watcher spells an item however the application registered it,
    /// and only a fresh read resolves that back to an owner.
    pub fn resync(&mut self, keys: &[String]) -> Vec<String> {
        let gone: Vec<String> = self
            .items
            .iter()
            .filter(|item| !keys.contains(item))
            .cloned()
            .collect();
        self.items = keys.to_vec();
        gone
    }

    pub fn items(&self) -> &[String] {
        &self.items
    }

    pub fn host_registered(&self) -> bool {
        !self.hosts.is_empty()
    }
}

#[cfg(test)]
mod tests;
