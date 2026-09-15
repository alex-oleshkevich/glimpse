use std::time::Duration;

use futures_util::StreamExt as _;
use glimpse_dbus::status_notifier_watcher::{
    StatusNotifierWatcherProxy, WATCHER_ALIAS, WATCHER_NAME, WATCHER_PATH,
};
use glimpse_dbus::testing::PrivateBus;
use glimpse_dbus::testing::tray::{FakeItem, IncumbentWatcher, Shape};
use zbus::Connection;

async fn watcher(connection: &Connection) -> StatusNotifierWatcherProxy<'static> {
    StatusNotifierWatcherProxy::builder(connection)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

async fn items(connection: &Connection) -> Vec<String> {
    watcher(connection)
        .await
        .registered_status_notifier_items()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_item_registers_by_bus_name_by_path_or_by_both_and_lands_on_one_key() {
    let bus = PrivateBus::start();
    let _incumbent = IncumbentWatcher::start(bus.connection().await)
        .await
        .unwrap();

    let by_name = bus.connection().await;
    watcher(&by_name)
        .await
        .register_status_notifier_item("org.kde.StatusNotifierItem-4242-1")
        .await
        .unwrap();

    let by_path = bus.connection().await;
    watcher(&by_path)
        .await
        .register_status_notifier_item("/org/ayatana/NotificationItem/fake")
        .await
        .unwrap();

    let registered = items(&by_name).await;
    assert_eq!(
        registered,
        [
            format!("{}/StatusNotifierItem", by_name.unique_name().unwrap()),
            format!(
                "{}/org/ayatana/NotificationItem/fake",
                by_path.unique_name().unwrap()
            ),
        ],
        "a bus-name argument falls back to the default path; a path argument is taken as sent"
    );

    watcher(&by_name)
        .await
        .register_status_notifier_item("/StatusNotifierItem")
        .await
        .unwrap();
    assert_eq!(
        items(&by_name).await.len(),
        2,
        "the same owner and path twice is idempotent, however it was spelled"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unregister_removes_exactly_one_and_announces_it() {
    let bus = PrivateBus::start();
    let _incumbent = IncumbentWatcher::start(bus.connection().await)
        .await
        .unwrap();

    let first = bus.connection().await;
    let second = bus.connection().await;
    for connection in [&first, &second] {
        watcher(connection)
            .await
            .register_status_notifier_item("/StatusNotifierItem")
            .await
            .unwrap();
    }
    assert_eq!(items(&first).await.len(), 2);

    let observer = watcher(&bus.connection().await).await;
    let mut gone = observer
        .receive_status_notifier_item_unregistered()
        .await
        .unwrap();

    watcher(&first)
        .await
        .unregister_status_notifier_item("/StatusNotifierItem")
        .await
        .unwrap();

    let signal = tokio::time::timeout(Duration::from_secs(3), gone.next())
        .await
        .expect("an unregistration is announced")
        .expect("a payload");
    assert_eq!(
        signal.args().unwrap().service,
        format!("{}/StatusNotifierItem", first.unique_name().unwrap())
    );
    assert_eq!(
        items(&first).await,
        [format!(
            "{}/StatusNotifierItem",
            second.unique_name().unwrap()
        )],
        "the other owner keeps its item"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_host_registers_once_and_the_flag_is_what_applications_check() {
    let bus = PrivateBus::start();
    let _incumbent = IncumbentWatcher::start(bus.connection().await)
        .await
        .unwrap();
    let host = bus.connection().await;
    let proxy = watcher(&host).await;

    assert!(
        !proxy.is_status_notifier_host_registered().await.unwrap(),
        "applications fall back to XEmbed while this is false"
    );

    let mut announced = proxy
        .receive_status_notifier_host_registered()
        .await
        .unwrap();
    proxy
        .register_status_notifier_host("org.kde.StatusNotifierHost-1234")
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(3), announced.next())
        .await
        .expect("the host registration is announced")
        .expect("a signal");
    assert!(proxy.is_status_notifier_host_registered().await.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_freedesktop_alias_answers_at_the_same_path() {
    let bus = PrivateBus::start();
    let _incumbent = IncumbentWatcher::start(bus.connection().await)
        .await
        .unwrap();
    let client = bus.connection().await;

    watcher(&client)
        .await
        .register_status_notifier_item("/StatusNotifierItem")
        .await
        .unwrap();

    let through_alias: Vec<String> = client
        .call_method(
            Some(WATCHER_NAME),
            WATCHER_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(WATCHER_ALIAS, "RegisteredStatusNotifierItems"),
        )
        .await
        .unwrap()
        .body()
        .deserialize::<zbus::zvariant::Value<'_>>()
        .unwrap()
        .try_into()
        .unwrap();

    assert_eq!(
        through_alias,
        [format!(
            "{}/StatusNotifierItem",
            client.unique_name().unwrap()
        )],
        "a client that only knows the freedesktop spelling sees the same registry"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_item_registers_over_its_own_connection_so_the_key_reaches_its_objects() {
    let bus = PrivateBus::start();
    let _incumbent = IncumbentWatcher::start(bus.connection().await)
        .await
        .unwrap();
    let item = FakeItem::start(bus.connection().await, Shape::Ayatana, "fake")
        .await
        .unwrap();

    item.register().await.unwrap();

    let observer = bus.connection().await;
    assert_eq!(
        items(&observer).await,
        [item.key()],
        "the recorded key addresses the connection the objects are on"
    );
}
