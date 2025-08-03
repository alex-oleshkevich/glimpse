use glimpse_sdk::{Request, Response};
use iced::window;

#[derive(Debug, Clone)]

pub enum Message {
    DispatchRequest(Request),
    DaemonResponse {
        request_id: usize,
        plugin_id: Option<usize>,
        response: Response,
    },
    OpenWindow,
    WindowOpened(window::Id),
    Nothing,
}
