use glimpse_sdk::{Action, Request, Response};
use iced::window;

#[derive(Debug, Clone)]

pub enum Screen {
    MainView,
    PluginView,
}

#[derive(Debug, Clone)]
pub enum Key {
    Escape,
    Down,
    Up,
    Enter,
}

#[derive(Debug, Clone)]
pub enum KeyModifier {
    Shift,
    Control,
    Alt,
}

#[derive(Debug, Clone)]
pub enum Message {
    CallDaemon(Request),
    DaemonResponse {
        request_id: usize,
        plugin_id: Option<usize>,
        response: Response,
    },
    OpenWindow,
    CloseWindow,
    ClearSearch,
    WindowOpened(window::Id),
    Nothing,
    Search(String),
    Navigate(Screen),
    KeyPressed(Key, Vec<KeyModifier>),
    CallAction {
        plugin_id: Option<usize>,
        action: Action,
    },
    Quit,
}
