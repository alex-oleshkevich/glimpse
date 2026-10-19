use std::sync::Arc;

use futures_util::StreamExt;
use glimpse_dbus::idle::{IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets};
use tokio_util::sync::CancellationToken;
use zbus::Connection;
use zbus::fdo::DBusProxy;
use zbus::message::Header;
use zbus::names::BusName;

use super::{Backend, Health, SharedRegistry, WHO_CAP, WHY_CAP, unix_now};

const BUS_NAME: &str = "org.freedesktop.ScreenSaver";
const OBJECT_PATH: &str = "/org/freedesktop/ScreenSaver";
const LEGACY_OBJECT_PATH: &str = "/ScreenSaver";

#[derive(Clone)]
pub struct ScreenSaverServer {
    registry: Arc<SharedRegistry>,
    dbus: DBusProxy<'static>,
}

impl ScreenSaverServer {
    pub fn new(registry: Arc<SharedRegistry>, dbus: DBusProxy<'static>) -> Self {
        Self { registry, dbus }
    }
}

#[zbus::interface(name = "org.freedesktop.ScreenSaver")]
impl ScreenSaverServer {
    async fn inhibit(
        &self,
        application_name: String,
        reason_for_inhibit: String,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<u32> {
        let application_name = glimpse_utils::clean(&application_name, WHO_CAP);
        let reason_for_inhibit = glimpse_utils::clean(&reason_for_inhibit, WHY_CAP);
        let bus_name = header
            .sender()
            .map(|name| name.as_str().to_owned())
            .unwrap_or_default();

        let bus_name_for_record = bus_name.clone();
        let added_at_unix = unix_now();
        let (id, cookie) = self
            .registry
            .mutate(move |registry| -> Result<(u64, u32), String> {
                registry.check_capacity(Some(&bus_name_for_record))?;
                let id = registry.mint_id();
                let cookie = registry.mint_cookie();
                let record = IdleInhibitorRecord {
                    id,
                    who: application_name,
                    why: reason_for_inhibit,
                    bus_name: bus_name_for_record,
                    process_name: String::new(),
                    source: IdleInhibitorSource::screen_saver(cookie),
                    targets: InhibitionTargets::idle_only(),
                    can_release: true,
                    added_at_unix,
                };
                registry.insert(record, None);
                Ok((id, cookie))
            })
            .await
            .inspect_err(|reason| {
                tracing::warn!(%bus_name, reason, "rejecting ScreenSaver Inhibit");
            })
            .map_err(zbus::fdo::Error::LimitsExceeded)?;

        if !bus_name.is_empty() {
            tokio::spawn(backfill_process_name(
                self.registry.clone(),
                self.dbus.clone(),
                bus_name,
                id,
            ));
        }

        Ok(cookie)
    }

    async fn un_inhibit(
        &self,
        cookie: u32,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        let bus_name = header
            .sender()
            .map(|name| name.as_str().to_owned())
            .unwrap_or_default();
        let bus_name_for_request = bus_name.clone();
        self.registry
            .mutate(move |registry| -> Result<(), String> {
                match registry.lookup_by_cookie(cookie) {
                    Some(id) if registry.record_is_owned_by(id, &bus_name_for_request) => {
                        registry.release_record(id);
                    }
                    Some(_) => {
                        tracing::debug!(cookie, bus_name = %bus_name_for_request, "UnInhibit for another client's cookie")
                    }
                    None => tracing::debug!(cookie, "UnInhibit for an unknown cookie"),
                }
                Ok(())
            })
            .await
            .inspect_err(|reason| {
                tracing::warn!(%bus_name, reason, "rejecting ScreenSaver UnInhibit");
            })
            .map_err(zbus::fdo::Error::LimitsExceeded)
    }
}

async fn backfill_process_name(
    registry: Arc<SharedRegistry>,
    dbus: DBusProxy<'static>,
    bus_name: String,
    id: u64,
) {
    let Ok(name) = BusName::try_from(bus_name.as_str()) else {
        return;
    };
    let Ok(pid) = dbus.get_connection_unix_process_id(name).await else {
        return;
    };
    let Ok(comm) = tokio::fs::read_to_string(format!("/proc/{pid}/comm")).await else {
        return;
    };
    let process_name = glimpse_utils::clean(&comm, WHO_CAP);
    registry
        .mutate(move |registry| registry.set_process_name(id, process_name))
        .await;
}

pub async fn start(connection: Connection, registry: Arc<SharedRegistry>, health: Health) {
    let dbus = match DBusProxy::new(&connection).await {
        Ok(dbus) => dbus,
        Err(error) => {
            tracing::warn!(%error, "cannot reach org.freedesktop.DBus; ScreenSaver will not start");
            health.degraded(Backend::ScreenSaver, "cannot reach org.freedesktop.DBus");
            return;
        }
    };

    let server = ScreenSaverServer::new(registry, dbus);
    for path in [OBJECT_PATH, LEGACY_OBJECT_PATH] {
        if let Err(error) = connection.object_server().at(path, server.clone()).await {
            tracing::warn!(%error, path, "failed to register org.freedesktop.ScreenSaver object");
        }
    }

    match glimpse_dbus::own_name(&connection, BUS_NAME).await {
        Ok(()) => {
            tracing::info!(bus_name = BUS_NAME, "acquired org.freedesktop.ScreenSaver");
            health.ready(Backend::ScreenSaver);
        }
        Err(error) => {
            tracing::warn!(
                %error,
                "org.freedesktop.ScreenSaver already owned; running in degraded mode"
            );
            health.degraded(Backend::ScreenSaver, "Bus name already owned");
        }
    }
}

pub async fn watch_disconnects(
    connection: Connection,
    registry: Arc<SharedRegistry>,
    cancel: CancellationToken,
) {
    let dbus = match DBusProxy::new(&connection).await {
        Ok(dbus) => dbus,
        Err(error) => {
            tracing::warn!(
                %error,
                "cannot observe NameOwnerChanged; disconnected inhibitors will not auto-release"
            );
            return;
        }
    };
    let mut owners = match dbus.receive_name_owner_changed().await {
        Ok(owners) => owners,
        Err(error) => {
            tracing::warn!(%error, "cannot subscribe to NameOwnerChanged");
            return;
        }
    };

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            signal = owners.next() => match signal {
                Some(signal) => {
                    let Ok(args) = signal.args() else { continue };
                    if args.old_owner().is_some() && args.new_owner().is_none() {
                        let name = args.name().to_string();
                        let handles = registry
                            .mutate(move |registry| {
                                let handles = registry.portal_handles_of(&name);
                                registry.release_by_bus_name(&name);
                                handles
                            })
                            .await;
                        super::portal::remove_requests(connection.object_server(), handles).await;
                    }
                }
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests;
