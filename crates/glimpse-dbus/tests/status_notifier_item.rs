use std::time::Duration;

use futures_util::StreamExt as _;
use glimpse_dbus::dbusmenu::{DBusMenuProxy, MenuToggle, ToggleState, decode_layout};
use glimpse_dbus::status_notifier_item::{StatusNotifierItemProxy, TrayStatus, decode_item};
use glimpse_dbus::testing::PrivateBus;
use glimpse_dbus::testing::tray::{Call, FakeItem, MENU_PATH, Shape};
use zbus::Connection;
use zbus::names::InterfaceName;

const ITEM: &str = "org.kde.StatusNotifierItem";

async fn item(
    connection: &Connection,
    owner: &str,
    path: &str,
) -> StatusNotifierItemProxy<'static> {
    StatusNotifierItemProxy::builder(connection)
        .destination(owner.to_owned())
        .unwrap()
        .path(path.to_owned())
        .unwrap()
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

async fn keys(connection: &Connection, owner: &str, path: &str) -> Vec<String> {
    let properties = zbus::fdo::PropertiesProxy::builder(connection)
        .destination(owner.to_owned())
        .unwrap()
        .path(path.to_owned())
        .unwrap()
        .build()
        .await
        .unwrap();
    let mut names: Vec<String> = properties
        .get_all(InterfaceName::try_from(ITEM).unwrap())
        .await
        .unwrap()
        .into_keys()
        .collect();
    names.sort();
    names
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_ayatana_item_answers_its_extensions_and_errors_on_what_it_never_implemented() {
    let bus = PrivateBus::start();
    let fake = FakeItem::start(bus.connection().await, Shape::Ayatana, "fake-ayatana")
        .await
        .unwrap();
    let client = bus.connection().await;
    let owner = fake.unique_name();
    let proxy = item(&client, &owner, fake.path()).await;

    assert_eq!(proxy.id().await.unwrap(), "fake-ayatana");
    assert_eq!(proxy.status().await.unwrap(), "Active");
    assert_eq!(
        proxy.icon_name().await.unwrap(),
        "folder-publicshare-symbolic"
    );
    assert!(
        proxy.icon_pixmap().await.is_err(),
        "a property the item never implemented is an error, not an empty default"
    );

    fake.set_label("3").await.unwrap();
    assert_eq!(proxy.x_ayatana_label().await.unwrap(), "3");
    assert_eq!(proxy.x_ayatana_label_guide().await.unwrap(), "");
    assert_eq!(proxy.x_ayatana_ordering_index().await.unwrap(), 0);
    assert_eq!(proxy.icon_accessible_desc().await.unwrap(), "fake-ayatana");
    proxy.x_ayatana_secondary_activate(7).await.unwrap();
    assert!(
        proxy.window_id().await.is_err(),
        "WindowId is one of the members this shape does not carry"
    );

    let names = keys(&client, &owner, fake.path()).await;
    assert!(names.contains(&"XAyatanaLabel".to_owned()));
    assert!(
        !names.contains(&"IconPixmap".to_owned()),
        "GetAll returns only what the item implements"
    );
    assert_eq!(
        fake.key(),
        format!("{owner}/org/ayatana/NotificationItem/fake"),
        "the watcher key is the owner's unique name concatenated with the object path"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pixmap_item_is_the_mirror_image_and_puts_tooltip_prose_in_the_title() {
    let bus = PrivateBus::start();
    let fake = FakeItem::start(bus.connection().await, Shape::Pixmap, "fake-pixmap")
        .await
        .unwrap();
    let client = bus.connection().await;
    let owner = fake.unique_name();
    let proxy = item(&client, &owner, fake.path()).await;

    let pixmaps = proxy.icon_pixmap().await.unwrap();
    assert_eq!(
        pixmaps.iter().map(|(w, h, _)| (*w, *h)).collect::<Vec<_>>(),
        [(16, 16), (32, 32)],
        "two sizes, so a scaled output really chooses between them"
    );
    assert!(
        proxy.icon_name().await.is_err(),
        "the Electron shape ships pixmaps only"
    );

    let (icon, pixmap, title, body) = proxy.tool_tip().await.unwrap();
    assert!(icon.is_empty() && pixmap.is_empty() && body.is_empty());
    assert_eq!(
        title, "You have 1 notification",
        "a sentence arrives in the title field, not the body"
    );

    let names = keys(&client, &owner, fake.path()).await;
    assert!(names.contains(&"IconPixmap".to_owned()));
    assert!(!names.contains(&"IconName".to_owned()));
    assert!(!names.contains(&"XAyatanaLabel".to_owned()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn flipping_status_emits_new_status_and_the_property_follows() {
    let bus = PrivateBus::start();
    let fake = FakeItem::start(bus.connection().await, Shape::Ayatana, "fake-flip")
        .await
        .unwrap();
    let client = bus.connection().await;
    let proxy = item(&client, &fake.unique_name(), fake.path()).await;
    let mut statuses = proxy.receive_new_status().await.unwrap();

    fake.set_status("NeedsAttention").await.unwrap();

    let signal = tokio::time::timeout(Duration::from_secs(3), statuses.next())
        .await
        .expect("a signal arrives")
        .expect("a payload");
    assert_eq!(signal.args().unwrap().status, "NeedsAttention");
    assert_eq!(proxy.status().await.unwrap(), "NeedsAttention");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn commands_reach_the_item_and_the_menu_records_what_a_host_asked_for() {
    let bus = PrivateBus::start();
    let fake = FakeItem::start(bus.connection().await, Shape::Ayatana, "fake-commands")
        .await
        .unwrap();
    let client = bus.connection().await;
    let owner = fake.unique_name();
    let proxy = item(&client, &owner, fake.path()).await;

    proxy.activate(12, 34).await.unwrap();
    proxy.scroll(-1, "vertical").await.unwrap();

    let menu = DBusMenuProxy::builder(&client)
        .destination(owner)
        .unwrap()
        .path(MENU_PATH)
        .unwrap()
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    assert_eq!(menu.version().await.unwrap(), 4);
    assert_eq!(menu.status().await.unwrap(), "normal");
    assert!(
        !menu.about_to_show(0).await.unwrap(),
        "AboutToShow reports whether the layout changed, and must be awaited"
    );

    let (revision, _) = menu.get_layout(0, -1, &[]).await.unwrap();
    assert_eq!(revision, 1);
    fake.bump_menu_revision().await.unwrap();
    let (bumped, _) = menu.get_layout(0, -1, &[]).await.unwrap();
    assert_eq!(bumped, 2, "the revision is what gates a refetch");

    fake.set_menu_status("notice").await.unwrap();
    assert_eq!(menu.status().await.unwrap(), "notice");

    assert_eq!(
        fake.calls(),
        [
            Call::Activate(12, 34),
            Call::Scroll(-1, "vertical".to_owned()),
            Call::AboutToShow(0),
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_item_that_offers_no_menu_says_so_and_the_object_is_gone() {
    let bus = PrivateBus::start();
    let fake = FakeItem::start(bus.connection().await, Shape::Ayatana, "fake-menuless")
        .await
        .unwrap();
    let client = bus.connection().await;
    let owner = fake.unique_name();
    let proxy = item(&client, &owner, fake.path()).await;
    assert_eq!(proxy.menu().await.unwrap().as_str(), MENU_PATH);

    fake.drop_menu().await.unwrap();

    assert_eq!(proxy.menu().await.unwrap().as_str(), "/");
    let menu = DBusMenuProxy::builder(&client)
        .destination(owner)
        .unwrap()
        .path(MENU_PATH)
        .unwrap()
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    assert!(
        menu.version().await.is_err(),
        "a dropped menu is UnknownObject, which a host drops rather than retries"
    );
}

async fn properties(
    connection: &Connection,
    owner: &str,
    path: &str,
) -> std::collections::HashMap<String, zbus::zvariant::OwnedValue> {
    zbus::fdo::PropertiesProxy::builder(connection)
        .destination(owner.to_owned())
        .unwrap()
        .path(path.to_owned())
        .unwrap()
        .build()
        .await
        .unwrap()
        .get_all(InterfaceName::try_from(ITEM).unwrap())
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_get_all_decodes_either_shape_without_a_failed_round_trip() {
    let bus = PrivateBus::start();
    let ayatana = FakeItem::start(bus.connection().await, Shape::Ayatana, "walz")
        .await
        .unwrap();
    let electron = FakeItem::start(bus.connection().await, Shape::Pixmap, "Slack_status_icon_1")
        .await
        .unwrap();
    ayatana.set_label("3").await.unwrap();
    let client = bus.connection().await;

    let first = decode_item(
        &ayatana.key(),
        &properties(&client, &ayatana.unique_name(), ayatana.path()).await,
    );
    assert_eq!(first.id, "walz");
    assert_eq!(first.status, TrayStatus::Active);
    assert_eq!(
        first.icon_name.as_deref(),
        Some("folder-publicshare-symbolic")
    );
    assert_eq!(first.label.as_deref(), Some("3"));
    assert!(
        first.icon_pixmaps.is_empty(),
        "the property is absent, which is not an error here"
    );

    let second = decode_item(
        &electron.key(),
        &properties(&client, &electron.unique_name(), electron.path()).await,
    );
    assert_eq!(second.id, "Slack_status_icon_1");
    assert_eq!(second.icon_pixmaps.len(), 2);
    assert!(second.icon_name.is_none());
    assert_eq!(
        second.tooltip.expect("a tooltip").title,
        "You have 1 notification"
    );

    ayatana.set_status("NeedsAttention").await.unwrap();
    let again = decode_item(
        &ayatana.key(),
        &properties(&client, &ayatana.unique_name(), ayatana.path()).await,
    );
    assert_eq!(again.status, TrayStatus::NeedsAttention);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_layout_a_real_menu_sends_decodes_through_every_default() {
    let bus = PrivateBus::start();
    let fake = FakeItem::start(bus.connection().await, Shape::Ayatana, "menu")
        .await
        .unwrap();
    let client = bus.connection().await;
    let menu = DBusMenuProxy::builder(&client)
        .destination(fake.unique_name())
        .unwrap()
        .path(MENU_PATH)
        .unwrap()
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();

    let (_, layout) = menu.get_layout(0, -1, &[]).await.unwrap();
    let root = decode_layout(&layout);

    assert!(root.submenu);
    let labels: Vec<&str> = root
        .children
        .iter()
        .map(|node| node.label.as_str())
        .collect();
    assert_eq!(
        labels,
        [
            "Open Nextcloud",
            "",
            "Recent",
            "Pause syncing",
            "Resolve conflicts",
            "Never shown",
            "Quit"
        ],
        "the mnemonic is stripped and the separator carries no label"
    );

    assert!(root.children[1].separator);
    assert!(!root.children[4].enabled, "enabled: false survives");
    assert!(
        !root.children[5].visible,
        "visible: false survives to the caller"
    );
    assert_eq!(root.children[6].disposition, "alert");
    assert_eq!(root.children[3].toggle, MenuToggle::Checkmark);
    assert_eq!(root.children[3].toggle_state, ToggleState::On);
    assert!(
        root.children[0].enabled && root.children[0].visible,
        "an item that says nothing is enabled and visible"
    );

    let recent = &root.children[2];
    assert_eq!(
        recent.children[0].label, "2026-09 _invoice.odt",
        "a doubled underscore is a literal one"
    );
    assert_eq!(recent.children[1].children[0].label, "2025");
}
