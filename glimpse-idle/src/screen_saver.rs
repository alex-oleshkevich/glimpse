use std::os::fd::OwnedFd;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use glimpse_core::services::idle_inhibitor::InhibitionTargets;

use crate::inhibitor_registry::{Registry, build_screen_saver_record};

pub const MANUAL_HOLD_WHO: &str = "Glimpse";
pub const MANUAL_HOLD_WHY: &str = "Manual hold";

pub struct ScreenSaver {
    pub registry: Arc<Mutex<Registry>>,
    pub login1_inhibit: Arc<dyn Login1InhibitTaker + Send + Sync>,
    pub on_change: Arc<dyn Fn() + Send + Sync>,
}

/// Abstraction so unit tests run without a live logind connection.
#[async_trait]
pub trait Login1InhibitTaker {
    async fn take(&self, what: &str, who: &str, why: &str) -> Result<OwnedFd, zbus::Error>;
}

#[zbus::interface(name = "org.freedesktop.ScreenSaver")]
impl ScreenSaver {
    async fn inhibit(
        &self,
        application_name: String,
        reason_for_inhibit: String,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(connection)] conn: &zbus::Connection,
    ) -> zbus::fdo::Result<u32> {
        let bus_name = header.sender().map(|s| s.to_string()).unwrap_or_default();
        let cookie = inhibit_impl(
            &self.registry,
            self.login1_inhibit.as_ref(),
            application_name,
            reason_for_inhibit,
            bus_name.clone(),
            self.on_change.as_ref(),
        )
        .await?;

        if !bus_name.is_empty() {
            let registry = self.registry.clone();
            let on_change = self.on_change.clone();
            let conn = conn.clone();
            tokio::spawn(async move {
                if let Some(name) =
                    crate::dbus_helpers::resolve_process_name(&conn, &bus_name).await
                {
                    let mut reg = registry.lock().await;
                    if let Some(id) = reg.lookup_by_cookie(cookie) {
                        if let Some(internal) = reg.records_mut().get_mut(&id) {
                            internal.record.process_name = name;
                        }
                    }
                    drop(reg);
                    on_change();
                }
            });
        }

        Ok(cookie)
    }

    async fn un_inhibit(&self, cookie: u32) -> zbus::fdo::Result<()> {
        let mut reg = self.registry.lock().await;
        if let Some(id) = reg.lookup_by_cookie(cookie) {
            reg.release_record(id);
            drop(reg);
            (self.on_change)();
        } else {
            tracing::debug!(cookie, "UnInhibit for unknown cookie — already released");
        }
        Ok(())
    }
}

pub(crate) async fn inhibit_impl(
    registry: &Mutex<Registry>,
    login1: &(dyn Login1InhibitTaker + Send + Sync),
    application_name: String,
    reason_for_inhibit: String,
    bus_name: String,
    on_change: &(dyn Fn() + Send + Sync),
) -> zbus::fdo::Result<u32> {
    let manual_hold =
        application_name == MANUAL_HOLD_WHO && reason_for_inhibit == MANUAL_HOLD_WHY;
    let targets = if manual_hold {
        InhibitionTargets::manual_hold()
    } else {
        InhibitionTargets::idle_only()
    };

    let logind_fd = if targets.suspend {
        match login1
            .take("idle:sleep", "Glimpse · Manual hold", &reason_for_inhibit)
            .await
        {
            Ok(fd) => Some(fd),
            Err(e) => {
                tracing::warn!(error = ?e, "manual hold: failed to take logind inhibit fd");
                None
            }
        }
    } else {
        None
    };

    let mut reg = registry.lock().await;
    let id = reg.mint_id();
    let cookie = reg.mint_cookie();
    let record = build_screen_saver_record(
        id,
        cookie,
        application_name,
        reason_for_inhibit,
        bus_name,
        targets,
    );
    reg.insert(record, logind_fd);
    drop(reg);
    on_change();
    Ok(cookie)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    struct FakeLogin1 {
        calls: StdMutex<Vec<(String, String, String)>>,
    }

    #[async_trait]
    impl Login1InhibitTaker for FakeLogin1 {
        async fn take(&self, what: &str, who: &str, why: &str) -> Result<OwnedFd, zbus::Error> {
            self.calls
                .lock()
                .unwrap()
                .push((what.into(), who.into(), why.into()));
            let file = std::fs::File::open("/dev/null").expect("open /dev/null");
            Ok(OwnedFd::from(file))
        }
    }

    #[tokio::test]
    async fn manual_hold_triggers_logind_inhibit() {
        let registry = Arc::new(Mutex::new(Registry::new()));
        let fake = Arc::new(FakeLogin1 {
            calls: StdMutex::new(Vec::new()),
        });
        let cookie = inhibit_impl(
            &registry,
            fake.as_ref(),
            MANUAL_HOLD_WHO.into(),
            MANUAL_HOLD_WHY.into(),
            ":1.42".into(),
            &|| (),
        )
        .await
        .unwrap();
        assert!(cookie >= 1);
        let calls = fake.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "idle:sleep");
        drop(calls);

        let reg = registry.lock().await;
        let snap = reg.snapshot();
        assert_eq!(snap.len(), 1);
        assert!(snap[0].targets.idle && snap[0].targets.suspend);
    }

    #[tokio::test]
    async fn external_inhibit_does_not_take_logind_fd() {
        let registry = Arc::new(Mutex::new(Registry::new()));
        let fake = Arc::new(FakeLogin1 {
            calls: StdMutex::new(Vec::new()),
        });
        inhibit_impl(
            &registry,
            fake.as_ref(),
            "Firefox".into(),
            "Playing video".into(),
            ":1.99".into(),
            &|| (),
        )
        .await
        .unwrap();
        assert_eq!(fake.calls.lock().unwrap().len(), 0);
        let snap = registry.lock().await.snapshot();
        assert!(snap[0].targets.idle && !snap[0].targets.suspend);
    }

    #[tokio::test]
    async fn glimpse_who_with_different_why_stays_idle_only() {
        let registry = Arc::new(Mutex::new(Registry::new()));
        let fake = Arc::new(FakeLogin1 {
            calls: StdMutex::new(Vec::new()),
        });
        inhibit_impl(
            &registry,
            fake.as_ref(),
            "Glimpse".into(),
            "Something else".into(),
            ":1.7".into(),
            &|| (),
        )
        .await
        .unwrap();
        assert_eq!(fake.calls.lock().unwrap().len(), 0);
        let snap = registry.lock().await.snapshot();
        assert!(snap[0].targets.idle && !snap[0].targets.suspend);
    }
}
