pub const GLIMPSE_NOTIFICATIONS_BUS_NAME: &str = "me.aresa.Glimpse.Notifications";
pub const GLIMPSE_NOTIFICATIONS_OBJECT_PATH: &str = "/me/aresa/Glimpse/Notifications";

pub type NotificationWire = (
    u32,                   // notification ID
    String,                // application ID
    String,                // application display name
    i32,                   // application process ID, or 0 when absent
    String,                // summary
    String,                // body, or empty when absent
    String,                // icon, or empty when absent
    String,                // image, or empty when absent
    u8,                    // urgency: 0 low, 1 normal, 2 critical, 255 unknown
    Vec<(String, String)>, // action key and label pairs
    f64,                   // progress, or -1.0 when absent
    i64,                   // creation time in Unix microseconds
    bool,                  // unread
    bool,                  // resident
);

pub type DoNotDisturbWire = (bool, i64);

pub type NotificationsSnapshot = (Vec<NotificationWire>, DoNotDisturbWire, bool, String);

#[zbus::proxy(
    interface = "me.aresa.Glimpse.Notifications1",
    default_service = "me.aresa.Glimpse.Notifications",
    default_path = "/me/aresa/Glimpse/Notifications"
)]
pub trait Notifications1 {
    #[zbus(property)]
    fn snapshot(&self) -> zbus::Result<NotificationsSnapshot>;

    fn dismiss(&self, id: u32) -> zbus::Result<()>;
    fn remove(&self, id: u32) -> zbus::Result<()>;
    fn activate(&self, id: u32, activation_token: &str) -> zbus::Result<()>;
    fn invoke_action(&self, id: u32, action_key: &str, activation_token: &str) -> zbus::Result<()>;
    fn clear_application(&self, application_id: &str) -> zbus::Result<()>;
    fn clear_all(&self) -> zbus::Result<()>;
    fn set_do_not_disturb(&self, enabled: bool, until: i64) -> zbus::Result<()>;
}

#[derive(Clone)]
pub struct NotificationsProviderState {
    pub snapshot: Option<NotificationsSnapshot>,
    pub unavailable: Option<String>,
}

impl NotificationsProviderState {
    fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            snapshot: None,
            unavailable: Some(reason.into()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NotificationsProviderError {
    #[error("notification action is invalid: {0}")]
    InvalidAction(String),
    #[error("notification provider unavailable: {0}")]
    Unavailable(String),
    #[error("notification provider call timed out")]
    TimedOut,
    #[error("notification provider call failed: {0}")]
    Call(String),
}

#[derive(Clone)]
pub struct NotificationsProviderHandle {
    state: tokio::sync::watch::Receiver<NotificationsProviderState>,
    proxy: std::sync::Arc<tokio::sync::RwLock<Option<Notifications1Proxy<'static>>>>,
}

impl NotificationsProviderHandle {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        let (_, state) =
            tokio::sync::watch::channel(NotificationsProviderState::unavailable(reason));
        Self {
            state,
            proxy: Default::default(),
        }
    }

    pub fn start(connection: zbus::Connection) -> (Self, tokio::task::JoinHandle<()>) {
        let (updates, state) = tokio::sync::watch::channel(
            NotificationsProviderState::unavailable("provider has no bus owner"),
        );
        let proxy = std::sync::Arc::new(tokio::sync::RwLock::new(None));
        let task = tokio::spawn(follow_provider(connection, updates, proxy.clone()));
        (Self { state, proxy }, task)
    }

    pub fn snapshot(&self) -> NotificationsProviderState {
        self.state.borrow().clone()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<NotificationsProviderState> {
        self.state.clone()
    }

    async fn proxy(&self) -> Result<Notifications1Proxy<'static>, NotificationsProviderError> {
        self.proxy.read().await.clone().ok_or_else(|| {
            NotificationsProviderError::Unavailable(
                self.state
                    .borrow()
                    .unavailable
                    .clone()
                    .unwrap_or_else(|| "provider has no bus owner".to_owned()),
            )
        })
    }

    pub async fn dismiss(&self, id: u32) -> Result<(), NotificationsProviderError> {
        call(self.proxy().await?.dismiss(id)).await
    }

    pub async fn remove(&self, id: u32) -> Result<(), NotificationsProviderError> {
        call(self.proxy().await?.remove(id)).await
    }

    pub async fn activate(
        &self,
        id: u32,
        activation_token: Option<String>,
    ) -> Result<(), NotificationsProviderError> {
        call(
            self.proxy()
                .await?
                .activate(id, activation_token.as_deref().unwrap_or_default()),
        )
        .await
    }

    pub async fn invoke_action(
        &self,
        id: u32,
        action_key: String,
        activation_token: Option<String>,
    ) -> Result<(), NotificationsProviderError> {
        call(self.proxy().await?.invoke_action(
            id,
            &action_key,
            activation_token.as_deref().unwrap_or_default(),
        ))
        .await
    }

    pub async fn clear_application(
        &self,
        application_id: String,
    ) -> Result<(), NotificationsProviderError> {
        call(self.proxy().await?.clear_application(&application_id)).await
    }

    pub async fn clear_all(&self) -> Result<(), NotificationsProviderError> {
        call(self.proxy().await?.clear_all()).await
    }

    pub async fn set_do_not_disturb(
        &self,
        enabled: bool,
        until: i64,
    ) -> Result<(), NotificationsProviderError> {
        call(self.proxy().await?.set_do_not_disturb(enabled, until)).await
    }
}

async fn call(
    request: impl std::future::Future<Output = zbus::Result<()>>,
) -> Result<(), NotificationsProviderError> {
    match tokio::time::timeout(std::time::Duration::from_secs(5), request).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(zbus::Error::MethodError(name, reason, _)))
            if name.as_str() == "me.aresa.Glimpse.Notifications1.Error.InvalidAction" =>
        {
            Err(NotificationsProviderError::InvalidAction(
                reason.unwrap_or_default(),
            ))
        }
        Ok(Err(zbus::Error::MethodError(name, reason, _)))
            if name.as_str() == "me.aresa.Glimpse.Notifications1.Error.Unavailable" =>
        {
            Err(NotificationsProviderError::Unavailable(
                reason.unwrap_or_default(),
            ))
        }
        Ok(Err(error)) => Err(NotificationsProviderError::Call(error.to_string())),
        Err(_) => Err(NotificationsProviderError::TimedOut),
    }
}

