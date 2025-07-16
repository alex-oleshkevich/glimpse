use iced::{
    futures::{SinkExt, channel::mpsc},
    task::Handle,
    *,
};

use crate::{
    app::{self, AppMessage},
    gui::{
        messages::{Message, SearchMessage, WindowMessage},
        widgets::{main_view, plugin_view},
    },
    search::SearchItem,
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
                self.state.to_app = Some(sender);
                Task::<Message>::none()
            }
            Message::Navigate(screen) => {
                self.state.screen = screen;
                Task::none()
            }
            Message::DispatchAction(action) => {
                println!("Executing action: {:?}", action);
                Task::none()
            }
            Message::Search(message) => match message {
                SearchMessage::StartSearch(title) => {
                    if let Some(handle) = self.state.current_search.take() {
                        tracing::debug!("aborted previous search: {}", self.state.query.clone());
                        handle.abort();
                    }
                    if let None = self.state.to_app {
                        tracing::warn!("no app sender available to send search message");
                        return Task::none();
                    }

                    let new_query = title.clone();
                    let mut sender = self.state.to_app.clone().unwrap();
                    let (task, handle) = Task::abortable(
                        //
                        Task::future(async move {
                            sender.send(AppMessage::Search(new_query)).await.ok();
                        }),
                    );

                    self.state.query = title.clone();
                    self.state.current_search = Some(handle);
                    task.map(|_| Message::Noop)
                }
                SearchMessage::ClearResults => {
                    Task::done(Message::Search(SearchMessage::SetResults(vec![])))
                }
                SearchMessage::SetResults(results) => {
                    self.state.search_results = results;
                    tracing::debug!(
                        "search results updated: {}",
                        self.state.search_results.len()
                    );
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
            Subscription::run(app::connect).map(|message| match message {
                AppMessage::Bootstrap(sender) => Message::AppBootstrapped(sender),
                AppMessage::SearchCompleted(results) => {
                    Message::Search(SearchMessage::SetResults(results))
                }
                _ => Message::Noop,
            }),
        ])
    }
}
