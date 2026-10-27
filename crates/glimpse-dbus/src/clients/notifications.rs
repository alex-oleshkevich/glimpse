use super::follower::{
    Changes, Follow, Provider, ProviderError, ProviderHandle, REASON, Snapshots, call, changes,
};
use super::{epoch, optional_clean};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zbus::proxy::CacheProperties;

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
    /// The sender's `resident` hint: invoking an action does not close the notification.
    pub resident: bool,
    /// The sender's `expire_timeout`: -1 is the server's default, 0 is never, otherwise milliseconds.
    pub expire_timeout: i32,
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
    i32,                   // expire timeout: -1 server default, 0 never, else milliseconds
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
    fn post(
        &self,
        app_name: &str,
        app_id: &str,
        icon: &str,
        summary: &str,
        body: &str,
        urgency: u32,
    ) -> zbus::Result<u32>;
    fn set_do_not_disturb(&self, enabled: bool, until: i64) -> zbus::Result<()>;
}

#[derive(Clone, PartialEq, serde::Serialize)]
pub struct NotificationsView {
    pub notifications: Vec<NotificationRecord>,
    pub do_not_disturb: DoNotDisturb,
    pub serving: bool,
    pub reason: String,
}

const APP_ID: usize = 120;
const APP_NAME: usize = 120;
const SUMMARY: usize = 200;
const ICON: usize = 200;
const IMAGE: usize = 4096;
const ACTION_LABEL: usize = 120;
const ACTION_KEY: usize = 64;
const MOST_ACTIONS: usize = 8;

pub fn decode_snapshot(snapshot: NotificationsSnapshot) -> Result<NotificationsView, String> {
    let (records, dnd, serving, reason) = snapshot;
    Ok(NotificationsView {
        notifications: records
            .into_iter()
            .filter_map(|wire| decode_notification(wire).ok())
            .collect(),
        do_not_disturb: decode_do_not_disturb(dnd)?,
        serving,
        reason: glimpse_utils::clean(&reason, REASON),
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
        expire_timeout,
    ) = wire;
    if !is_identifier(&app_id, APP_ID) {
        return Err("notification application id is empty or too long".to_owned());
    }
    Ok(NotificationRecord {
        id,
        app_id,
        app_name: glimpse_utils::clean(&app_name, APP_NAME),
        app_pid: (app_pid != 0).then_some(app_pid),
        summary: glimpse_utils::clean(&summary, SUMMARY),
        body: (!body.trim().is_empty()).then(|| glimpse_utils::markup::sanitize_body(&body)),
        icon: optional_clean(icon, ICON),
        image: optional_clean(image, IMAGE),
        urgency: decode_urgency(urgency),
        actions: actions
            .into_iter()
            .filter(|(key, _)| is_identifier(key, ACTION_KEY))
            .take(MOST_ACTIONS)
            .map(|(key, label)| NotificationAction {
                key,
                label: glimpse_utils::clean(&label, ACTION_LABEL),
            })
            .collect(),
        progress: (progress >= 0.0).then_some(progress),
        created: epoch(created)?,
        unread,
        resident,
        expire_timeout,
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

fn is_identifier(value: &str, cap: usize) -> bool {
    !value.is_empty() && value.chars().count() <= cap
}

fn decode_urgency(urgency: u8) -> NotificationUrgency {
    match urgency {
        0 => NotificationUrgency::Low,
        1 => NotificationUrgency::Normal,
        2 => NotificationUrgency::Critical,
        _ => NotificationUrgency::Unknown,
    }
}

pub fn encode_urgency(urgency: NotificationUrgency) -> u8 {
    match urgency {
        NotificationUrgency::Low => 0,
        NotificationUrgency::Critical => 2,
        NotificationUrgency::Normal | NotificationUrgency::Unknown => 1,
    }
}

pub fn urgency_from_wire(urgency: u32) -> NotificationUrgency {
    decode_urgency(u8::try_from(urgency).unwrap_or(1))
}

pub type NotificationsProvider = Provider<NotificationsProviderState>;
pub type NotificationsProviderHandle = ProviderHandle<NotificationsProviderState>;

#[derive(Clone, PartialEq)]
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

impl Follow for NotificationsProviderState {
    type Proxy = Notifications1Proxy<'static>;
    type Error = NotificationsProviderError;

    const NAME: &'static str = "notifications";

    fn lost(reason: String, _: Option<&Self>) -> Self {
        Self::unavailable(reason)
    }

    fn reason(&self) -> Option<&str> {
        self.unavailable.as_deref()
    }
}

impl Snapshots for NotificationsProviderState {
    type Snapshot = NotificationsSnapshot;

    const BUS_NAME: &'static str = GLIMPSE_NOTIFICATIONS_BUS_NAME;

    async fn build(
        connection: &zbus::Connection,
        owner: zbus::names::OwnedUniqueName,
    ) -> zbus::Result<Self::Proxy> {
        Notifications1Proxy::builder(connection)
            .destination(owner)?
            .cache_properties(CacheProperties::Yes)
            .build()
            .await
    }

    async fn read(proxy: &Self::Proxy) -> zbus::Result<Self::Snapshot> {
        match proxy.cached_snapshot()? {
            Some(snapshot) => Ok(snapshot),
            None => proxy.snapshot().await,
        }
    }

    async fn changes(proxy: &Self::Proxy) -> Changes<Self::Snapshot> {
        changes(proxy.receive_snapshot_changed().await)
    }

    fn decode(snapshot: Self::Snapshot) -> Result<Self, String> {
        Ok(Self::decoded(snapshot))
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

impl ProviderError for NotificationsProviderError {
    fn unavailable(reason: String) -> Self {
        Self::Unavailable(reason)
    }

    fn timed_out() -> Self {
        Self::TimedOut
    }

    fn call(reason: String) -> Self {
        Self::Call(reason)
    }

    fn named(name: &str, reason: String) -> Option<Self> {
        match name {
            "me.aresa.Glimpse.Notifications1.Error.InvalidAction" => {
                Some(Self::InvalidAction(reason))
            }
            "me.aresa.Glimpse.Notifications1.Error.Unavailable" => Some(Self::Unavailable(reason)),
            _ => None,
        }
    }
}

impl NotificationsProviderHandle {
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

    pub async fn post(
        &self,
        app_name: &str,
        app_id: &str,
        icon: &str,
        summary: &str,
        body: &str,
        urgency: NotificationUrgency,
    ) -> Result<u32, NotificationsProviderError> {
        let urgency = u32::from(encode_urgency(urgency));
        call(
            self.proxy()
                .await?
                .post(app_name, app_id, icon, summary, body, urgency),
        )
        .await
    }

    pub async fn set_do_not_disturb(
        &self,
        enabled: bool,
        until: i64,
    ) -> Result<(), NotificationsProviderError> {
        call(self.proxy().await?.set_do_not_disturb(enabled, until)).await
    }
}

#[cfg(test)]
mod tests;
