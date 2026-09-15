use std::time::Duration;

use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_dbus::status_notifier_item::TrayStatus;
use glimpse_dbus::status_notifier_watcher::{
    StatusNotifierWatcherProxy, WATCHER_ALIAS, WATCHER_NAME,
};
use glimpse_dbus::testing::PrivateBus;
use glimpse_dbus::testing::tray::{Call, FakeItem, IncumbentWatcher, Shape};
use glimpse_services::{Running, ServiceState, Tray, TrayHandle};
use zbus::Connection;

async fn tray(bus: &PrivateBus) -> (Running<Tray>, TrayHandle) {
    let (service, handle, _) = tray_named(bus).await;
    (service, handle)
}

async fn tray_named(bus: &PrivateBus) -> (Running<Tray>, TrayHandle, String) {
    let connection = bus.connection().await;
    let name = connection
        .unique_name()
        .map(ToString::to_string)
        .unwrap_or_default();
    let buses = Buses::for_session(connection);
    let (service, handle) = Running::<Tray>::spawn(&Config::default(), buses, ());
    (service, handle, name)
}

async fn settle(handle: &TrayHandle, ready: impl Fn(&ServiceState) -> bool) -> ServiceState {
    let mut health = handle.health();
    let settled = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if ready(&health.borrow_and_update()) {
                return;
            }
            health.changed().await.expect("the service is alive");
        }
    })
    .await;
    assert!(
        settled.is_ok(),
        "health never settled; last seen {:?}",
        *handle.health().borrow()
    );
    handle.health().borrow().clone()
}

/// The claim happens in the `Watch::Names` source, which the runtime starts *after* `start`
/// returns, so health reaches `Running` before the name is ours. Wait for the name itself.
async fn claimed(bus: &PrivateBus) {
    owns(bus, WATCHER_NAME).await;
}

/// The alias is taken *after* the KDE name, so a test reaching for it has its own wait: the two
/// are separate requests and owning one says nothing about the other.
async fn owns(bus: &PrivateBus, name: &str) {
    let connection = bus.connection().await;
    let proxy = zbus::fdo::DBusProxy::new(&connection).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if proxy
                .name_has_owner(name.try_into().expect("a bus name"))
                .await
                .unwrap_or(false)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the tray takes {name}"));
}

/// `Running` is set by the framework the moment `start` returns, so it says nothing about whether
/// the host registration has happened. The incumbent's own flag is what does.
async fn hosted(bus: &PrivateBus) {
    let connection = bus.connection().await;
    let proxy = watcher(&connection).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if proxy
                .is_status_notifier_host_registered()
                .await
                .unwrap_or(false)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the tray registers as a host with the watcher that owns the name");
}

