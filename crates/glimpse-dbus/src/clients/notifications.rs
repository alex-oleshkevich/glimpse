use super::{epoch, optional_clean};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Urgency as the freedesktop specification defines it. Internally tagged for the same reason
/// `Condition` is: a sender may send a byte outside the three the specification names, and that
/// must not make the whole payload fail to decode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "urgency", rename_all = "snake_case")]
pub enum NotificationUrgency {
    Low,
    #[default]
    Normal,
    Critical,
    #[serde(other)]
    Unknown,
}

/// One action a sender offered. `key` is what goes back to it over the bus; `label` is what the
/// reader sees, and is third-party text like every other string here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationAction {
    pub key: String,
    pub label: String,
}

pub const DEFAULT_ACTION: &str = "default";

/// One notification as the store holds it. Every text field is chosen by another application,
/// arrives over the session bus, and is capped and sanitised by the service before it gets here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationRecord {
    /// The id the specification handed the sender, and what every command names it by.
    pub id: u32,
    /// The sender's own identity: its `desktop-entry` hint where it gave one, its bus name
    /// otherwise. Grouping keys on this rather than on `app_name`, which a sender chooses freely
    /// and could therefore borrow from somebody else.
    pub app_id: String,
    pub app_name: String,
    /// The process that sent this notification, where the bus could name one, so a client can ask
    /// the compositor to raise that process's window. Raising belongs to the specification's
    /// `default` action, which means the reader activated the notification itself; a button the
    /// sender named is a command, and there only the activation token decides. A portal-relayed
    /// sender resolves to the portal rather than to the application, and then there is nothing to
    /// raise.
    pub app_pid: Option<i32>,
    pub summary: String,
    /// Sanitised Pango markup, safe to hand to `set_markup`. The freedesktop specification makes
    /// markup a server capability rather than a per-notification flag, so there is no "is this
    /// markup" boolean to carry: every body has been through `glimpse_utils::markup::sanitize_body`
    /// by the time it is here. A client that cannot render markup strips it.
    pub body: Option<String>,
    /// A themed icon name, never a path a client should go and load.
    pub icon: Option<String>,
    /// An absolute local path supplied by the sender through an image hint.
    pub image: Option<String>,
    pub urgency: NotificationUrgency,
    pub actions: Vec<NotificationAction>,
    /// The `value` hint, as a fraction. Present only when the sender sent one.
    pub progress: Option<f64>,
    pub created: DateTime<Utc>,
    pub unread: bool,
    /// The sender asked to stay until it is acted on. `resident` and a zero timeout are the two
    /// ways it can say so, and the store treats them alike.
    pub resident: bool,
}

