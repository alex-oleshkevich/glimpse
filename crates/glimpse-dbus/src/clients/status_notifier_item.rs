use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

pub type IconPixmap = Vec<(i32, i32, Vec<u8>)>;

pub type ToolTip = (String, IconPixmap, String, String);

#[zbus::proxy(interface = "org.kde.StatusNotifierItem", assume_defaults = false)]
pub trait StatusNotifierItem {
    fn activate(&self, x: i32, y: i32) -> zbus::Result<()>;
    fn context_menu(&self, x: i32, y: i32) -> zbus::Result<()>;
    fn secondary_activate(&self, x: i32, y: i32) -> zbus::Result<()>;
    fn scroll(&self, delta: i32, orientation: &str) -> zbus::Result<()>;
    fn provide_xdg_activation_token(&self, token: &str) -> zbus::Result<()>;

    /// The Ayatana middle-click, which takes a timestamp where `SecondaryActivate` takes
    /// coordinates. An item offers one, the other, or both.
    #[zbus(name = "XAyatanaSecondaryActivate")]
    fn x_ayatana_secondary_activate(&self, timestamp: u32) -> zbus::Result<()>;

    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn title(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn category(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn icon_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn icon_pixmap(&self) -> zbus::Result<IconPixmap>;
    #[zbus(property)]
    fn overlay_icon_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> zbus::Result<IconPixmap>;
    #[zbus(property)]
    fn attention_icon_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> zbus::Result<IconPixmap>;
    #[zbus(property)]
    fn icon_theme_path(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn item_is_menu(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn menu(&self) -> zbus::Result<OwnedObjectPath>;
    #[zbus(property)]
    fn tool_tip(&self) -> zbus::Result<ToolTip>;
    #[zbus(property)]
    fn window_id(&self) -> zbus::Result<i32>;
    #[zbus(property)]
    fn attention_movie_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn icon_accessible_desc(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn attention_accessible_desc(&self) -> zbus::Result<String>;
    #[zbus(property, name = "XAyatanaLabel")]
    fn x_ayatana_label(&self) -> zbus::Result<String>;
    #[zbus(property, name = "XAyatanaLabelGuide")]
    fn x_ayatana_label_guide(&self) -> zbus::Result<String>;
    #[zbus(property, name = "XAyatanaOrderingIndex")]
    fn x_ayatana_ordering_index(&self) -> zbus::Result<u32>;

    #[zbus(signal)]
    fn new_title(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn new_icon(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn new_overlay_icon(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn new_attention_icon(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn new_tool_tip(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn new_menu(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn new_icon_theme_path(&self, path: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn new_status(&self, status: String) -> zbus::Result<()>;

    #[zbus(signal, name = "XAyatanaNewLabel")]
    fn x_ayatana_new_label(&self, label: String, guide: String) -> zbus::Result<()>;
}

const ID: usize = 200;
const TITLE: usize = 256;
const CATEGORY: usize = 64;
const LABEL: usize = 64;
const TOOLTIP_TITLE: usize = 256;
const TOOLTIP_BODY: usize = 800;
/// `IconName` carries a themed name *or* an absolute path, so it takes the path cap: the icon cap
/// would truncate a legitimate path into one that opens nothing.
const ICON_PATH: usize = 4096;

/// What the item says it is doing. `Passive` is a placement instruction — the host tucks the item
/// away — and not a dimmer tint, which is the reading that makes a bar quietly wrong.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TrayStatus {
    #[default]
    Active,
    Passive,
    NeedsAttention,
}

impl TrayStatus {
    pub fn parse(value: &str) -> Self {
        match value {
            "Passive" => TrayStatus::Passive,
            "NeedsAttention" => TrayStatus::NeedsAttention,
            _ => TrayStatus::Active,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayPixmap {
    pub width: i32,
    pub height: i32,
    /// ARGB32 in network byte order, which is byte order A,R,G,B.
    pub argb: Vec<u8>,
}

/// Four fields, not a string: flattening a tooltip loses its icon and the title/body split, and
/// applications do put a whole sentence in the title.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrayTooltip {
    pub icon_name: Option<String>,
    pub pixmaps: Vec<TrayPixmap>,
    pub title: String,
    pub body: String,
}

impl TrayTooltip {
    pub fn is_empty(&self) -> bool {
        self.icon_name.is_none()
            && self.pixmaps.is_empty()
            && self.title.is_empty()
            && self.body.is_empty()
    }
}

/// One item as glimpse holds it. Every field has a default, because an application implements a
/// subset of the interface and a property it never implemented arrives as an absent key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrayItem {
    pub key: String,
    pub id: String,
    pub title: String,
    pub category: String,
    pub status: TrayStatus,
    pub icon_name: Option<String>,
    pub icon_theme_path: Option<String>,
    pub icon_pixmaps: Vec<TrayPixmap>,
    pub attention_icon_name: Option<String>,
    pub attention_pixmaps: Vec<TrayPixmap>,
    /// Decoded so it is not silently dropped; nothing renders an animation.
    pub attention_movie_name: Option<String>,
    pub overlay_icon_name: Option<String>,
    pub overlay_pixmaps: Vec<TrayPixmap>,
    pub label: Option<String>,
    pub tooltip: Option<TrayTooltip>,
    pub menu: Option<String>,
    pub item_is_menu: bool,
    pub ordering_index: u32,
    /// `com.canonical.dbusmenu.Status` — a *different* `Status`, on the menu object rather than the
    /// item, saying `normal` or `notice`. Set by the follower, not by `decode_item`.
    pub notice: bool,
}

use super::{Properties, flag, number, text};

/// A buffer whose length disagrees with its dimensions is dropped: every consumer indexes it as
/// `width * height * 4`, so a short one would be read past its end. One copy of that guard.
fn decode_pixmaps(raw: IconPixmap) -> Vec<TrayPixmap> {
    raw.into_iter()
        .filter(|(width, height, argb)| {
            *width > 0 && *height > 0 && argb.len() == (*width as usize) * (*height as usize) * 4
        })
        .map(|(width, height, argb)| TrayPixmap {
            width,
            height,
            argb,
        })
        .collect()
}

fn pixmaps(properties: &Properties, key: &str) -> Vec<TrayPixmap> {
    let Some(value) = properties.get(key) else {
        return Vec::new();
    };
    match IconPixmap::try_from(value.clone()) {
        Ok(raw) => decode_pixmaps(raw),
        Err(_) => Vec::new(),
    }
}

/// `Menu` is an `o`, which no `&str` extraction reads; a few implementations send an `s` instead.
fn object_path(value: &OwnedValue) -> Option<String> {
    match &**value {
        Value::ObjectPath(path) => Some(path.as_str().to_owned()),
        Value::Str(text) => Some(text.as_str().to_owned()),
        _ => None,
    }
}

fn tooltip(properties: &Properties) -> Option<TrayTooltip> {
    let value = properties.get("ToolTip")?;
    let (icon_name, raw, title, body) = ToolTip::try_from(value.clone()).ok()?;
    let decoded = TrayTooltip {
        icon_name: super::optional_clean(icon_name, ICON_PATH),
        pixmaps: decode_pixmaps(raw),
        title: glimpse_utils::clean(&title, TOOLTIP_TITLE),
        body: glimpse_utils::clean(&body, TOOLTIP_BODY),
    };
    (!decoded.is_empty()).then_some(decoded)
}

/// Decode the map one `GetAll` returns. Reading each property through its typed getter instead
/// costs a round trip *and* an error for every member the application did not implement.
pub fn decode_item(key: &str, properties: &Properties) -> TrayItem {
    let menu = properties
        .get("Menu")
        .and_then(object_path)
        .filter(|path| path.starts_with('/') && path != "/");

    TrayItem {
        key: key.to_owned(),
        id: text(properties, "Id", ID).unwrap_or_default(),
        title: text(properties, "Title", TITLE).unwrap_or_default(),
        category: text(properties, "Category", CATEGORY).unwrap_or_default(),
        status: properties
            .get("Status")
            .and_then(|value| <&str>::try_from(value).ok())
            .map(TrayStatus::parse)
            .unwrap_or_default(),
        icon_name: text(properties, "IconName", ICON_PATH),
        icon_theme_path: text(properties, "IconThemePath", ICON_PATH),
        icon_pixmaps: pixmaps(properties, "IconPixmap"),
        attention_icon_name: text(properties, "AttentionIconName", ICON_PATH),
        attention_pixmaps: pixmaps(properties, "AttentionIconPixmap"),
        attention_movie_name: text(properties, "AttentionMovieName", ICON_PATH),
        overlay_icon_name: text(properties, "OverlayIconName", ICON_PATH),
        overlay_pixmaps: pixmaps(properties, "OverlayIconPixmap"),
        label: text(properties, "XAyatanaLabel", LABEL),
        tooltip: tooltip(properties),
        menu,
        item_is_menu: flag(properties, "ItemIsMenu").unwrap_or_default(),
        ordering_index: number(properties, "XAyatanaOrderingIndex").unwrap_or_default(),
        notice: false,
    }
}

/// The smallest pixmap that still covers `target`, or the largest available when none does, as an
/// index — returning a reference makes the caller recover the index by comparing whole ARGB
/// buffers. Upscaling a 16px icon to 48 is what makes a tray look broken on a HiDPI bar.
pub fn best_pixmap(pixmaps: &[TrayPixmap], target: i32) -> Option<usize> {
    pixmaps
        .iter()
        .enumerate()
        .filter(|(_, pixmap)| pixmap.width >= target)
        .min_by_key(|(_, pixmap)| pixmap.width)
        .or_else(|| {
            pixmaps
                .iter()
                .enumerate()
                .max_by_key(|(_, pixmap)| pixmap.width)
        })
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
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
                        zbus::zvariant::ObjectPath::try_from(
                            "/org/ayatana/NotificationItem/fake/Menu",
                        )
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
}
