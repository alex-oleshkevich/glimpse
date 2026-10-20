use std::collections::HashMap;
use std::sync::Arc;

use glimpse_dbus::idle::{IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets};
use glimpse_dbus::login1::Login1ManagerProxy;
use zbus::Connection;
use zbus::message::Header;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue};

use super::{Backend, Health, SharedRegistry, WHO_CAP, WHY_CAP, unix_now};

const BUS_NAME: &str = "me.aresa.Glimpse.Idle.Portal";
const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_RESPONSE_SUCCESS: u32 = 0;
const PORTAL_RESPONSE_OTHER: u32 = 2;
const MAX_HANDLE_LEN: usize = 255;
const SESSION_RUNNING: u32 = 1;

pub struct PortalInhibit {
    registry: Arc<SharedRegistry>,
    login1: Option<Login1ManagerProxy<'static>>,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Inhibit")]
impl PortalInhibit {
    #[allow(
        clippy::too_many_arguments,
        reason = "five wire arguments plus the connection and header the zbus macro injects"
    )]
    async fn inhibit(
        &self,
        handle: ObjectPath<'_>,
        app_id: String,
        _window: String,
        flags: u32,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        if handle.as_str().len() > MAX_HANDLE_LEN {
            return Err(zbus::fdo::Error::LimitsExceeded(format!(
                "portal request handle is longer than {MAX_HANDLE_LEN} bytes"
            )));
        }
        let sender = header
            .sender()
            .map(|name| name.as_str().to_owned())
            .unwrap_or_default();
        let app_id = glimpse_utils::clean(&app_id, WHO_CAP);
        let targets = InhibitionTargets::from_portal_flags(flags);
        let reason = options
            .get("reason")
            .and_then(|value| String::try_from(value.clone()).ok())
            .map(|reason| glimpse_utils::clean(&reason, WHY_CAP))
            .unwrap_or_default();

        let what = login1_what(&targets);
        let handle_string = handle.to_string();
        let added_at_unix = unix_now();
        let id = self
            .registry
            .mutate({
                let app_id = app_id.clone();
                let reason = reason.clone();
                move |registry| -> Result<u64, String> {
                    registry.check_capacity(Some(&app_id))?;
                    let id = registry.mint_id();
                    let record = IdleInhibitorRecord {
                        id,
                        who: hold_label(&app_id, "glimpse-idle").to_owned(),
                        why: reason,
                        bus_name: sender,
                        process_name: String::new(),
                        source: IdleInhibitorSource::portal(handle_string, app_id),
                        targets,
                        can_release: true,
                        added_at_unix,
                    };
                    registry.insert(record, None);
                    Ok(id)
                }
            })
            .await
            .inspect_err(|reason| {
                tracing::warn!(%app_id, reason, "rejecting portal Inhibit");
            })
            .map_err(zbus::fdo::Error::LimitsExceeded)?;

        let owned_handle: OwnedObjectPath = handle.to_owned().into();
        let request = PortalRequest {
            registry: self.registry.clone(),
            id,
            handle: owned_handle.clone(),
        };
        match conn.object_server().at(owned_handle.clone(), request).await {
            Ok(true) => {
                if self
                    .registry
                    .read(|registry| registry.can_release(id).is_none())
                    .await
                {
                    remove_requests(conn.object_server(), vec![owned_handle.to_string()]).await;
                    return Ok(());
                }
            }
            other => {
                tracing::warn!(
                    ?other,
                    handle = %owned_handle,
                    "portal Request path already taken; releasing the record it would have \
                     controlled"
                );
                self.registry
                    .mutate(move |registry| registry.release_record(id))
                    .await;
                return Ok(());
            }
        }

        let Some(login1) = self.login1.as_ref().filter(|_| !what.is_empty()) else {
            return Ok(());
        };
        match login1
            .inhibit(
                &what,
                hold_label(&app_id, "glimpse-idle"),
                hold_label(&reason, "Portal inhibit"),
                "block",
            )
            .await
        {
            Ok(fd) => {
                self.registry
                    .mutate(move |registry| registry.attach_fd(id, fd))
                    .await;
            }
            Err(error) => tracing::warn!(
                %error,
                %app_id,
                "portal inhibit could not take a login1 hold; tracking without one"
            ),
        }

        Ok(())
    }

    async fn create_monitor(
        &self,
        _handle: ObjectPath<'_>,
        session_handle: ObjectPath<'_>,
        app_id: String,
        _window: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) -> zbus::fdo::Result<u32> {
        let owned: OwnedObjectPath = session_handle.to_owned().into();
        let session = PortalSession {
            handle: owned.clone(),
        };
        match conn.object_server().at(owned.clone(), session).await {
            Ok(true) => {}
            other => {
                tracing::warn!(?other, session = %owned, "portal monitor session path already taken");
                return Ok(PORTAL_RESPONSE_OTHER);
            }
        }
        tracing::info!(%app_id, session = %owned, "portal monitor session created");

        let _ = emitter.state_changed(owned.as_ref(), running_state()).await;
        Ok(PORTAL_RESPONSE_SUCCESS)
    }

    async fn query_end_response(&self, session_handle: ObjectPath<'_>) {
        tracing::debug!(session = %session_handle, "portal query-end acknowledged");
    }

    #[zbus(signal)]
    async fn state_changed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        session_handle: ObjectPath<'_>,
        state: HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;
}

