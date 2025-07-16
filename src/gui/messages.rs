use crate::{
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
    Clear,
    SetResults(Vec<SearchItem>),
}

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Screen),
    Window(WindowMessage),
    Search(SearchMessage),
    DispatchAction(Action),
    Noop,
}
