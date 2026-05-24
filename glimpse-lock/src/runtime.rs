use std::{
    collections::HashSet,
    sync::{Mutex, OnceLock},
};

use anyhow::{Context, anyhow, bail};
use zbus::fdo::{DBusProxy, RequestNameFlags, RequestNameReply};

pub const APP_ID: &str = "me.aresa.GlimpseLock";
pub const GTK_APPLICATION_ID: &str = "me.aresa.GlimpseLock.App";
pub const GTK_PREVIEW_APPLICATION_ID: &str = "me.aresa.GlimpseLock.Preview";

#[derive(Debug, Default)]
pub struct LockRuntime {
    locked: bool,
    auth_success: bool,
}

impl LockRuntime {
    pub fn mark_locked(&mut self) {
        self.locked = true;
    }

    pub fn reset(&mut self) {
        self.locked = false;
        self.auth_success = false;
    }

    pub fn mark_auth_success(&mut self) {
        self.auth_success = true;
    }

    /// Clears the authenticated flag. Used after a failed unlock attempt or
    /// when a stale auth result should be discarded.
    pub fn clear_auth_success(&mut self) {
        self.auth_success = false;
    }

    pub fn can_unlock(&self) -> bool {
        self.locked && self.auth_success
    }

    pub async fn acquire_single_instance() -> anyhow::Result<InstanceGuard> {
        acquire_dbus_name(APP_ID).await
    }

    pub async fn acquire_single_instance_with_name(
        name: impl Into<String>,
    ) -> anyhow::Result<InstanceGuard> {
        acquire_dbus_name(name.into()).await
    }

    /// Test-only equivalent of [`acquire_single_instance`]: uses a
    /// process-local registry rather than the session bus so integration
    /// tests can verify single-instance semantics without touching D-Bus.
    /// Not gated behind `#[cfg(test)]` because the integration tests in
    /// `tests/runtime_behavior.rs` live in a separate crate that
    /// `#[cfg(test)]` does not include.
    #[doc(hidden)]
    pub async fn acquire_single_instance_for_testing(
        name: &str,
    ) -> anyhow::Result<TestInstanceGuard> {
        TestInstanceGuard::acquire(name)
    }
}

pub struct InstanceGuard {
    connection: zbus::Connection,
}

impl InstanceGuard {
    pub fn connection(&self) -> zbus::Connection {
        self.connection.clone()
    }
}

async fn acquire_dbus_name(name: impl Into<String>) -> anyhow::Result<InstanceGuard> {
    let name = name.into();
    tracing::debug!(name, "connecting to session D-Bus");
    let connection = zbus::Connection::session()
        .await
        .context("connect to session D-Bus")?;
    let proxy = DBusProxy::new(&connection)
        .await
        .context("create session D-Bus proxy")?;
    let well_known_name = zbus::names::WellKnownName::try_from(name.as_str())
        .with_context(|| format!("validate D-Bus name {name}"))?;
    let reply = proxy
        .request_name(well_known_name, RequestNameFlags::DoNotQueue.into())
        .await
        .with_context(|| format!("request D-Bus name {name}"))?;

    match reply {
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner => {
            tracing::debug!(name, reply = ?reply, "D-Bus name acquired");
            Ok(InstanceGuard { connection })
        }
        RequestNameReply::Exists | RequestNameReply::InQueue => {
            bail!("another glimpse-lock instance already owns {name}")
        }
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub struct TestInstanceGuard {
    name: String,
}

impl TestInstanceGuard {
    fn acquire(name: &str) -> anyhow::Result<Self> {
        let mut names = test_names()
            .lock()
            .map_err(|_| anyhow!("test instance registry is poisoned"))?;
        if names.contains(name) {
            bail!("another glimpse-lock instance already owns {name}");
        }
        names.insert(name.to_string());
        Ok(Self { name: name.into() })
    }
}

impl Drop for TestInstanceGuard {
    fn drop(&mut self) {
        if let Ok(mut names) = test_names().lock() {
            names.remove(&self.name);
        }
    }
}

fn test_names() -> &'static Mutex<HashSet<String>> {
    static NAMES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    NAMES.get_or_init(|| Mutex::new(HashSet::new()))
}
