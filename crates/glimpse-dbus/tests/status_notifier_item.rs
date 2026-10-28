use glimpse_dbus::dbusmenu::{DBusMenuProxy, MenuToggle, ToggleState, decode_layout};
use glimpse_dbus::status_notifier_item::{TrayStatus, decode_item};
use glimpse_dbus::testing::PrivateBus;
use glimpse_dbus::testing::tray::{FakeItem, MENU_PATH, Shape};
use zbus::Connection;
use zbus::names::InterfaceName;

const ITEM: &str = "org.kde.StatusNotifierItem";

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