async fn watcher(connection: &Connection) -> StatusNotifierWatcherProxy<'static> {
    StatusNotifierWatcherProxy::builder(connection)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_service_takes_the_watcher_name_and_registers_itself_as_a_host() {
    let bus = PrivateBus::start();
    let (mut service, handle) = tray(&bus).await;

    let state = settle(&handle, |state| matches!(state, ServiceState::Running)).await;
    assert_eq!(state, ServiceState::Running);
    claimed(&bus).await;

    // Applications fall back to XEmbed while this is false, so items would never appear. It is
    // registered *after* the name is taken, which is why holding the name is not enough to assert.
    hosted(&bus).await;

    let client = bus.connection().await;
    let proxy = watcher(&client).await;
    assert_eq!(
        proxy.registered_status_notifier_items().await.unwrap(),
        Vec::<String>::new()
    );

    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exactly_one_of_two_services_holds_the_name_and_the_other_hosts_on_it() {
    let bus = PrivateBus::start();
    let (mut first, first_handle) = tray(&bus).await;
    let (mut second, second_handle) = tray(&bus).await;

    // Which of the two wins is the bus's business, and the claim runs off the handler, so waiting
    // on one of them in particular would be asserting an order nothing promises. The contract is
    // that exactly one holds the name and *both* end up serving: the loser registers with the
    // winner as a host rather than sitting over an empty bar until it exits.
    let settled = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let (ServiceState::Running, ServiceState::Running) = (
                first_handle.health().borrow().clone(),
                second_handle.health().borrow().clone(),
            ) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;

    settled.unwrap_or_else(|_| {
        panic!(
            "one holds the name and the other hosts on it, so both serve; saw {:?} and {:?}",
            *first_handle.health().borrow(),
            *second_handle.health().borrow()
        )
    });

    let probe = bus.connection().await;
    assert!(
        zbus::fdo::DBusProxy::new(&probe)
            .await
            .unwrap()
            .name_has_owner(WATCHER_NAME.try_into().expect("a bus name"))
            .await
            .unwrap_or(false),
        "the winner keeps the name"
    );

    second.stop().await;
    first.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_item_that_registers_reaches_the_published_list() {
    let bus = PrivateBus::start();
    let (mut service, handle) = tray(&bus).await;
    settle(&handle, |state| matches!(state, ServiceState::Running)).await;
    claimed(&bus).await;

    let item = FakeItem::start(bus.connection().await, Shape::Ayatana, "fake")
        .await
        .unwrap();
    item.register().await.unwrap();

    let mut items = handle.subscribe();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if !items.borrow_and_update().items.is_empty() {
                return;
            }
            items.changed().await.expect("the service is alive");
        }
    })
    .await
    .expect("the item reaches the published list");

    assert_eq!(
        items
            .borrow()
            .items
            .iter()
            .map(|item| item.key.clone())
            .collect::<Vec<_>>(),
        [item.key()]
    );

    service.stop().await;
}

