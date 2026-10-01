use chrono::DateTime;
use glimpse_dbus::notifications::{DoNotDisturb, NotificationRecord, NotificationUrgency};
use glimpse_dbus::notifications::{
    DoNotDisturbWire, GLIMPSE_NOTIFICATIONS_BUS_NAME, GLIMPSE_NOTIFICATIONS_OBJECT_PATH,
    NotificationWire, NotificationsSnapshot,
};
use glimpse_dbus::{Exported, Snapshot};
use glimpse_services::{CommandError, Incoming, NotificationsHandle};
use zbus::{Connection, DBusError};

#[derive(Debug, DBusError)]
#[zbus(prefix = "me.aresa.Glimpse.Notifications1.Error", impl_display = true)]
pub enum Error {
    InvalidAction(String),
    Unavailable(String),
    #[zbus(error)]
    ZBus(zbus::Error),
}

pub(crate) struct Provider {
    notifications: NotificationsHandle,
}

impl Snapshot for Provider {
    async fn emit_snapshot_changed(
        &self,
        emitter: &zbus::object_server::SignalEmitter<'_>,
    ) -> zbus::Result<()> {
        self.snapshot_changed(emitter).await
    }
}

#[zbus::interface(name = "me.aresa.Glimpse.Notifications1")]
impl Provider {
    #[zbus(property)]
    fn snapshot(&self) -> NotificationsSnapshot {
        snapshot(&self.notifications)
    }

    async fn dismiss(&self, id: u32) -> Result<(), Error> {
        self.notifications.dismiss(id).await.map_err(Error::from)
    }

    async fn remove(&self, id: u32) -> Result<(), Error> {
        self.notifications.remove(id).await.map_err(Error::from)
    }

    async fn activate(&self, id: u32, activation_token: &str) -> Result<(), Error> {
        self.notifications
            .activate(id, optional(activation_token))
            .await
            .map_err(Error::from)
    }

    async fn invoke_action(
        &self,
        id: u32,
        action_key: &str,
        activation_token: &str,
    ) -> Result<(), Error> {
        self.notifications
            .invoke_action(id, action_key.to_owned(), optional(activation_token))
            .await
            .map_err(Error::from)
    }

    async fn clear_application(&self, application_id: &str) -> Result<(), Error> {
        self.notifications
            .clear_app(application_id.to_owned())
            .await
            .map_err(Error::from)
    }

    async fn clear_all(&self) -> Result<(), Error> {
        self.notifications.clear_all().await.map_err(Error::from)
    }

    async fn post(
        &self,
        app_name: String,
        app_id: String,
        icon: String,
        summary: String,
        body: String,
        urgency: u32,
    ) -> Result<u32, Error> {
        if summary.is_empty() {
            return Err(Error::InvalidAction(
                "a notification needs a summary".to_owned(),
            ));
        }
        self.notifications
            .post(Incoming {
                app_name,
                app_id,
                app_pid: None,
                icon: (!icon.is_empty()).then_some(icon),
                image: None,
                summary,
                body,
                actions: Vec::new(),
                urgency: glimpse_dbus::notifications::urgency_from_wire(urgency),
                progress: None,
                resident: false,
                expire_timeout: -1,
                transient: false,
            })
            .await
            .map_err(Error::from)
    }

    async fn set_do_not_disturb(&self, enabled: bool, until: i64) -> Result<(), Error> {
        let until = if until == 0 {
            None
        } else {
            Some(
                DateTime::from_timestamp_micros(until)
                    .ok_or_else(|| Error::InvalidAction("invalid DND expiry".to_owned()))?,
            )
        };
        self.notifications
            .set_dnd(DoNotDisturb { enabled, until })
            .await
            .map_err(Error::from)
    }
}

impl From<CommandError> for Error {
    fn from(error: CommandError) -> Self {
        match error {
            CommandError::InvalidArgument(reason) => Self::InvalidAction(reason),
            error => Self::Unavailable(error.to_string()),
        }
    }
}

pub(crate) type Runtime = Exported<Provider>;

pub(crate) async fn start(
    connection: Connection,
    notifications: NotificationsHandle,
) -> zbus::Result<Runtime> {
    Exported::serve(
        connection,
        GLIMPSE_NOTIFICATIONS_BUS_NAME,
        GLIMPSE_NOTIFICATIONS_OBJECT_PATH,
        Provider {
            notifications: notifications.clone(),
        },
        notifications.subscribe(),
        notifications.health(),
    )
    .await
}

fn snapshot(notifications: &NotificationsHandle) -> NotificationsSnapshot {
    let state = notifications.snapshot();
    let health = notifications.health();
    let unavailable = health.borrow().unavailable_reason().map(str::to_owned);
    let records = state
        .list
        .map(|list| list.notifications)
        .unwrap_or_default()
        .into_iter()
        .map(notification)
        .collect();
    let dnd = state.dnd.map(|dnd| dnd.dnd).unwrap_or_default();
    (
        records,
        do_not_disturb(dnd),
        unavailable.is_none(),
        unavailable.unwrap_or_default(),
    )
}

fn notification(record: NotificationRecord) -> NotificationWire {
    (
        record.id,
        record.app_id,
        record.app_name,
        record.app_pid.unwrap_or(0),
        record.summary,
        record.body.unwrap_or_default(),
        record.icon.unwrap_or_default(),
        record.image.unwrap_or_default(),
        urgency(record.urgency),
        record
            .actions
            .into_iter()
            .map(|action| (action.key, action.label))
            .collect(),
        record.progress.unwrap_or(-1.0),
        record.created.timestamp_micros(),
        record.unread,
        record.resident,
        record.expire_timeout,
    )
}

fn do_not_disturb(dnd: DoNotDisturb) -> DoNotDisturbWire {
    (
        dnd.enabled,
        dnd.until.map(|until| until.timestamp_micros()).unwrap_or(0),
    )
}

fn urgency(urgency: NotificationUrgency) -> u8 {
    match urgency {
        NotificationUrgency::Low => 0,
        NotificationUrgency::Normal => 1,
        NotificationUrgency::Critical => 2,
        NotificationUrgency::Unknown => 255,
    }
}

fn optional(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests;