/// Do not disturb. `until` is when it lapses on its own; `None` means it stands until the reader
/// turns it off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoNotDisturb {
    pub enabled: bool,
    pub until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationsList {
    pub notifications: Vec<NotificationRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationsDnd {
    pub dnd: DoNotDisturb,
}

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

#[derive(Clone, serde::Serialize)]
pub struct NotificationsView {
    pub notifications: Vec<NotificationRecord>,
    pub do_not_disturb: DoNotDisturb,
    pub serving: bool,
    pub reason: String,
}

const APP_ID: usize = 120;
const APP_NAME: usize = 120;
const SUMMARY: usize = 200;
const BODY: usize = 800;
const ICON: usize = 200;
const IMAGE: usize = 4096;
const ACTION_LABEL: usize = 120;
const MOST_ACTIONS: usize = 8;
const NOTIFICATION_REASON: usize = 240;

pub fn decode_snapshot(snapshot: NotificationsSnapshot) -> Result<NotificationsView, String> {
    let (records, dnd, serving, reason) = snapshot;
    Ok(NotificationsView {
        notifications: records
            .into_iter()
            .filter_map(|wire| decode_notification(wire).ok())
            .collect(),
        do_not_disturb: decode_do_not_disturb(dnd)?,
        serving,
        reason: glimpse_utils::clean(&reason, NOTIFICATION_REASON),
    })
}

fn decode_notification(wire: NotificationWire) -> Result<NotificationRecord, String> {
    let (
        id,
        app_id,
        app_name,
        app_pid,
        summary,
        body,
        icon,
        image,
        urgency,
        actions,
        progress,
        created,
        unread,
        resident,
    ) = wire;
    Ok(NotificationRecord {
        id,
        app_id: glimpse_utils::clean(&app_id, APP_ID),
        app_name: glimpse_utils::clean(&app_name, APP_NAME),
        app_pid: (app_pid != 0).then_some(app_pid),
        summary: glimpse_utils::clean(&summary, SUMMARY),
        body: optional_clean(body, BODY),
        icon: optional_clean(icon, ICON),
        image: optional_clean(image, IMAGE),
        urgency: decode_urgency(urgency),
        actions: actions
            .into_iter()
            .take(MOST_ACTIONS)
            .map(|(key, label)| NotificationAction {
                key: glimpse_utils::clean(&key, ACTION_LABEL),
                label: glimpse_utils::clean(&label, ACTION_LABEL),
            })
            .collect(),
        progress: (progress >= 0.0).then_some(progress),
        created: epoch(created)?,
        unread,
        resident,
    })
}

fn decode_do_not_disturb((enabled, until): DoNotDisturbWire) -> Result<DoNotDisturb, String> {
    Ok(DoNotDisturb {
        enabled,
        until: match until {
            0 => None,
            value => Some(epoch(value)?),
        },
    })
}

fn decode_urgency(urgency: u8) -> NotificationUrgency {
    match urgency {
        0 => NotificationUrgency::Low,
        1 => NotificationUrgency::Normal,
        2 => NotificationUrgency::Critical,
        _ => NotificationUrgency::Unknown,
    }
}

#[derive(Clone)]
pub struct NotificationsProviderState {
    pub view: Option<NotificationsView>,
    pub unavailable: Option<String>,
}

impl NotificationsProviderState {
    fn decoded(snapshot: NotificationsSnapshot) -> Self {
        match decode_snapshot(snapshot) {
            Ok(view) => Self {
                view: Some(view),
                unavailable: None,
            },
            Err(reason) => Self::unavailable(reason),
        }
    }

    fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            view: None,
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

pub struct NotificationsProvider {
    handle: NotificationsProviderHandle,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl NotificationsProvider {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        let (_, state) =
            tokio::sync::watch::channel(NotificationsProviderState::unavailable(reason));
        Self {
            handle: NotificationsProviderHandle {
                state,
                proxy: Default::default(),
            },
            task: None,
        }
    }

    pub fn start(connection: zbus::Connection) -> Self {
        let (updates, state) = tokio::sync::watch::channel(
            NotificationsProviderState::unavailable("provider has no bus owner"),
        );
        let proxy = std::sync::Arc::new(tokio::sync::RwLock::new(None));
        let task = tokio::spawn(follow_provider(connection, updates, proxy.clone()));
        Self {
            handle: NotificationsProviderHandle { state, proxy },
            task: Some(task),
        }
    }

    pub fn handle(&self) -> NotificationsProviderHandle {
        self.handle.clone()
    }

    pub async fn shutdown(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for NotificationsProvider {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

impl NotificationsProviderHandle {
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
    match tokio::time::timeout(super::DEADLINE, request).await {
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
            tracing::warn!(%error, "cannot observe the notifications provider owner");
            return;
        }
    };
    let mut owners = match dbus.receive_name_owner_changed().await {
        Ok(owners) => owners,
        Err(error) => {
            updates.send_replace(NotificationsProviderState::unavailable(error.to_string()));
            tracing::warn!(%error, "cannot observe notifications provider owner changes");
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
                tracing::debug!(
                    reason = %error,
                    "notifications provider has no owner; waiting"
                );
                if !wait_for_owner(&mut owners).await {
                    return;
                }
                continue;
            }
        };
        let mut snapshots = proxy.receive_snapshot_changed().await;
        let snapshot = match proxy.cached_snapshot() {
            Ok(Some(snapshot)) => Some(snapshot),
            Ok(None) => match proxy.snapshot().await {
                Ok(snapshot) => Some(snapshot),
                Err(error) => {
                    updates
                        .send_replace(NotificationsProviderState::unavailable(error.to_string()));
                    *current.write().await = None;
                    tracing::warn!(reason = %error, "notifications snapshot is unavailable");
                    None
                }
            },
            Err(error) => {
                updates.send_replace(NotificationsProviderState::unavailable(error.to_string()));
                *current.write().await = None;
                tracing::warn!(reason = %error, "notifications property cache is unavailable");
                None
            }
        };
        let mut serving = match snapshot {
            Some(snapshot) => {
                *current.write().await = Some(proxy.clone());
                let state = NotificationsProviderState::decoded(snapshot);
                tracing::info!(
                    serving = state.view.as_ref().is_some_and(|view| view.serving),
                    held = state
                        .view
                        .as_ref()
                        .map_or(0, |view| view.notifications.len()),
                    "notifications provider connected"
                );
                updates.send_replace(state);
                true
            }
            None => false,
        };

        loop {
            tokio::select! {
                changed = snapshots.next() => {
                    let Some(changed) = changed else {
                        break;
                    };
                    match changed.get().await {
                        Ok(snapshot) => {
                            if !serving {
                                *current.write().await = Some(proxy.clone());
                                serving = true;
                                tracing::info!(
                                    "notifications provider recovered without changing owner"
                                );
                            }
                            let state = NotificationsProviderState::decoded(snapshot);
                            tracing::debug!(
                                serving = state.view.as_ref().is_some_and(|view| view.serving),
                                held = state.view.as_ref().map_or(0, |view| view.notifications.len()),
                                "notifications snapshot changed"
                            );
                            updates.send_replace(state);
                        }
                        Err(error) => {
                            updates.send_replace(NotificationsProviderState::unavailable(error.to_string()));
                            tracing::warn!(reason = %error, "notifications snapshot change failed");
                            if serving {
                                *current.write().await = None;
                                serving = false;
                            }
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
                    // `old_owner` is what makes this a disconnect; see the weather client.
                    if args.name().as_str() != GLIMPSE_NOTIFICATIONS_BUS_NAME {
                        continue;
                    }
                    if args.old_owner().is_some() || (!serving && args.new_owner().is_some()) {
                        break;
                    }
                }
            }
        }
        if serving {
            *current.write().await = None;
            updates.send_replace(NotificationsProviderState::unavailable(
                "provider has no bus owner",
            ));
            tracing::warn!("notifications provider disconnected");
        }
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

    fn wire(summary: String, body: String, actions: Vec<(String, String)>) -> NotificationWire {
        (
            7,
            "app".to_owned(),
            "App".to_owned(),
            0,
            summary,
            body,
            String::new(),
            String::new(),
            255,
            actions,
            -1.0,
            1_234_567_890,
            true,
            false,
        )
    }

    #[test]
    fn wire_sentinels_become_absent_fields_rather_than_defaults() {
        let decoded = decode_notification(wire(
            "Summary".to_owned(),
            String::new(),
            vec![("reply".to_owned(), "Reply".to_owned())],
        ))
        .expect("the wire is readable");

        assert_eq!(decoded.app_pid, None, "pid 0 means absent");
        assert_eq!(decoded.body, None, "an empty body is absent");
        assert_eq!(decoded.icon, None);
        assert_eq!(decoded.image, None);
        assert_eq!(decoded.progress, None, "-1.0 means absent");
        assert_eq!(decoded.urgency, NotificationUrgency::Unknown);
        assert_eq!(decoded.actions[0].key, "reply");
        assert_eq!(decoded.created.timestamp_micros(), 1_234_567_890);
    }

    #[test]
    fn untrusted_text_is_bounded_before_anything_can_render_it() {
        let decoded = decode_notification(wire(
            format!("{}\u{202e}", "s".repeat(SUMMARY + 50)),
            "b".repeat(BODY + 50),
            (0..MOST_ACTIONS + 5)
                .map(|n| (format!("key{n}"), "Label".to_owned()))
                .collect(),
        ))
        .expect("the wire is readable");

        assert_eq!(decoded.summary, format!("{}…", "s".repeat(SUMMARY)));
        assert_eq!(
            decoded.body.expect("a body"),
            format!("{}…", "b".repeat(BODY))
        );
        assert_eq!(decoded.actions.len(), MOST_ACTIONS);
    }

    #[test]
    fn one_unreadable_record_costs_its_row_rather_than_the_whole_surface() {
        let readable = wire("Readable".to_owned(), String::new(), Vec::new());
        let mut unreadable = wire("Unreadable".to_owned(), String::new(), Vec::new());
        unreadable.11 = i64::MAX;

        let view = decode_snapshot((vec![unreadable, readable], (false, 0), true, String::new()))
            .expect("the snapshot is still readable");

        assert_eq!(view.notifications.len(), 1);
        assert_eq!(view.notifications[0].summary, "Readable");
        assert!(view.serving, "the provider is still serving");
    }

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
