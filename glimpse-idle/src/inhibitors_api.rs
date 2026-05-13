use std::sync::Arc;

use tokio::sync::Mutex;

use glimpse_core::services::idle_inhibitor::{IdleInhibitorRecord, InhibitorsHealth};

use crate::inhibitor_registry::Registry;

pub struct InhibitorsApi {
    pub registry: Arc<Mutex<Registry>>,
    pub health: Arc<Mutex<InhibitorsHealth>>,
    pub on_change: Arc<dyn Fn() + Send + Sync>,
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use tokio::sync::Mutex;

    use super::*;
    use crate::inhibitor_registry::{Registry, build_screen_saver_record};
    use glimpse_core::services::idle_inhibitor::{InhibitionTargets, InhibitorsHealth};

    #[tokio::test]
    async fn release_notifies_when_record_is_removed() {
        let mut registry = Registry::new();
        let id = registry.mint_id();
        let cookie = registry.mint_cookie();
        registry.insert(
            build_screen_saver_record(
                id,
                cookie,
                "CLI test".into(),
                "release notification".into(),
                ":1.42".into(),
                InhibitionTargets::idle_only(),
            ),
            None,
        );

        let notifications = Arc::new(AtomicUsize::new(0));
        let api = InhibitorsApi {
            registry: Arc::new(Mutex::new(registry)),
            health: Arc::new(Mutex::new(InhibitorsHealth::default())),
            on_change: {
                let notifications = notifications.clone();
                Arc::new(move || {
                    notifications.fetch_add(1, Ordering::SeqCst);
                })
            },
        };

        api.release(id).await.expect("release should succeed");

        assert_eq!(notifications.load(Ordering::SeqCst), 1);
    }
}

#[zbus::interface(name = "me.aresa.GlimpseIdle.Inhibitors")]
impl InhibitorsApi {
    /// Current snapshot of every active inhibitor across all sources.
    /// Shell subscribes to PropertiesChanged on this property to mirror state.
    #[zbus(property)]
    async fn inhibitors(&self) -> Vec<IdleInhibitorRecord> {
        self.registry.lock().await.snapshot()
    }

    /// Daemon-side backend health (ScreenSaver bus availability, portal
    /// availability, logind reachability). The shell folds this with its
    /// own Wayland-side health for the popover subtitle.
    #[zbus(property)]
    async fn health(&self) -> InhibitorsHealth {
        self.health.lock().await.clone()
    }

    /// Administrative release by daemon-internal id. Used by the popover's
    /// per-row Release button. NotSupported for records where can_release
    /// is false (logind inhibitors owned by other processes — we can't
    /// release someone else's fd).
    async fn release(&self, id: u64) -> zbus::fdo::Result<()> {
        let mut reg = self.registry.lock().await;
        let record = reg.get(id).map(|r| r.record.clone());
        let Some(record) = record else {
            return Ok(());
        };
        if !record.can_release {
            return Err(zbus::fdo::Error::NotSupported(
                "logind inhibitors owned by other processes cannot be released".into(),
            ));
        }
        let released = reg.release_record(id).is_some();
        drop(reg);
        if released {
            (self.on_change)();
        }
        Ok(())
    }
}