fn running_state() -> HashMap<String, OwnedValue> {
    let mut state = HashMap::new();
    if let Ok(running) = OwnedValue::try_from(zbus::zvariant::Value::from(SESSION_RUNNING)) {
        state.insert("session-state".to_owned(), running);
    }
    state
}

struct PortalSession {
    handle: OwnedObjectPath,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Session")]
impl PortalSession {
    async fn close(&self, #[zbus(object_server)] object_server: &zbus::ObjectServer) {
        let _ = object_server
            .remove::<PortalSession, _>(self.handle.clone())
            .await;
        tracing::debug!(session = %self.handle, "portal monitor session closed");
    }

    #[zbus(signal)]
    async fn closed(emitter: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;
}

struct PortalRequest {
    registry: Arc<SharedRegistry>,
    id: u64,
    handle: OwnedObjectPath,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Request")]
impl PortalRequest {
    async fn close(
        &self,
        #[zbus(object_server)] object_server: &zbus::ObjectServer,
    ) -> zbus::fdo::Result<()> {
        self.registry
            .mutate(|registry| registry.release_record(self.id))
            .await;
        let _ = object_server
            .remove::<PortalRequest, _>(self.handle.clone())
            .await;
        Ok(())
    }
}

pub async fn remove_requests(object_server: &zbus::ObjectServer, handles: Vec<String>) {
    for handle in handles {
        if let Ok(path) = OwnedObjectPath::try_from(handle) {
            let _ = object_server.remove::<PortalRequest, _>(path).await;
        }
    }
}

fn hold_label<'a>(given: &'a str, fallback: &'a str) -> &'a str {
    if given.is_empty() { fallback } else { given }
}

fn login1_what(targets: &InhibitionTargets) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if targets.suspend {
        parts.push("sleep");
    }
    if targets.shutdown {
        parts.push("shutdown");
    }
    parts.join(":")
}

pub async fn start(
    connection: Connection,
    registry: Arc<SharedRegistry>,
    login1: Option<Login1ManagerProxy<'static>>,
    health: Health,
) {
    let server = PortalInhibit { registry, login1 };
    if let Err(error) = connection.object_server().at(OBJECT_PATH, server).await {
        tracing::warn!(%error, "failed to register org.freedesktop.impl.portal.Inhibit object");
    }

    match glimpse_dbus::own_name(&connection, BUS_NAME).await {
        Ok(()) => {
            tracing::info!(bus_name = BUS_NAME, "acquired me.aresa.Glimpse.Idle.Portal");
            health.ready(Backend::Portal);
        }
        Err(error) => {
            tracing::warn!(
                %error,
                "me.aresa.Glimpse.Idle.Portal already owned; running in degraded mode"
            );
            health
                .name_taken(&connection, Backend::Portal, BUS_NAME)
                .await;
        }
    }
}

#[cfg(test)]
mod tests;
