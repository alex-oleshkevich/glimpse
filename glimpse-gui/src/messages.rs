use glimpse_sdk::{Request, Response};
use iced::window;

#[derive(Debug, Clone)]

pub enum Screen {
    MainView,
    PluginView,
}

#[derive(Debug, Clone)]
pub enum Message {
    DispatchRequest(Request),
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
    Quit,
    EscapePressed,
}