async fn follow_provider(
    connection: zbus::Connection,
    updates: tokio::sync::watch::Sender<NotificationsProviderState>,
    current: std::sync::Arc<tokio::sync::RwLock<Option<Notifications1Proxy<'static>>>>,
) {
    use futures_util::StreamExt;
    use zbus::proxy::CacheProperties;

    let dbus = match zbus::fdo::DBusProxy::new(&connection).await {
        Ok(dbus) => dbus,
        Err(error) => {
            updates.send_replace(NotificationsProviderState::unavailable(error.to_string()));
            return;
        }
    };
    let mut owners = match dbus.receive_name_owner_changed().await {
        Ok(owners) => owners,
        Err(error) => {
            updates.send_replace(NotificationsProviderState::unavailable(error.to_string()));
            return;
        }
    };

    loop {
        let proxy = match Notifications1Proxy::builder(&connection)
            .cache_properties(CacheProperties::Yes)
            .build()
            .await
        {
            Ok(proxy) => proxy,
            Err(error) => {
                *current.write().await = None;
                updates.send_replace(NotificationsProviderState::unavailable(error.to_string()));
                if !wait_for_owner(&mut owners).await {
                    return;
                }
                continue;
            }
        };
        let mut snapshots = proxy.receive_snapshot_changed().await;
        let snapshot = match proxy.cached_snapshot() {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => match proxy.snapshot().await {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    updates
                        .send_replace(NotificationsProviderState::unavailable(error.to_string()));
                    *current.write().await = None;
                    if !wait_for_owner(&mut owners).await {
                        return;
                    }
                    continue;
                }
            },
            Err(error) => {
                updates.send_replace(NotificationsProviderState::unavailable(error.to_string()));
                *current.write().await = None;
                if !wait_for_owner(&mut owners).await {
                    return;
                }
                continue;
            }
        };
        *current.write().await = Some(proxy.clone());
        updates.send_replace(NotificationsProviderState {
            snapshot: Some(snapshot),
            unavailable: None,
        });

        loop {
            tokio::select! {
                changed = snapshots.next() => {
                    let Some(changed) = changed else {
                        break;
                    };
                    match changed.get().await {
                        Ok(snapshot) => {
                            updates.send_replace(NotificationsProviderState {
                                snapshot: Some(snapshot),
                                unavailable: None,
                            });
                        }
                        Err(error) => {
                            updates.send_replace(NotificationsProviderState::unavailable(error.to_string()));
                            break;
                        }
                    }
                }
                owner = owners.next() => {
                    let Some(owner) = owner else {
                        break;
                    };
                    let Ok(args) = owner.args() else {
                        continue;
                    };
                    if args.name().as_str() == GLIMPSE_NOTIFICATIONS_BUS_NAME {
                        break;
                    }
                }
            }
        }
        *current.write().await = None;
        updates.send_replace(NotificationsProviderState::unavailable(
            "provider has no bus owner",
        ));
    }
}

async fn wait_for_owner(owners: &mut zbus::fdo::NameOwnerChangedStream) -> bool {
    use futures_util::StreamExt;

    while let Some(owner) = owners.next().await {
        let Ok(args) = owner.args() else {
            continue;
        };
        if args.name().as_str() == GLIMPSE_NOTIFICATIONS_BUS_NAME && args.new_owner().is_some() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Type;

    #[test]
    fn wire_signatures_match_the_versioned_contract() {
        assert_eq!(NotificationWire::SIGNATURE, "(ussissssya(ss)dxbb)");
        assert_eq!(DoNotDisturbWire::SIGNATURE, "(bx)");
        assert_eq!(
            NotificationsSnapshot::SIGNATURE,
            "(a(ussissssya(ss)dxbb)(bx)bs)"
        );
    }
}
