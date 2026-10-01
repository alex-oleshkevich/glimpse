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
    <property name="Snapshot" type="(a(ussissssya(ss)dxbbi)(bx)bs)" access="read"/>
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
        expire_timeout: -1,
    });

    assert_eq!(wire.0, 7);
    assert_eq!(wire.3, 0);
    assert_eq!(wire.5, "");
    assert_eq!(wire.8, 2);
    assert_eq!(wire.9, [("reply".to_owned(), "Reply".to_owned())]);
    assert_eq!(wire.10, -1.0);
    assert_eq!(wire.11, created.timestamp_micros());
}
