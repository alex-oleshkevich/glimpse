use schemars::JsonSchema;
use serde::de::value::{Error as ValueError, StrDeserializer};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct Applet {
    #[serde(default)]
    pub extends: Option<Kind>,
    #[serde(flatten)]
    #[schemars(with = "serde_json::Map<String, serde_json::Value>")]
    pub settings: toml::Table,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Audio,
    Battery,
    Brightness,
    Bluetooth,
    Display,
    Clipboard,
    Clock,
    Command,
    Exec,
    Heartbeat,
    Idle,
    Keyboard,
    Mpris,
    Network,
    NextEvent,
    Notifications,
    Pager,
    Privacy,
    Printing,
    Removable,
    Session,
    Tray,
    Weather,
    Window,
    Workspace,
}

impl Kind {
    pub fn from_name(name: &str) -> Option<Self> {
        Self::deserialize(StrDeserializer::<ValueError>::new(name)).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zone_entry_resolves_to_the_kind_it_names() {
        assert_eq!(Kind::from_name("clock"), Some(Kind::Clock));
        assert_eq!(Kind::from_name("next-event"), Some(Kind::NextEvent));
        assert_eq!(Kind::from_name("heartbeat"), Some(Kind::Heartbeat));
        assert_eq!(Kind::from_name("Clock"), None, "names are kebab-case");
        assert_eq!(Kind::from_name("nonesuch"), None);
    }

    #[test]
    fn every_applet_named_by_the_default_panels_resolves() {
        let config = crate::Config::default();
        let unresolved: Vec<&String> = config
            .panels
            .iter()
            .flat_map(|panel| [&panel.left, &panel.center, &panel.right])
            .flatten()
            .filter(|name| Kind::from_name(name).is_none())
            .collect();

        assert!(
            unresolved.is_empty(),
            "unresolvable by default: {unresolved:?}"
        );
    }
}
