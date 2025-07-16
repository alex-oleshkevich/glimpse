use iced::futures::channel::mpsc;

use crate::{
    app::AppMessage,
    gui::app::Screen,
    search::{Action, SearchItem},
};

#[derive(Debug, Clone)]
pub enum WindowMessage {
    Close,
}

#[derive(Debug, Clone)]
pub enum SearchMessage {
    StartSearch(String),
    ClearResults,
    SetResults(Vec<SearchItem>),
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
