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

    pub fn set(&self, backend: Backend, next: BackendHealth) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_writes_only_its_own_backend() {
        let (health, _changes) = Health::new();
        health.degraded(Backend::Wayland, "no ext-idle-notify-v1");

        let snapshot = health.snapshot();
        assert_eq!(snapshot.wayland.kind, HealthKind::Degraded);
        assert_eq!(snapshot.wayland.message, "no ext-idle-notify-v1");
        assert_eq!(
            snapshot.screen_saver.kind,
            HealthKind::Unsupported,
            "the other three start unknown and stay there"
        );
    }

    #[test]
    fn an_unchanged_write_does_not_bump_the_generation() {
        let (health, mut changes) = Health::new();
        changes.borrow_and_update();

        health.ready(Backend::Login1);
        assert!(changes.has_changed().expect("the sender is live"));
        changes.borrow_and_update();

        health.ready(Backend::Login1);
        assert!(
            !changes.has_changed().expect("the sender is live"),
            "a repeat of the same health would otherwise emit a PropertiesChanged per poll"
        );
    }

    #[test]
    fn a_degraded_message_is_capped_like_any_other_backend_text() {
        let (health, _changes) = Health::new();
        health.degraded(Backend::Portal, "x".repeat(4096));
        assert!(health.snapshot().portal.message.chars().count() <= super::super::WHY_CAP + 2);
    }
}
