use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue};

use glimpse_core::services::idle_inhibitor::{
    IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets, now_unix,
};

use crate::inhibitor_registry::{Registry, clamp_label};
use crate::screen_saver::Login1InhibitTaker;

pub struct PortalInhibit {
    pub registry: Arc<Mutex<Registry>>,
    pub login1_inhibit: Arc<dyn Login1InhibitTaker + Send + Sync>,
    pub on_change: Arc<dyn Fn() + Send + Sync>,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Inhibit")]
impl PortalInhibit {
    /// Per the xdg-desktop-portal backend spec, all parameters are IN and
    /// the method has no return value. Lifecycle is controlled by the
    /// `Request` object hosted at `handle`; the caller calls `Close()` on
    /// it to release the inhibition.
    async fn inhibit(
        &self,
        handle: ObjectPath<'_>,
        app_id: String,
        _window: String,
        flags: u32,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] conn: &zbus::Connection,
    ) -> zbus::fdo::Result<()> {
        let targets = InhibitionTargets::from_portal_flags(flags);
        let app_id = clamp_label(app_id);
        let reason = clamp_label(
            options
                .get("reason")
                .and_then(|v| v.try_clone().ok())
                .and_then(|v| String::try_from(v).ok())
                .unwrap_or_default(),
        );

        let logind_fd = if targets.suspend || targets.shutdown {
            let what = login1_what_for_portal(&targets);
            self.login1_inhibit
                .take(&what, &format!("Glimpse · portal: {app_id}"), &reason)
                .await
                .ok()
        } else {
            None
        };

        let id;
        {
            let mut reg = self.registry.lock().await;
            if let Err(reason) = reg.check_capacity("") {
                drop(reg);
                // `logind_fd` drops here, releasing any logind inhibit taken above.
                tracing::warn!(%app_id, reason, "rejecting portal inhibit");
                return Err(zbus::fdo::Error::LimitsExceeded(reason));
            }
            id = reg.mint_id();
            let record = IdleInhibitorRecord {
                id,
                who: app_id.clone(),
                why: reason,
                bus_name: String::new(),
                process_name: String::new(),
                source: IdleInhibitorSource::portal(app_id, handle.to_string()),
                targets,
                can_release: true,
                added_at_unix: now_unix(),
            };
            reg.insert(record, logind_fd);
        }

        let request = RequestObject {
            registry: self.registry.clone(),
            id,
            on_change: self.on_change.clone(),
        };
        let owned_handle: OwnedObjectPath = handle.to_owned().into();
        conn.object_server()
            .at(owned_handle, request)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("register Request: {e}")))?;

        (self.on_change)();
        Ok(())
    }
}

fn login1_what_for_portal(t: &InhibitionTargets) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if t.suspend {
        parts.push("sleep");
    }
    if t.shutdown {
        parts.push("shutdown");
    }
    parts.join(":")
}

/// Hosted dynamically at the `handle` path from a successful `Inhibit` call.
/// When the caller closes the Request, the portal forwards it here and we
/// release the registry record.
pub struct RequestObject {
    registry: Arc<Mutex<Registry>>,
    id: u64,
    on_change: Arc<dyn Fn() + Send + Sync>,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Request")]
impl RequestObject {
    async fn close(
        &self,
        #[zbus(object_server)] server: &zbus::ObjectServer,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<()> {
        let mut reg = self.registry.lock().await;
        reg.release_record(self.id);
        drop(reg);
        if let Some(path) = header.path() {
            let owned: OwnedObjectPath = path.to_owned().into();
            let _ = server.remove::<RequestObject, _>(owned).await;
        }
        (self.on_change)();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_string_orders_sleep_then_shutdown() {
        let t = InhibitionTargets {
            suspend: true,
            shutdown: true,
            ..InhibitionTargets::default()
        };
        assert_eq!(login1_what_for_portal(&t), "sleep:shutdown");
    }

    #[test]
    fn what_string_skips_idle_and_key_handlers() {
        let t = InhibitionTargets {
            idle: true,
            suspend: true,
            power_key: true,
            ..InhibitionTargets::default()
        };
        assert_eq!(login1_what_for_portal(&t), "sleep");
    }

    #[test]
    fn what_string_empty_when_no_suspend_or_shutdown_target() {
        let t = InhibitionTargets::idle_only();
        assert_eq!(login1_what_for_portal(&t), "");
    }
}
