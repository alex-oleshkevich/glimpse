use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

use crate::ThemeMode;

pub const DEFAULT_PANEL_APPLETS_PLACEHOLDER: &str = "...";

/// Marker that expands to all currently-installed dev applets when building a panel.
pub const DEV_SLOT: &str = "__dev__";

/// Replace every `DEV_SLOT` entry in a panel's left/center/right lists with
/// the sorted set of dev-applet names.
pub fn expand_dev_slot(config: &PanelConfig, dev_names: &[String]) -> PanelConfig {
    let expand = |list: &[String]| -> Vec<String> {
        list.iter()
            .flat_map(|name| {
                if name == DEV_SLOT {
                    dev_names.to_vec()
                } else {
                    vec![name.clone()]
                }
            })
            .collect()
    };
    PanelConfig {
        left: expand(&config.left),
        center: expand(&config.center),
        right: expand(&config.right),
        ..config.clone()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Left,
    Top,
    Right,
    Bottom,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Margin {
    #[serde(default)]
    pub left: i32,
    #[serde(default)]
    pub right: i32,
    #[serde(default)]
    pub top: i32,
    #[serde(default)]
    pub bottom: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct PanelConfig {
    pub size: i32,
    pub monitor: Option<String>,
    pub position: Position,
    pub margin: Margin,
    pub theme_mode: ThemeMode,
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
}

impl Default for PanelConfig {
    fn default() -> Self {
        Self {
            size: 36,
            monitor: None,
            position: Position::Top,
            margin: Margin::default(),
            theme_mode: ThemeMode::Dark,
            left: vec!["pager".into(), "mpris".into()],
            center: vec![
                "clock".into(),
                "weather".into(),
                "notifications".into(),
                "privacy".into(),
            ],
            right: vec![
                "tray".into(),
                "removable".into(),
                "clipboard".into(),
                "keyboard".into(),
                "bluetooth".into(),
                "network".into(),
                "brightness".into(),
                "audio".into(),
                "idle".into(),
                "battery".into(),
                "session".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AppletType {
    Audio,
    Battery,
    Bluetooth,
    Brightness,
    Clipboard,
    Clock,
    Command,
    DevGallery,
    Exec,
    Idle,
    Keyboard,
    Mpris,
    Network,
    Notifications,
    Pager,
    Privacy,
    Removable,
    Session,
    Tray,
    Weather,
}

impl AppletType {
    pub fn from_config_name(name: &str) -> Option<Self> {
        match name {
            "audio" => Some(Self::Audio),
            "battery" => Some(Self::Battery),
            "bluetooth" => Some(Self::Bluetooth),
            "brightness" => Some(Self::Brightness),
            "clipboard" => Some(Self::Clipboard),
            "clock" => Some(Self::Clock),
            "devgallery" => Some(Self::DevGallery),
            "idle" => Some(Self::Idle),
            "keyboard" => Some(Self::Keyboard),
            "mpris" => Some(Self::Mpris),
            "network" => Some(Self::Network),
            "notifications" => Some(Self::Notifications),
            "pager" => Some(Self::Pager),
            "privacy" => Some(Self::Privacy),
            "removable" => Some(Self::Removable),
            "session" => Some(Self::Session),
            "tray" => Some(Self::Tray),
            "weather" => Some(Self::Weather),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(default)]
pub struct AppletConfig {
    pub extends: Option<AppletType>,
    #[serde(flatten)]
    pub settings: toml::Value,
}

impl Default for AppletConfig {
    fn default() -> Self {
        Self {
            extends: None,
            settings: toml::Value::Table(toml::map::Map::new()),
        }
    }
}

impl<'de> Deserialize<'de> for AppletConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(default)]
        struct RawAppletConfig {
            extends: Option<String>,
            #[serde(flatten)]
            settings: toml::Value,
        }

        impl Default for RawAppletConfig {
            fn default() -> Self {
                Self {
                    extends: None,
                    settings: toml::Value::Table(toml::map::Map::new()),
                }
            }
        }

        let raw = RawAppletConfig::deserialize(deserializer)?;
        let extends = raw.extends.as_deref().and_then(|name| {
            let applet_type = AppletType::from_config_name(name);
            if applet_type.is_none() {
                tracing::warn!(
                    extends = name,
                    "unknown applet type in extends, ignoring applet config"
                );
            }
            applet_type
        });

        Ok(Self {
            extends,
            settings: raw.settings,
        })
    }
}

pub type AppletConfigs = HashMap<String, AppletConfig>;

#[cfg(test)]
mod tests {
    use super::AppletType;

    #[test]
    fn removable_applet_type_is_available_from_config_name() {
        assert!(AppletType::from_config_name("removable").is_some());
    }
}
