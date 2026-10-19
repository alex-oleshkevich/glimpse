use std::sync::{Arc, Mutex};

use glimpse_dbus::idle::{BackendHealth, HealthKind, InhibitorsHealth};
use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    ScreenSaver,
    Portal,
    Login1,
    Wayland,
}

#[derive(Clone)]
pub struct Health {
    state: Arc<Mutex<InhibitorsHealth>>,
    generation: watch::Sender<u64>,
}

impl Health {
    pub fn new() -> (Self, watch::Receiver<u64>) {
        let (generation, changes) = watch::channel(0);
        let health = Self {
            state: Arc::new(Mutex::new(InhibitorsHealth::default())),
            generation,
        };
        (health, changes)
    }

    pub fn snapshot(&self) -> InhibitorsHealth {
        self.held().clone()
    }

    pub fn ready(&self, backend: Backend) {
        self.set(
            backend,
            BackendHealth {
                kind: HealthKind::Ready,
                message: String::new(),
            },
        );
    }

    pub fn degraded(&self, backend: Backend, message: impl AsRef<str>) {
        self.set(
            backend,
            BackendHealth {
                kind: HealthKind::Degraded,
                message: glimpse_utils::clean(message.as_ref(), super::WHY_CAP),
            },
        );
    }

    pub fn unsupported(&self, backend: Backend, message: impl AsRef<str>) {
        self.set(
            backend,
            BackendHealth {
                kind: HealthKind::Unsupported,
                message: glimpse_utils::clean(message.as_ref(), super::WHY_CAP),
            },
        );
    }

    pub async fn name_taken(&self, connection: &zbus::Connection, backend: Backend, name: &str) {
        match owner_comm(connection, name).await {
            Some(comm) => self.unsupported(backend, format!("served by {comm}")),
            None => self.degraded(backend, "Bus name already owned"),
        }
    }

    fn set(&self, backend: Backend, next: BackendHealth) {
        let changed = {
            let mut held = self.held();
            let slot = match backend {
                Backend::ScreenSaver => &mut held.screen_saver,
                Backend::Portal => &mut held.portal,
                Backend::Login1 => &mut held.login1,
                Backend::Wayland => &mut held.wayland,
            };
            if *slot == next {
                false
            } else {
                *slot = next;
                true
            }
        };
        if changed {
            self.generation
                .send_modify(|value| *value = value.wrapping_add(1));
        }
    }

    #[cfg(test)]
    pub fn kind(&self, backend: Backend) -> HealthKind {
        let held = self.held();
        match backend {
            Backend::ScreenSaver => held.screen_saver.kind,
            Backend::Portal => held.portal.kind,
            Backend::Login1 => held.login1.kind,
            Backend::Wayland => held.wayland.kind,
        }
    }

    fn held(&self) -> std::sync::MutexGuard<'_, InhibitorsHealth> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

async fn owner_comm(connection: &zbus::Connection, name: &str) -> Option<String> {
    let dbus = zbus::fdo::DBusProxy::new(connection).await.ok()?;
    let bus_name = zbus::names::BusName::try_from(name).ok()?;
    let owner = dbus.get_name_owner(bus_name).await.ok()?;
    let pid = dbus
        .get_connection_unix_process_id(zbus::names::BusName::from(owner))
        .await
        .ok()?;
    let comm = tokio::fs::read_to_string(format!("/proc/{pid}/comm"))
        .await
        .ok()?;
    Some(glimpse_utils::clean(&comm, super::WHO_CAP))
}

#[cfg(test)]
mod tests;
