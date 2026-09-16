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
mod tests {

    use chrono::{TimeZone, Utc};
    use glimpse_dbus::notifications::NotificationAction;
    use glimpse_dbus::{Buses, notifications::Notifications1Proxy};
    use glimpse_services::{Notifications, Service, ServiceRuntime};
    use tokio_util::sync::CancellationToken;

    use glimpse_dbus::testing::PrivateBus;

    use super::*;

    #[test]
    fn a_snapshot_says_the_service_is_not_serving_and_why() {
        let (_runtime, notifications) = ServiceRuntime::<Notifications>::new(
            <Notifications as Service>::Config::from(&glimpse_config::Config::default()),
            Buses::unavailable("no bus in tests"),
            CancellationToken::new(),
        );

        let (_records, _dnd, serving, reason) = snapshot(&notifications);

        assert!(!serving);
        assert_eq!(reason, "starting");
    }

    const NOT_YET_DUE: i64 = 4_102_444_800_000_000;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn introspection_and_typed_round_trip_match_the_frozen_interface() {
        let bus = PrivateBus::start();
        let cancel = CancellationToken::new();
        let (mut service, notifications) = ServiceRuntime::<Notifications>::new(
            <Notifications as Service>::Config::from(&glimpse_config::Config::default()),
            Buses::unavailable("no backend bus in test"),
            cancel.clone(),
        );
        let service_task = tokio::spawn(async move { service.run(()).await });
        let provider = start(bus.connection().await, notifications.clone())
            .await
            .unwrap();
        let client = bus.connection().await;
        let proxy = Notifications1Proxy::new(&client).await.unwrap();

        proxy.set_do_not_disturb(true, NOT_YET_DUE).await.unwrap();
        notifications
            .subscribe()
            .wait_for(|state| {
                state
                    .dnd
                    .as_ref()
                    .is_some_and(|state| state.dnd.enabled && state.dnd.until.is_some())
            })
            .await
            .unwrap();

        let reply = client
            .call_method(
                Some(GLIMPSE_NOTIFICATIONS_BUS_NAME),
                GLIMPSE_NOTIFICATIONS_OBJECT_PATH,
                Some("org.freedesktop.DBus.Introspectable"),
                "Introspect",
                &(),
            )
            .await
            .unwrap();
        let xml: String = reply.body().deserialize().unwrap();
        let start = xml
            .find("<interface name=\"me.aresa.Glimpse.Notifications1\">")
            .unwrap();
        let end = start + xml[start..].find("</interface>").unwrap() + "</interface>".len();
        assert_eq!(
            &xml[start..end],
            r#"<interface name="me.aresa.Glimpse.Notifications1">
    <method name="Dismiss">
      <arg name="id" type="u" direction="in"/>
    </method>
    <method name="Remove">
      <arg name="id" type="u" direction="in"/>
    </method>
    <method name="Activate">
      <arg name="id" type="u" direction="in"/>
      <arg name="activation_token" type="s" direction="in"/>
    </method>
    <method name="InvokeAction">
      <arg name="id" type="u" direction="in"/>
      <arg name="action_key" type="s" direction="in"/>
      <arg name="activation_token" type="s" direction="in"/>
    </method>
    <method name="ClearApplication">
      <arg name="application_id" type="s" direction="in"/>
    </method>
    <method name="ClearAll">
    </method>
    <method name="Post">
      <arg name="app_name" type="s" direction="in"/>
      <arg name="app_id" type="s" direction="in"/>
      <arg name="icon" type="s" direction="in"/>
      <arg name="summary" type="s" direction="in"/>
      <arg name="body" type="s" direction="in"/>
      <arg name="urgency" type="u" direction="in"/>
      <arg type="u" direction="out"/>
    </method>
    <method name="SetDoNotDisturb">
      <arg name="enabled" type="b" direction="in"/>
      <arg name="until" type="x" direction="in"/>
    </method>
    <property name="Snapshot" type="(a(ussissssya(ss)dxbb)(bx)bs)" access="read"/>
  </interface>"#
        );

        provider.shutdown().await;
        cancel.cancel();
        service_task.await.unwrap().unwrap();
    }

    #[test]
    fn a_record_maps_to_the_frozen_wire_shape() {
        let created = Utc.with_ymd_and_hms(2026, 9, 13, 12, 30, 0).unwrap();
        let wire = notification(NotificationRecord {
            id: 7,
            app_id: "org.example.Chat".to_owned(),
            app_name: "Chat".to_owned(),
            app_pid: None,
            summary: "Message".to_owned(),
            body: None,
            icon: Some("chat".to_owned()),
            image: None,
            urgency: NotificationUrgency::Critical,
            actions: vec![NotificationAction {
                key: "reply".to_owned(),
                label: "Reply".to_owned(),
            }],
            progress: None,
            created,
            unread: true,
            resident: false,
        });

        assert_eq!(wire.0, 7);
        assert_eq!(wire.3, 0);
        assert_eq!(wire.5, "");
        assert_eq!(wire.8, 2);
        assert_eq!(wire.9, [("reply".to_owned(), "Reply".to_owned())]);
        assert_eq!(wire.10, -1.0);
        assert_eq!(wire.11, created.timestamp_micros());
    }
}
