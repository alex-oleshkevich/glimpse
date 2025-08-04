use std::sync::Arc;

use glimpse_sdk::{Command, Request, Response};
use iced::futures::{SinkExt, Stream};
use iced::{Element, widget};
use iced::{Subscription, Task, stream, window};
use tokio::sync::{Mutex, mpsc};

use crate::components::{main_view, plugin_view};
use crate::dbus::setup_dbus_service;
use crate::messages::{Message, Screen};

pub struct State {
    pub query: String,
    pub window_id: Option<window::Id>,
    pub search_items: Vec<Command>,
    pub screen: Screen,
}

impl Default for State {
    fn default() -> Self {
        State {
            query: String::new(),
            search_items: Vec::new(),
            screen: Screen::MainView,
            window_id: None,
        }
    }
}

pub struct App {
    state: State,
    from_daemon_rx: Arc<Mutex<mpsc::Receiver<Message>>>,
    to_daemon_tx: mpsc::Sender<Message>,
}

impl App {
    pub fn new(
        from_daemon_rx: mpsc::Receiver<Message>,
        to_daemon_tx: mpsc::Sender<Message>,
    ) -> (Self, Task<Message>) {
        (
            App {
                state: State::default(),
                from_daemon_rx: Arc::new(Mutex::new(from_daemon_rx)),
                to_daemon_tx,
            },
            Task::done(Message::OpenWindow),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenWindow => {
                let mut settings = window::Settings::default();
                settings.decorations = false;
                settings.level = window::Level::AlwaysOnTop;
                settings.resizable = false;
                settings.size = iced::Size::new(700.0, 500.0);

                let (id, task) = window::open(settings);
                task.map(move |_| Message::WindowOpened(id))
            }
            Message::CloseWindow => {
                if self.state.window_id.is_none() {
                    return Task::none();
                }
                let id = self.state.window_id.unwrap();
                window::close(id)
            }
            Message::Navigate(screen) => {
                self.state.screen = screen;
                Task::none()
            }
            Message::WindowOpened(id) => {
                self.state.window_id = Some(id);
                Task::batch([widget::focus_next()])
            }
            Message::Search(query) => {
                self.state.query = query;
                let query = self.state.query.clone();
                let sender = self.to_daemon_tx.clone();
                Task::perform(
                    async move {
                        sender
                            .send(Message::DispatchRequest(Request::Search {
                                query: query.clone(),
                            }))
                            .await
                            .ok();
                    },
                    |_| Message::Nothing,
                )
            }
            Message::ClearSearch => {
                self.state.query.clear();
                self.state.search_items.clear();
                Task::batch([widget::focus_next()])
            }
            Message::DaemonResponse {
                request_id: _,
                plugin_id: _,
                response,
            } => {
                match response {
                    Response::SearchResults(items) => {
                        self.state.search_items = items;
                        tracing::debug!("search results: {:?}", self.state.search_items);
                    }
                    _ => {}
                }
                Task::none()
            }
            Message::EscapePressed => {
                if self.state.query.is_empty() {
                    return Task::done(Message::CloseWindow);
                }
                return Task::done(Message::ClearSearch);
            }
            _ => Task::none(),
        }
    }

    pub fn view(&self, _window_id: window::Id) -> Element<Message> {
        match self.state.screen {
            Screen::MainView => main_view(&self.state),
            Screen::PluginView => plugin_view(&self.state.search_items),
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let from_daemon_rx = Arc::clone(&self.from_daemon_rx);
        Subscription::batch(vec![
            iced::event::listen().map(|event| match event {
                iced::event::Event::Keyboard(iced::keyboard::Event::KeyReleased {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    ..
                }) => Message::EscapePressed,
                _ => Message::Nothing,
            }),
            Subscription::run_with_id("daemon_connection", connect_daemon(from_daemon_rx)).map(
                |message| match message {
                    Message::DaemonResponse {
                        request_id,
                        plugin_id,
                        response,
                    } => Message::DaemonResponse {
                        request_id,
                        plugin_id,
                        response,
                    },
                    _ => Message::Nothing,
                },
            ),
            Subscription::run_with_id("dbus", connect_dbus()).map(|message| match message {
                Message::OpenWindow => Message::OpenWindow,
                Message::CloseWindow => Message::CloseWindow,
                _ => Message::Nothing,
            }),
        ])
    }
}

fn connect_daemon(
    from_daemon_rx: Arc<Mutex<mpsc::Receiver<Message>>>,
) -> impl Stream<Item = Message> {
    stream::channel(100, |mut output| async move {
        use iced::futures::SinkExt;

        tokio::spawn(async move {
            while let Some(input) = from_daemon_rx.lock().await.recv().await {
                tracing::debug!("forwarding message app -> ui stream: {:?}", input);
                output.send(input).await.ok();
            }
        });
    })
}

fn connect_dbus() -> impl Stream<Item = Message> {
    stream::channel(100, move |mut output| async move {
        let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Some(message) => {
                        tracing::debug!("forwarding message dbus -> ui stream: {:?}", message);
                        output.send(message).await.ok();
                    }
                    None => break,
                }
            }
        });

        if let Err(e) = setup_dbus_service(tx).await {
            tracing::error!("failed to setup DBus service: {}", e);
        }
    })
}
