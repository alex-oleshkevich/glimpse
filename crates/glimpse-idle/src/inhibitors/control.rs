use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use glimpse_dbus::idle::{
    IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets, InhibitorsHealth, Login1Mode,
};
use glimpse_dbus::login1::{Login1ManagerProxy, current_uid};
use zbus::message::Header;

use super::{Health, SharedRegistry, unix_now};

const REASON_CAP: usize = 240;

pub struct Idle1Server {
    registry: Arc<SharedRegistry>,
    login1: Option<Login1ManagerProxy<'static>>,
    health: Health,
}

impl Idle1Server {
    pub fn new(
        registry: Arc<SharedRegistry>,
        login1: Option<Login1ManagerProxy<'static>>,
        health: Health,
    ) -> Self {
        Self {
            registry,
            login1,
            health,
        }
    }
}

#[zbus::interface(name = "me.aresa.Glimpse.Idle1")]
impl Idle1Server {
    #[zbus(property)]
    async fn inhibitors(&self) -> Vec<IdleInhibitorRecord> {
        self.registry.read(|registry| registry.snapshot()).await
    }

    #[zbus(property)]
    async fn hold_expiries(&self) -> HashMap<u64, u64> {
        self.registry
            .read(|registry| registry.hold_expiries())
            .await
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn health(&self) -> InhibitorsHealth {
        self.health.snapshot()
    }

    async fn hold(
        &self,
        seconds: u32,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<u64> {
        let Some(login1) = self.login1.as_ref() else {
            return Err(zbus::fdo::Error::NotSupported(
                "Idle1.Hold needs org.freedesktop.login1, which is unavailable on this system"
                    .to_owned(),
            ));
        };

        let bus_name = header
            .sender()
            .map(|name| name.as_str().to_owned())
            .unwrap_or_default();

        let bus_name_for_check = bus_name.clone();
        let id = self
            .registry
            .mutate(move |registry| -> Result<u64, String> {
                registry.check_capacity(Some(&bus_name_for_check))?;
                Ok(registry.mint_id())
            })
            .await
            .map_err(zbus::fdo::Error::LimitsExceeded)?;

        let fd = login1
            .inhibit("idle:sleep", "glimpse-idle", "Manual hold", "block")
            .await
            .map_err(|error| {
                zbus::fdo::Error::Failed(glimpse_utils::clean(&error.to_string(), REASON_CAP))
            })?;

        let uid = current_uid_or_warn();
        let added_at_unix = unix_now();
        self.registry
            .mutate(move |registry| -> Result<(), String> {
                registry.check_capacity(Some(&bus_name))?;
                let record = IdleInhibitorRecord {
                    id,
                    who: "glimpse-idle".to_owned(),
                    why: "Manual hold".to_owned(),
                    bus_name,
                    process_name: String::new(),
                    source: IdleInhibitorSource::login1(std::process::id(), uid, Login1Mode::Block),
                    targets: InhibitionTargets::manual_hold(),
                    can_release: true,
                    added_at_unix,
                };
                registry.insert(record, Some(fd));
                if seconds > 0 {
                    registry.set_hold_expiry(id, added_at_unix + u64::from(seconds));
                }
                Ok(())
            })
            .await
            .map_err(zbus::fdo::Error::LimitsExceeded)?;

        if seconds > 0 {
            spawn_auto_release(self.registry.clone(), id, seconds);
        }

        Ok(id)
    }

    async fn release(&self, id: u64) -> zbus::fdo::Result<()> {
        self.registry
            .mutate(|registry| match registry.can_release(id) {
                None => Err(zbus::fdo::Error::UnknownObject(format!(
                    "no such idle inhibitor: {id}"
                ))),
                Some(false) => Err(zbus::fdo::Error::NotSupported(
                    "this idle inhibitor is not releasable through Idle1".to_owned(),
                )),
                Some(true) => {
                    registry.release_record(id);
                    Ok(())
                }
            })
            .await
    }
}

fn spawn_auto_release(registry: Arc<SharedRegistry>, id: u64, seconds: u32) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(u64::from(seconds))).await;
        registry
            .mutate(|registry| registry.release_record(id))
            .await;
    });
}

fn current_uid_or_warn() -> u32 {
    match current_uid() {
        Ok(uid) => uid,
        Err(error) => {
            tracing::warn!(%error, "cannot read the current uid; recording 0");
            0
        }
    }
}

#[cfg(test)]
mod tests;
