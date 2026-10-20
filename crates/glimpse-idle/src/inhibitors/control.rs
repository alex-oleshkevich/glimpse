use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use glimpse_dbus::idle::{
    IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets, InhibitorsHealth, Login1Mode,
};
use glimpse_dbus::login1::{Login1ManagerProxy, current_uid};
use zbus::message::Header;

use super::{Health, SharedRegistry, WHY_CAP, unix_now};

const MAX_SLEEP_SECS: u64 = 30;

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

        let uid = current_uid();
        let added_at_unix = unix_now();
        let id = self
            .registry
            .mutate(move |registry| -> Result<u64, String> {
                registry.check_capacity(Some(&bus_name))?;
                let id = registry.mint_id();
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
                registry.insert(record, None);
                if seconds > 0 {
                    registry.set_hold_expiry(id, added_at_unix + u64::from(seconds));
                }
                Ok(id)
            })
            .await
            .map_err(zbus::fdo::Error::LimitsExceeded)?;

        let fd = match login1
            .inhibit("idle:sleep", "glimpse-idle", "Manual hold", "block")
            .await
        {
            Ok(fd) => fd,
            Err(error) => {
                self.registry
                    .mutate(move |registry| registry.release_record(id))
                    .await;
                return Err(zbus::fdo::Error::Failed(glimpse_utils::clean(
                    &error.to_string(),
                    WHY_CAP,
                )));
            }
        };
        self.registry
            .mutate(move |registry| registry.attach_fd(id, fd))
            .await;

        if seconds > 0 {
            let deadline = added_at_unix + u64::from(seconds);
            tokio::spawn(release_at(self.registry.clone(), id, deadline, unix_now));
        }

        Ok(id)
    }

    async fn release(
        &self,
        id: u64,
        #[zbus(object_server)] object_server: &zbus::ObjectServer,
    ) -> zbus::fdo::Result<()> {
        let handles = self
            .registry
            .mutate(|registry| match registry.can_release(id) {
                None => Err(zbus::fdo::Error::UnknownObject(format!(
                    "no such idle inhibitor: {id}"
                ))),
                Some(false) => Err(zbus::fdo::Error::NotSupported(
                    "this idle inhibitor is not releasable through Idle1".to_owned(),
                )),
                Some(true) => {
                    let handles = registry.portal_handle(id).into_iter().collect();
                    registry.release_record(id);
                    Ok(handles)
                }
            })
            .await?;
        super::portal::remove_requests(object_server, handles).await;
        Ok(())
    }
}

async fn release_at(registry: Arc<SharedRegistry>, id: u64, deadline: u64, now: impl Fn() -> u64) {
    loop {
        let left = deadline.saturating_sub(now());
        if left == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_secs(left.min(MAX_SLEEP_SECS))).await;
    }
    registry
        .mutate(|registry| registry.release_record(id))
        .await;
}

#[cfg(test)]
mod tests;
