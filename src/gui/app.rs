use iced::*;

use crate::{
    app::AppChannel,
    gui::widgets::{main_view, plugin_view},
    search::{Action, Icon, SearchItem},
};

#[derive(Debug, Clone)]
pub enum Screen {
    Search,
    PluginView,
}

#[derive(Debug)]
pub struct State {
    pub query: String,
    pub screen: Screen,
    pub search_results: Vec<SearchItem>,
    pub app_channel: AppChannel,
}

impl State {
    pub fn new(app_channel: AppChannel) -> Self {
        let mut state = State::default();
        state.app_channel = app_channel;
        return state;
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            query: String::new(),
            screen: Screen::Search,
            app_channel: AppChannel::new(),
            search_results: vec![
                SearchItem {
                    title: "Example Item".to_string(),
                    subtitle: "This is an example subtitle".to_string(),
                    icon: Icon::Path(
                        "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                    ),
                    category: "Apps".to_string(),
                    actions: vec![Action {}],
                },
                SearchItem {
                    title: "Another Item".to_string(),
                    subtitle: "This is another example subtitle".to_string(),
                    icon: Icon::Path(
                        "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                    ),
                    category: "Apps".to_string(),
                    actions: vec![Action {}],
                },
                SearchItem {
                    title: "Third Item".to_string(),
                    subtitle: "This is a third example subtitle".to_string(),
                    icon: Icon::Path(
                        "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                    ),
                    category: "Apps".to_string(),
                    actions: vec![Action {}],
                },
                SearchItem {
                    title: "Fourth Item".to_string(),
                    subtitle: "This is a fourth example subtitle".to_string(),
                    icon: Icon::Path(
                        "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                    ),
                    category: "Apps".to_string(),
                    actions: vec![Action {}],
                },
                SearchItem {
                    title: "Fifth Item".to_string(),
                    subtitle: "This is a fifth example subtitle".to_string(),
                    icon: Icon::Path(
                        "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                    ),
                    category: "Apps".to_string(),
                    actions: vec![Action {}],
                },
                SearchItem {
                    title: "Sixth Item".to_string(),
                    subtitle: "This is a sixth example subtitle".to_string(),
                    icon: Icon::Path(
                        "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                    ),
                    category: "Apps".to_string(),
                    actions: vec![Action {}],
                },
                SearchItem {
                    title: "Seventh Item".to_string(),
                    subtitle: "This is a seventh example subtitle".to_string(),
                    icon: Icon::Path(
                        "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                    ),
                    category: "Apps".to_string(),
                    actions: vec![Action {}],
                },
                SearchItem {
                    title: "Eighth Item".to_string(),
                    subtitle: "This is an eighth example subtitle".to_string(),
                    icon: Icon::Path(
                        "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                    ),
                    category: "Apps".to_string(),
                    actions: vec![Action {}],
                },
                SearchItem {
                    title: "Ninth Item".to_string(),
                    subtitle: "This is a ninth example subtitle".to_string(),
                    icon: Icon::Path(
                        "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                    ),
                    category: "Apps".to_string(),
                    actions: vec![Action {}],
                },
                SearchItem {
                    title: "Tenth Item".to_string(),
                    subtitle: "This is a tenth example subtitle".to_string(),
                    icon: Icon::Path(
                        "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                    ),
                    category: "Apps".to_string(),
                    actions: vec![Action {}],
                },
            ],
        }
    }
}

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

pub fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::Navigate(screen) => state.screen = screen,
        Message::DispatchAction(action) => {
            println!("Executing action: {:?}", action);
        }
        Message::Search(message) => match message {
            SearchMessage::StartSearch(title) => state.query = title,
            SearchMessage::Clear => state.query.clear(),
            SearchMessage::SetResults(results) => state.search_results = results,
        },
        Message::Window(WindowMessage::Close) => {
            return iced::window::get_latest()
                .and_then(|id| iced::window::change_mode(id, iced::window::Mode::Hidden));
        }
        Message::Noop => {}
    };
    Task::none()
}

pub fn view(state: &State) -> Element<Message> {
    match &state.screen {
        Screen::Search => main_view(&state.query, &state.search_results),
        Screen::PluginView => plugin_view(&state.search_results),
    }
}

pub fn subscription(_state: &State) -> Subscription<Message> {
    Subscription::batch(vec![iced::event::listen().map(|event| match event {
        iced::event::Event::Window(iced::window::Event::CloseRequested) => {
            Message::Window(WindowMessage::Close)
        }
        _ => Message::Noop,
    })])
}
