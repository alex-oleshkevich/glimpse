use std::time::Duration;

use chrono::{TimeDelta, Utc};
use futures_util::StreamExt as _;
use glimpse_config::{CalendarSource, CalendarSourceKind, Config};
use glimpse_dbus::Buses;
use glimpse_dbus::calendar::{Calendar1Proxy, CalendarProvider, decode_range};
use glimpse_dbus::testing::PrivateBus;
use glimpse_services::{Calendar, Running};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::time::timeout;
use zbus::proxy::CacheProperties;

use super::*;

fn config(uri: String) -> Config {
    let mut config = Config::default();
    config.calendar.sources.push(CalendarSource {
        id: "work".into(),
        kind: CalendarSourceKind::Ical,
        uri,
        name: Some("Work".into()),
        poll_interval: None,
        color: Some("purple".into()),
    });
    config
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stalled_feed_does_not_block_the_snapshot_or_refresh() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let document = config(format!(
        "http://{}/stalled.ics",
        listener.local_addr().unwrap()
    ));
    let (mut service, calendar) =
        Running::<Calendar>::spawn(&Config::default(), Buses::unavailable("test"), ());
    let bus = PrivateBus::start();
    let provider = start(bus.connection().await, calendar).await.unwrap();
    service.reconfigure(&document);
    let (_socket, _) = timeout(Duration::from_secs(3), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let client = bus.connection().await;
    let proxy = Calendar1Proxy::builder(&client)
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .unwrap();
    let snapshot = timeout(Duration::from_secs(1), proxy.snapshot())
        .await
        .unwrap()
        .unwrap();
    assert!(snapshot.2.is_empty());
    timeout(Duration::from_secs(1), proxy.refresh())
        .await
        .unwrap()
        .unwrap();
    provider.shutdown().await;
    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restarted_provider_keeps_the_last_snapshot_until_all_sources_finish() {
    let scratch = tempfile::tempdir().unwrap();
    let file = scratch.path().join("work.ics");
    let now = Utc::now().format("%Y%m%dT%H%M%SZ");
    tokio::fs::write(
        &file,
        format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//test//EN\r\n\
             BEGIN:VEVENT\r\nUID:kept\r\nDTSTAMP:{now}\r\nDTSTART:{now}\r\n\
             SUMMARY:Kept\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
        ),
    )
    .await
    .unwrap();
    let (mut service, calendar) = Running::<Calendar>::spawn(
        &config(file.to_string_lossy().into_owned()),
        Buses::unavailable("test"),
        (),
    );
    let bus = PrivateBus::start();
    let provider = start(bus.connection().await, calendar).await.unwrap();
    let mut follower = CalendarProvider::start(bus.connection().await);
    let handle = follower.handle();
    let mut updates = handle.subscribe();
    timeout(
        Duration::from_secs(3),
        updates.wait_for(|state| state.available && !state.events.is_empty()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(handle.snapshot().events.len(), 1);
    assert_eq!(handle.snapshot().events[0].summary, "Kept");
    provider.shutdown().await;
    service.stop().await;
    timeout(
        Duration::from_secs(3),
        updates.wait_for(|state| !state.available),
    )
    .await
    .unwrap()
    .unwrap();

    let empty = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//test//EN\r\nEND:VCALENDAR\r\n";
    tokio::fs::write(&file, empty).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut document = config(format!(
        "http://{}/work.ics",
        listener.local_addr().unwrap()
    ));
    let mut local = config(file.to_string_lossy().into_owned())
        .calendar
        .sources
        .remove(0);
    local.id = "local".into();
    document.calendar.sources.push(local);
    let (mut restarted, calendar) =
        Running::<Calendar>::spawn(&document, Buses::unavailable("test"), ());
    let connection = bus.connection().await;
    let mut starting = tokio::spawn(start(connection, calendar));
    let (mut socket, _) = timeout(Duration::from_secs(3), listener.accept())
        .await
        .unwrap()
        .unwrap();
    assert!(
        timeout(Duration::from_millis(200), &mut starting)
            .await
            .is_err()
    );
    let client = bus.connection().await;
    let dbus = zbus::fdo::DBusProxy::new(&client).await.unwrap();
    assert!(
        !dbus
            .name_has_owner(GLIMPSE_CALENDAR_BUS_NAME.try_into().unwrap())
            .await
            .unwrap()
    );
    assert!(!handle.snapshot().available);
    assert_eq!(handle.snapshot().events[0].summary, "Kept");
    let mut request = [0; 4096];
    timeout(Duration::from_secs(3), socket.read(&mut request))
        .await
        .unwrap()
        .unwrap();
    socket
        .write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{empty}",
                empty.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let provider = timeout(Duration::from_secs(3), starting)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    timeout(
        Duration::from_secs(3),
        updates.wait_for(|state| state.available),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(handle.snapshot().events.is_empty());
    follower.shutdown().await;
    provider.shutdown().await;
    restarted.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn range_queries_return_far_events_without_changing_the_snapshot() {
    let scratch = tempfile::tempdir().unwrap();
    let file = scratch.path().join("work.ics");
    let near = Utc::now() + TimeDelta::hours(1);
    let far = near + TimeDelta::days(200);
    let ics = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:near\r\nDTSTART:{}\r\nSUMMARY:Near\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:far\r\nDTSTART:{}\r\nSUMMARY:Far\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        near.format("%Y%m%dT%H%M%SZ"),
        far.format("%Y%m%dT%H%M%SZ")
    );
    tokio::fs::write(&file, ics).await.unwrap();
    let (mut service, calendar) = Running::<Calendar>::spawn(
        &config(file.to_string_lossy().into_owned()),
        Buses::unavailable("test"),
        (),
    );
    let mut state = calendar.subscribe();
    timeout(Duration::from_secs(5), async {
        loop {
            if state
                .borrow()
                .events
                .iter()
                .any(|event| event.summary == "Near")
            {
                break;
            }
            state.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let mut health = calendar.health();
    timeout(Duration::from_secs(3), async {
        loop {
            if health.borrow().unavailable_reason().is_none() {
                break;
            }
            health.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let bus = PrivateBus::start();
    let provider = start(bus.connection().await, calendar.clone())
        .await
        .unwrap();
    let client = bus.connection().await;
    let proxy = Calendar1Proxy::builder(&client)
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .unwrap();
    let properties = zbus::fdo::PropertiesProxy::builder(&client)
        .destination(GLIMPSE_CALENDAR_BUS_NAME)
        .unwrap()
        .path(GLIMPSE_CALENDAR_OBJECT_PATH)
        .unwrap()
        .build()
        .await
        .unwrap();
    let mut changes = glimpse_dbus::events(properties.receive_properties_changed().await.unwrap());
    let before = decode_range({
        let (_, mark, events, _) = proxy.snapshot().await.unwrap();
        (mark, events)
    })
    .unwrap();
    assert_eq!(
        before
            .events
            .iter()
            .map(|event| event.summary.as_str())
            .collect::<Vec<_>>(),
        ["Near"]
    );
    let found = decode_range(
        proxy
            .events(
                (far - TimeDelta::hours(1)).timestamp_micros(),
                (far + TimeDelta::hours(1)).timestamp_micros(),
            )
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(found.events.len(), 1);
    assert_eq!(found.events[0].summary, "Far");
    assert!(
        timeout(Duration::from_millis(200), changes.next())
            .await
            .is_err()
    );
    let (_, mark, events, _) = proxy.snapshot().await.unwrap();
    assert_eq!(decode_range((mark, events)).unwrap(), before);
    provider.shutdown().await;
    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn typed_errors_and_introspection_match_the_calendar_interface() {
    let (mut service, calendar) =
        Running::<Calendar>::spawn(&Config::default(), Buses::unavailable("test"), ());
    let bus = PrivateBus::start();
    let provider = start(bus.connection().await, calendar).await.unwrap();
    let client = bus.connection().await;
    let proxy = Calendar1Proxy::new(&client).await.unwrap();
    for (from, to) in [(i64::MIN, 0), (0, i64::MAX), (1, 1), (2, 1)] {
        assert!(matches!(
            proxy.events(from, to).await,
            Err(zbus::Error::MethodError(name, _, _))
                if name.as_str() == "me.aresa.Glimpse.Calendar1.Error.InvalidRange"
        ));
    }
    proxy.refresh().await.unwrap();
    assert!(proxy.events(0, 1).await.unwrap().1.is_empty());
    let mut follower = CalendarProvider::start(bus.connection().await);
    let handle = follower.handle();
    let mut updates = handle.subscribe();
    timeout(Duration::from_secs(3), async {
        while !updates.borrow_and_update().available {
            updates.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    handle.refresh().await.unwrap();
    assert!(
        handle
            .events(Utc::now(), Utc::now() + TimeDelta::hours(1))
            .await
            .unwrap()
            .events
            .is_empty()
    );
    let reply = client
        .call_method(
            Some(GLIMPSE_CALENDAR_BUS_NAME),
            GLIMPSE_CALENDAR_OBJECT_PATH,
            Some("org.freedesktop.DBus.Introspectable"),
            "Introspect",
            &(),
        )
        .await
        .unwrap();
    let xml: String = reply.body().deserialize().unwrap();
    let beginning = xml
        .find("<interface name=\"me.aresa.Glimpse.Calendar1\">")
        .unwrap();
    let end = beginning + xml[beginning..].find("</interface>").unwrap() + "</interface>".len();
    assert_eq!(
        &xml[beginning..end],
        r#"<interface name="me.aresa.Glimpse.Calendar1">
    <method name="Events">
      <arg name="from_us" type="x" direction="in"/>
      <arg name="to_us" type="x" direction="in"/>
      <arg type="((bx)a(ssssssss(buu)bxxbs))" direction="out"/>
    </method>
    <method name="Refresh">
    </method>
    <property name="Snapshot" type="(s(bx)a(ssssssss(buu)bxxbs)t)" access="read"/>
  </interface>"#
    );
    follower.shutdown().await;
    service.stop().await;
    assert!(matches!(
        proxy.refresh().await,
        Err(zbus::Error::MethodError(name, _, _))
            if name.as_str() == "me.aresa.Glimpse.Calendar1.Error.Unavailable"
    ));
    provider.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshots_read_current_health_and_emit_its_changes() {
    let (mut service, calendar) =
        Running::<Calendar>::spawn(&Config::default(), Buses::unavailable("test"), ());
    let bus = PrivateBus::start();
    let provider = start(bus.connection().await, calendar.clone())
        .await
        .unwrap();
    let client = bus.connection().await;
    let proxy = Calendar1Proxy::new(&client).await.unwrap();
    let mut changes = proxy.receive_snapshot_changed().await;
    let scratch = tempfile::tempdir().unwrap();
    service.reconfigure(&config(
        scratch
            .path()
            .join("missing.ics")
            .to_string_lossy()
            .into_owned(),
    ));
    timeout(Duration::from_secs(5), async {
        loop {
            if proxy.snapshot().await.unwrap().0.contains("cannot be read") {
                break;
            }
            changes.next().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert!(proxy.snapshot().await.unwrap().0.contains("work"));
    provider.shutdown().await;
    service.stop().await;
}
