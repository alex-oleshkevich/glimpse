use tokio::sync::mpsc;

use crate::{
    app::{Action, AppMessage, SearchItem},
    gui::app::Screen,
};

#[derive(Debug, Clone)]
pub enum WindowMessage {
    Close,
}

#[derive(Debug, Clone)]
pub enum SearchMessage {
    StartSearch(String),
    AddResult(SearchItem),
}

#[derive(Debug, Clone)]
pub enum Message {
    AppBootstrapped(mpsc::Sender<AppMessage>),
    Navigate(Screen),
    Window(WindowMessage),
    Search(SearchMessage),
    DispatchAction(Action),
    Noop,
}
