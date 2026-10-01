use super::*;

fn properties(pairs: Vec<(&str, Value<'static>)>) -> Properties {
    pairs
        .into_iter()
        .map(|(key, value)| {
            (
                key.to_owned(),
                OwnedValue::try_from(value).expect("a plain value"),
            )
        })
        .collect()
}

fn pixmap(size: i32) -> TrayPixmap {
    TrayPixmap {
        width: size,
        height: size,
        argb: vec![0; (size * size * 4) as usize],
    }
}

#[test]
fn an_item_that_implements_nothing_decodes_to_defaults_rather_than_failing() {
    let item = decode_item(":1.9/StatusNotifierItem", &properties(Vec::new()));
    assert_eq!(item.key, ":1.9/StatusNotifierItem");
    assert_eq!(item.status, TrayStatus::Active, "no Status reads as Active");
    assert!(item.id.is_empty() && item.title.is_empty());
    assert!(item.icon_name.is_none() && item.icon_pixmaps.is_empty());
    assert!(item.menu.is_none() && !item.item_is_menu);
    assert!(item.tooltip.is_none(), "an empty tooltip is no tooltip");
}

#[test]
fn the_ayatana_shape_and_the_pixmap_shape_both_decode_from_what_each_sends() {
    let ayatana = decode_item(
        ":1.29/org/ayatana/NotificationItem/fake",
        &properties(vec![
            ("Id", Value::from("tray-icon")),
            ("Title", Value::from("walz")),
            ("IconName", Value::from("/run/user/1000/tray-icon/x.png")),
            ("IconThemePath", Value::from("/run/user/1000/tray-icon")),
            ("XAyatanaLabel", Value::from("3")),
            (
                "Menu",
                Value::from(
                    zbus::zvariant::ObjectPath::try_from("/org/ayatana/NotificationItem/fake/Menu")
                        .expect("a path"),
                ),
            ),
        ]),
    );
    assert_eq!(
        ayatana.icon_name.as_deref(),
        Some("/run/user/1000/tray-icon/x.png"),
        "an absolute path in IconName survives the cap meant for a themed name"
    );
    assert_eq!(ayatana.label.as_deref(), Some("3"));
    assert_eq!(
        ayatana.menu.as_deref(),
        Some("/org/ayatana/NotificationItem/fake/Menu")
    );
    assert!(ayatana.icon_pixmaps.is_empty());

    let electron = decode_item(
        ":1.2/StatusNotifierItem",
        &properties(vec![
            ("Id", Value::from("Slack_status_icon_1")),
            ("IconPixmap", Value::from(vec![(2i32, 2i32, vec![0u8; 16])])),
            ("ItemIsMenu", Value::from(false)),
            (
                "ToolTip",
                Value::from((
                    String::new(),
                    Vec::<(i32, i32, Vec<u8>)>::new(),
                    "You have 1 notification".to_owned(),
                    String::new(),
                )),
            ),
        ]),
    );
    assert!(
        electron.icon_name.is_none(),
        "no IconName key, no icon name"
    );
    assert_eq!(electron.icon_pixmaps, [pixmap(2)]);
    let tooltip = electron.tooltip.expect("a tooltip");
    assert_eq!(
        tooltip.title, "You have 1 notification",
        "a sentence arrives in the title field and stays there"
    );
    assert!(tooltip.body.is_empty() && tooltip.icon_name.is_none());
}

#[test]
fn a_pixmap_whose_bytes_do_not_match_its_dimensions_is_dropped() {
    let item = decode_item(
        ":1.2/StatusNotifierItem",
        &properties(vec![(
            "IconPixmap",
            Value::from(vec![
                (2i32, 2i32, vec![0u8; 3]),
                (0i32, 0i32, Vec::<u8>::new()),
                (1i32, 1i32, vec![0u8; 4]),
            ]),
        )]),
    );
    assert_eq!(
        item.icon_pixmaps,
        [pixmap(1)],
        "a short buffer would be read past its end by any consumer"
    );
}

#[test]
fn a_menu_path_of_root_means_the_item_offers_none() {
    let item = decode_item(
        ":1.2/StatusNotifierItem",
        &properties(vec![(
            "Menu",
            Value::from(zbus::zvariant::ObjectPath::try_from("/").expect("a path")),
        )]),
    );
    assert!(item.menu.is_none());
}

#[test]
fn hostile_text_is_capped_by_characters_and_not_by_bytes() {
    let hostile = "ы".repeat(TITLE * 2);
    let item = decode_item(
        ":1.2/StatusNotifierItem",
        &properties(vec![("Title", Value::from(hostile))]),
    );
    assert_eq!(
        item.title.chars().count(),
        TITLE + 1,
        "the cap adds an ellipsis"
    );
}

#[test]
fn the_smallest_pixmap_that_still_covers_the_target_wins() {
    let sizes = [pixmap(16), pixmap(22), pixmap(32), pixmap(48)];
    assert_eq!(
        best_pixmap(&sizes, 22),
        Some(1),
        "an exact size is used as-is"
    );
    assert_eq!(best_pixmap(&sizes, 24), Some(2));
    assert_eq!(
        best_pixmap(&sizes, 44),
        Some(3),
        "never the largest by default, only when it is the smallest that fits"
    );
}

#[test]
fn nothing_large_enough_falls_back_to_the_largest_rather_than_to_nothing() {
    let sizes = [pixmap(16), pixmap(22)];
    assert_eq!(best_pixmap(&sizes, 48), Some(1));
    assert_eq!(best_pixmap(&[], 48), None);
}

#[test]
fn a_scaled_bar_asks_for_the_scaled_size_and_gets_a_different_answer() {
    let sizes = [pixmap(16), pixmap(24), pixmap(48)];
    assert_eq!(best_pixmap(&sizes, 24), Some(1));
    assert_eq!(
        best_pixmap(&sizes, 48),
        Some(2),
        "scale 2 of the same 24px slot must not reuse the 24px pixmap"
    );
}
