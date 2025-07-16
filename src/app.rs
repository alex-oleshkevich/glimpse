use crate::{extensions::ExtensionManager, search::Search};

pub enum AppMessage {}

#[derive(Debug)]
pub struct AppChannel {
    pub sender: iced::futures::channel::mpsc::Sender<AppMessage>,
    pub receiver: iced::futures::channel::mpsc::Receiver<AppMessage>,
}

impl AppChannel {
    pub fn new() -> Self {
        let (sender, receiver) = iced::futures::channel::mpsc::channel(100);
        AppChannel { sender, receiver }
    }
}

pub struct App {
    extensions: ExtensionManager,
    search: Search,
    pub channel: AppChannel,
}

impl App {
    pub fn new() -> Self {
        App {
            extensions: ExtensionManager::new(),
            search: Search::new(),
            channel: AppChannel::new(),
        }
    }

    pub fn initialize(&mut self) {
        self.extensions.load_extensions();
    }
}
