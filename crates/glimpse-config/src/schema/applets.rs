use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Applet {
    pub extends: Kind,
    #[serde(default)]
    pub settings: toml::Table,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Audio,
    Battery,
    Brightness,
    Bluetooth,
    Display,
    Clipboard,
    Clock,
    Command,
    Dynamic,
    Exec,
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
