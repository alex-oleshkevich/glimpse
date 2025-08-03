use std::sync::Arc;

use iced::futures::Stream;
use iced::{
    Subscription, Task, stream,
    widget::{Column, Text},
    window,
};
use tokio::sync::{Mutex, mpsc};

use crate::messages::Message;

struct State {}

impl Default for State {
    fn default() -> Self {
        State {}
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
            _ => Task::none(),
        }
    }

    pub fn view(&self, _window_id: window::Id) -> Column<Message> {
        Column::new()
            .push(Text::new("Welcome to Glimpse GUI!"))
            .padding(20)
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let from_daemon_rx = Arc::clone(&self.from_daemon_rx);
        Subscription::batch(vec![
            iced::event::listen().map(|event| match event {
                // iced::event::Event::Window(iced::window::Event::CloseRequested) => {
                //     Message::Window(WindowMessage::Close)
                // }
                _ => Message::Nothing,
            }),
            Subscription::run_with_id("daemon_connection", connect(from_daemon_rx)).map(
                |message| match message {
                    _ => Message::Nothing,
                },
            ),
        ])
        // event::listen().map(|event| match event {
        //     Event::Keyboard(keyboard::Event::KeyReleased {
        //         key: Key::Named(Named::F8),
        //         ..
        //     }) => Message::OpenWindow,
        //     _ => Message::Nothing,
        // })
    }
}

fn connect(from_daemon_rx: Arc<Mutex<mpsc::Receiver<Message>>>) -> impl Stream<Item = Message> {
    stream::channel(100, |mut output| async move {
        use iced::futures::SinkExt;

        tokio::spawn(async move {
            while let Some(input) = from_daemon_rx.lock().await.recv().await {
                tracing::debug!("forwarding message app -> ui: {:?}", input);
                output.send(input).await.ok();
            }
        });
    })
}
