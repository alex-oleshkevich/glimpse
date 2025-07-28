use iced::{task::Handle, *};
use tokio::sync::mpsc;

use crate::{
    app::{AppMessage, SearchItem},
    bridge,
    extensions::{Request, Response},
    gui::{
        messages::{Message, SearchMessage, WindowMessage},
        widgets::{main_view, plugin_view},
    },
};

#[derive(Debug, Clone)]
pub enum Screen {
    Search,
    PluginView,
}

#[derive(Debug)]
enum SearchState {
    Idle,
    Searching,
}

#[derive(Debug)]
pub struct State {
    query: String,
    screen: Screen,
    search_state: SearchState,
    search_results: Vec<SearchItem>,
    to_app: Option<mpsc::Sender<AppMessage>>,
    current_search: Option<Handle>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            query: String::new(),
            screen: Screen::Search,
            search_results: vec![],
            to_app: None,
            current_search: None,
            search_state: SearchState::Idle,
        }
    }
}

pub struct GuiApp {
    state: State,
}

impl GuiApp {
    pub fn new() -> (Self, Task<Message>) {
        let gui_app = Self {
            state: State::default(),
        };

        (gui_app, Task::batch([widget::focus_next()]))
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AppBootstrapped(sender) => {
                tracing::debug!("ui connected to app");
                self.state.to_app = Some(sender);
                Task::<Message>::none()
            }
            Message::Navigate(screen) => {
                self.state.screen = screen;
                Task::none()
            }
            Message::DispatchAction(action) => Task::none(),
            Message::Search(message) => match message {
                SearchMessage::StartSearch(title) => {
                    tracing::debug!("starting search for: {}", title);
                    self.state.search_state = SearchState::Searching;

                    if let Some(handle) = self.state.current_search.take() {
                        tracing::debug!("aborted previous search: {}", self.state.query.clone());
                        handle.abort();
                    }
                    if let None = self.state.to_app {
                        tracing::warn!("no app sender available to send search message");
                        return Task::none();
                    }

                    let new_query = title.clone();
                    let sender = self.state.to_app.clone().unwrap();
                    let (task, handle) = Task::abortable(
                        //
                        Task::future(async move {
                            match sender
                                .send(AppMessage::Request(Request::Search(new_query)))
                                .await
                            {
                                Ok(_) => {
                                    tracing::debug!("search message sent successfully");
                                }
                                Err(err) => {
                                    tracing::error!("failed to send search message: {}", err);
                                }
                            }
                        }),
                    );

                    self.state.query = title.clone();
                    self.state.current_search = Some(handle);
                    self.state.search_results.clear();
                    task.map(|_| Message::Noop)
                }
                SearchMessage::AddResult(item) => {
                    self.state.search_results.push(item);
                    Task::none()
                }
            },
            Message::Window(WindowMessage::Close) => {
                return iced::window::get_latest()
                    .and_then(|id| iced::window::change_mode(id, iced::window::Mode::Hidden));
            }
            Message::Noop => Task::none(),
        }
    }

    pub fn view(&self) -> Element<Message> {
        match &self.state.screen {
            Screen::Search => main_view(&self.state.query, &self.state.search_results),
            Screen::PluginView => plugin_view(&self.state.search_results),
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            iced::event::listen().map(|event| match event {
                iced::event::Event::Window(iced::window::Event::CloseRequested) => {
                    Message::Window(WindowMessage::Close)
                }
                _ => Message::Noop,
            }),
            Subscription::run(bridge::connect).map(|message| match message {
                AppMessage::Bootstrap(sender) => Message::AppBootstrapped(sender),
                AppMessage::Response(response) => match response {
                    Response::SearchItem(item) => Message::Search(SearchMessage::AddResult(item)),
                },
                _ => Message::Noop,
            }),
        ])
    }
}