async fn items_settle(handle: &TrayHandle, expected: usize) -> Vec<String> {
    let mut items = handle.subscribe();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if items.borrow_and_update().items.len() == expected {
                return;
            }
            items.changed().await.expect("the service is alive");
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "expected {expected} items, saw {:?}",
            handle
                .snapshot()
                .items
                .iter()
                .map(|item| item.key.clone())
                .collect::<Vec<_>>()
        )
    });
    handle
        .snapshot()
        .items
        .iter()
        .map(|item| item.key.clone())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_name_freeing_is_the_trigger_and_the_sweep_finds_what_registered_meanwhile() {
    let bus = PrivateBus::start();
    let incumbent = IncumbentWatcher::start(bus.connection().await)
        .await
        .unwrap();

    let (mut service, handle) = tray(&bus).await;
    hosted(&bus).await;

    // An item holding a well-known name that never registered with *anyone*: the incumbent cannot
    // list it, so hosting on the incumbent does not find it and only a sweep can.
    let item = FakeItem::start(bus.connection().await, Shape::Pixmap, "early")
        .await
        .unwrap();
    item.claim_well_known_name().await.unwrap();

    incumbent.stop().await.unwrap();

    claimed(&bus).await;
    assert_eq!(
        items_settle(&handle, 1).await,
        [item.key()],
        "the sweep adopts an item holding a well-known name, which never re-registers on its own"
    );

    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_watcher_we_cannot_have_is_hosted_on_rather_than_waited_out() {
    let bus = PrivateBus::start();
    let incumbent = IncumbentWatcher::start(bus.connection().await)
        .await
        .unwrap();

    // The item registers with the watcher that exists, which is the incumbent and never us.
    let item = FakeItem::start(bus.connection().await, Shape::Pixmap, "theirs")
        .await
        .unwrap();
    item.register().await.unwrap();

    let (mut service, handle) = tray(&bus).await;
    hosted(&bus).await;

    assert_eq!(
        items_settle(&handle, 1).await,
        [item.key()],
        "an item registered with someone else's watcher still reaches our bar"
    );
    assert!(
        !matches!(&*handle.health().borrow(), ServiceState::Degraded { .. }),
        "a taken name is not a broken tray: the spec separates watcher from host for this"
    );

    service.stop().await;
    incumbent.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_item_whose_owner_vanishes_leaves_the_bar_without_unregistering() {
    let bus = PrivateBus::start();
    let (mut service, handle) = tray(&bus).await;
    settle(&handle, |state| matches!(state, ServiceState::Running)).await;
    claimed(&bus).await;

    let first = FakeItem::start(bus.connection().await, Shape::Ayatana, "stays")
        .await
        .unwrap();
    let second = FakeItem::start(bus.connection().await, Shape::Pixmap, "goes")
        .await
        .unwrap();
    first.register().await.unwrap();
    second.register().await.unwrap();
    assert_eq!(items_settle(&handle, 2).await.len(), 2);

    // Most applications never call UnregisterStatusNotifierItem; they just exit.
    drop(second);

    assert_eq!(
        items_settle(&handle, 1).await,
        [first.key()],
        "a dead chip leaves on NameOwnerChanged, not on an unregistration nobody sends"
    );

    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn announcing_before_sweeping_yields_one_entry_for_a_client_that_does_both() {
    let bus = PrivateBus::start();
    let incumbent = IncumbentWatcher::start(bus.connection().await)
        .await
        .unwrap();
    let (mut service, handle) = tray(&bus).await;
    hosted(&bus).await;

    let item = FakeItem::start(bus.connection().await, Shape::Ayatana, "both")
        .await
        .unwrap();
    item.claim_well_known_name().await.unwrap();
    item.register().await.unwrap();
    item.re_register_on_host_announcement().await.unwrap();

    incumbent.stop().await.unwrap();
    settle(&handle, |state| matches!(state, ServiceState::Running)).await;
    claimed(&bus).await;

    let keys = items_settle(&handle, 1).await;
    assert_eq!(
        keys,
        [item.key()],
        "the announcement and the sweep collapse onto one canonical key"
    );

    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_item_is_decoded_from_the_bus_and_a_repeat_of_the_same_bytes_publishes_nothing() {
    let bus = PrivateBus::start();
    let (mut service, handle) = tray(&bus).await;
    settle(&handle, |state| matches!(state, ServiceState::Running)).await;
    claimed(&bus).await;

    let item = FakeItem::start(bus.connection().await, Shape::Ayatana, "walz")
        .await
        .unwrap();
    item.register().await.unwrap();
    items_settle(&handle, 1).await;

    let decoded = handle
        .snapshot()
        .items
        .into_iter()
        .next()
        .expect("one item");
    assert_eq!(decoded.id, "walz");
    assert_eq!(
        decoded.icon_name.as_deref(),
        Some("folder-publicshare-symbolic")
    );
    assert_eq!(decoded.status, TrayStatus::Active);

    let mut changes = handle.subscribe();
    changes.borrow_and_update();
    item.set_title("walz").await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(400), changes.changed())
            .await
            .is_err(),
        "identical bytes must not republish; the equality gate is what makes New* and \
         PropertiesChanged safe to follow together"
    );

    item.set_status("NeedsAttention").await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), changes.changed())
        .await
        .expect("a real change does arrive")
        .unwrap();
    assert_eq!(
        handle.snapshot().items[0].status,
        TrayStatus::NeedsAttention
    );

    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hostile_title_is_capped_before_it_reaches_the_published_state() {
    let bus = PrivateBus::start();
    let (mut service, handle) = tray(&bus).await;
    settle(&handle, |state| matches!(state, ServiceState::Running)).await;
    claimed(&bus).await;

    let item = FakeItem::start(bus.connection().await, Shape::Ayatana, "hostile")
        .await
        .unwrap();
    item.register().await.unwrap();
    items_settle(&handle, 1).await;

    let hostile = "ы".repeat(400);
    let mut changes = handle.subscribe();
    changes.borrow_and_update();
    item.set_title(&hostile).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), changes.changed())
        .await
        .expect("the change arrives")
        .unwrap();

    let title = handle.snapshot().items[0].title.clone();
    assert!(
        title.chars().count() < 400,
        "an unbounded title reached the state: {} characters",
        title.chars().count()
    );

    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn commands_reach_the_application_and_the_menu_is_decoded_once_per_revision() {
    let bus = PrivateBus::start();
    let (mut service, handle) = tray(&bus).await;
    settle(&handle, |state| matches!(state, ServiceState::Running)).await;
    claimed(&bus).await;

    let item = FakeItem::start(bus.connection().await, Shape::Ayatana, "commands")
        .await
        .unwrap();
    item.register().await.unwrap();
    items_settle(&handle, 1).await;
    let key = item.key();

    handle.activate(key.clone(), 12, 34).await.unwrap();
    handle
        .scroll(key.clone(), -1, "vertical".to_owned())
        .await
        .unwrap();

    let menu = handle.menu(key.clone()).await.unwrap();
    assert!(menu.submenu);
    assert_eq!(menu.children[0].label, "Open Nextcloud");
    assert_eq!(menu.children[2].children[1].children[0].label, "2025");

    handle
        .menu_event(key.clone(), 1, "clicked".to_owned())
        .await
        .unwrap();

    for expected in [
        Call::Activate(12, 34),
        Call::Scroll(-1, "vertical".to_owned()),
        Call::AboutToShow(0),
        // `Event` is no_reply on purpose — awaiting a reply would block the click on whatever the
        // application decides to do — so the arrival has to be waited for rather than assumed.
        Call::Event(1, "clicked".to_owned()),
    ] {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !item.calls().contains(&expected) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{expected:?} never reached the application"));
    }

    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_command_for_an_item_that_is_not_there_fails_without_blocking_the_next_one() {
    let bus = PrivateBus::start();
    let (mut service, handle) = tray(&bus).await;
    settle(&handle, |state| matches!(state, ServiceState::Running)).await;
    claimed(&bus).await;

    let item = FakeItem::start(bus.connection().await, Shape::Ayatana, "present")
        .await
        .unwrap();
    item.register().await.unwrap();
    items_settle(&handle, 1).await;

    let stranger = handle.activate(":1.999/StatusNotifierItem".to_owned(), 0, 0);
    let present = handle.activate(item.key(), 1, 2);
    let (missing, ok) = tokio::join!(stranger, present);

    assert!(
        missing.is_err(),
        "a command to nothing is an error, not a hang"
    );
    assert!(ok.is_ok(), "and it does not hold up the item that is there");

    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_menus_own_status_reaches_the_item_as_notice() {
    let bus = PrivateBus::start();
    let (mut service, handle) = tray(&bus).await;
    settle(&handle, |state| matches!(state, ServiceState::Running)).await;
    claimed(&bus).await;

    let item = FakeItem::start(bus.connection().await, Shape::Ayatana, "notice")
        .await
        .unwrap();
    item.register().await.unwrap();
    items_settle(&handle, 1).await;
    assert!(
        !handle.snapshot().items[0].notice,
        "a menu reporting `normal` is not a notice"
    );

    let mut changes = handle.subscribe();
    changes.borrow_and_update();
    item.set_menu_status("notice").await.unwrap();

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if handle
                .snapshot()
                .items
                .first()
                .is_some_and(|item| item.notice)
            {
                return;
            }
            changes.changed().await.expect("the service is alive");
        }
    })
    .await
    .expect("com.canonical.dbusmenu.Status reaches the item, unlike the item's own Status");

    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_client_that_only_knows_the_freedesktop_spelling_finds_a_watcher() {
    let bus = PrivateBus::start();
    let (mut service, handle) = tray(&bus).await;
    settle(&handle, |state| matches!(state, ServiceState::Running)).await;
    claimed(&bus).await;
    owns(&bus, WATCHER_ALIAS).await;

    let client = bus.connection().await;
    let items: Vec<String> = client
        .call_method(
            Some("org.freedesktop.StatusNotifierWatcher"),
            "/StatusNotifierWatcher",
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(
                "org.freedesktop.StatusNotifierWatcher",
                "RegisteredStatusNotifierItems",
            ),
        )
        .await
        .expect("the alias name has an owner, not just an exported interface")
        .body()
        .deserialize::<zbus::zvariant::Value<'_>>()
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(items, Vec::<String>::new());

    service.stop().await;
}
