use zbus::zvariant::{ObjectPath, OwnedObjectPath};

#[zbus::proxy(
    interface = "org.freedesktop.UPower",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower"
)]
pub trait UPower {
    fn enumerate_devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
    #[zbus(property)]
    fn on_battery(&self) -> zbus::Result<bool>;
}

const KBD_BACKLIGHT_PATH: &str = "/org/freedesktop/UPower/KbdBacklight";
const UPOWER_SERVICE: &str = "org.freedesktop.UPower";

#[zbus::proxy(
    interface = "org.freedesktop.UPower.KbdBacklight",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower/KbdBacklight"
)]
pub trait UPowerKbdBacklight {
    fn get_brightness(&self) -> zbus::Result<i32>;
    fn get_max_brightness(&self) -> zbus::Result<i32>;
    fn set_brightness(&self, value: i32) -> zbus::Result<()>;

    #[zbus(property)]
    fn native_path(&self) -> zbus::Result<String>;

    #[zbus(signal)]
    fn brightness_changed_with_source(&self, value: i32, source: String) -> zbus::Result<()>;
}

#[derive(Debug, Clone)]
pub struct KbdBacklightSource {
    pub path: OwnedObjectPath,
    pub brightness: u32,
    pub max_brightness: u32,
}

pub async fn kbd_backlight_proxy(
    bus: &zbus::Connection,
) -> zbus::Result<(OwnedObjectPath, UPowerKbdBacklightProxy<'static>)> {
    let path = kbd_backlight_path(bus).await;
    let proxy = UPowerKbdBacklightProxy::builder(bus)
        .path(path.clone())?
        .build()
        .await?;
    Ok((path, proxy))
}

pub async fn discover_kbd_backlight(
    bus: &zbus::Connection,
) -> zbus::Result<Option<KbdBacklightSource>> {
    let (path, proxy) = kbd_backlight_proxy(bus).await?;
    let max_brightness = proxy.get_max_brightness().await?;
    if max_brightness <= 0 {
        return Ok(None);
    }
    let brightness = proxy.get_brightness().await?.max(0) as u32;
    Ok(Some(KbdBacklightSource {
        path,
        brightness,
        max_brightness: max_brightness as u32,
    }))
}

async fn kbd_backlight_path(bus: &zbus::Connection) -> OwnedObjectPath {
    let parent = parent_kbd_backlight_path();
    let Some(xml) = introspect_kbd_backlight(bus).await else {
        return parent;
    };
    child_node_names(&xml)
        .into_iter()
        .find_map(|name| OwnedObjectPath::try_from(format!("{KBD_BACKLIGHT_PATH}/{name}")).ok())
        .unwrap_or(parent)
}

async fn introspect_kbd_backlight(bus: &zbus::Connection) -> Option<String> {
    let introspectable = zbus::fdo::IntrospectableProxy::builder(bus)
        .destination(UPOWER_SERVICE)
        .ok()?
        .path(KBD_BACKLIGHT_PATH)
        .ok()?
        .build()
        .await
        .ok()?;
    introspectable.introspect().await.ok()
}

fn parent_kbd_backlight_path() -> OwnedObjectPath {
    OwnedObjectPath::from(ObjectPath::from_static_str_unchecked(KBD_BACKLIGHT_PATH))
}

fn child_node_names(xml: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<node") {
        rest = &rest[start..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..end];
        if let Some(name) = node_name(tag) {
            names.push(name);
        }
        rest = &rest[end + 1..];
    }
    names.sort();
    names
}

fn node_name(tag: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!("name={quote}");
        let Some(start) = tag.find(&needle).map(|pos| pos + needle.len()) else {
            continue;
        };
        let end = tag[start..].find(quote)?;
        return Some(tag[start..start + end].to_owned());
    }
    None
}

#[zbus::proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower"
)]
pub trait UPowerDevice {
    #[zbus(property, name = "Type")]
    fn device_type(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn model(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn icon_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn time_to_empty(&self) -> zbus::Result<i64>;
    #[zbus(property)]
    fn time_to_full(&self) -> zbus::Result<i64>;
    #[zbus(property)]
    fn energy_rate(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn capacity(&self) -> zbus::Result<f64>;
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARENT_ONLY_DOCUMENT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<node>
  <interface name="org.freedesktop.DBus.Introspectable">
    <method name="Introspect">
      <arg name="xml_data" type="s" direction="out"/>
    </method>
  </interface>
  <interface name="org.freedesktop.DBus.Peer">
    <method name="Ping"/>
  </interface>
  <interface name="org.freedesktop.DBus.Properties">
    <method name="GetAll">
      <arg name="interface_name" type="s" direction="in"/>
      <arg name="properties" type="a{sv}" direction="out"/>
    </method>
  </interface>
</node>"#;

    const PARENT_WITH_CHILD_DOCUMENT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<node>
  <interface name="org.freedesktop.DBus.Introspectable">
    <method name="Introspect">
      <arg name="xml_data" type="s" direction="out"/>
    </method>
  </interface>
  <interface name="org.freedesktop.UPower.KbdBacklight">
    <property name="NativePath" type="s" access="read"/>
  </interface>
  <node name="acme_kbd_backlight"/>
</node>"#;

    #[test]
    fn a_document_with_no_child_nodes_yields_no_names() {
        assert_eq!(child_node_names(PARENT_ONLY_DOCUMENT), Vec::<String>::new());
    }

    #[test]
    fn a_document_with_a_child_node_yields_its_name() {
        assert_eq!(
            child_node_names(PARENT_WITH_CHILD_DOCUMENT),
            vec!["acme_kbd_backlight".to_owned()]
        );
    }

    #[test]
    fn a_child_name_resolves_under_the_parent_path() {
        let path = child_node_names(PARENT_WITH_CHILD_DOCUMENT)
            .into_iter()
            .find_map(|name| OwnedObjectPath::try_from(format!("{KBD_BACKLIGHT_PATH}/{name}")).ok())
            .unwrap_or_else(parent_kbd_backlight_path);

        assert_eq!(
            path.as_str(),
            "/org/freedesktop/UPower/KbdBacklight/acme_kbd_backlight"
        );
    }

    #[test]
    fn no_children_falls_back_to_the_parent_path() {
        let path = child_node_names(PARENT_ONLY_DOCUMENT)
            .into_iter()
            .find_map(|name| OwnedObjectPath::try_from(format!("{KBD_BACKLIGHT_PATH}/{name}")).ok())
            .unwrap_or_else(parent_kbd_backlight_path);

        assert_eq!(path.as_str(), KBD_BACKLIGHT_PATH);
    }

    #[test]
    fn a_single_quoted_name_attribute_is_recognised() {
        let document = r#"<node><node name='acme_kbd_backlight'/></node>"#;

        assert_eq!(
            child_node_names(document),
            vec!["acme_kbd_backlight".to_owned()]
        );
    }

    #[test]
    fn an_open_tag_child_node_is_recognised() {
        let document = r#"<node><node name="acme_kbd_backlight"></node></node>"#;

        assert_eq!(
            child_node_names(document),
            vec!["acme_kbd_backlight".to_owned()]
        );
    }

    #[test]
    fn several_children_are_returned_in_a_stable_sorted_order() {
        let document = r#"<node><node name="zzz"/><node name="aaa"/></node>"#;

        assert_eq!(
            child_node_names(document),
            vec!["aaa".to_owned(), "zzz".to_owned()]
        );
    }
}
