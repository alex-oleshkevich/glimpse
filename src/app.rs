use crate::{extensions::ExtensionManager, search::Search};

pub enum AppMessage {}

pub struct App {
    extensions: ExtensionManager,
    search: Search,
}

impl App {
    pub fn new() -> Self {
        App {
            extensions: ExtensionManager::new(),
            search: Search::new(),
        }
    }

    pub fn initialize(&mut self) {
        self.extensions.load_extensions();
    }

    pub async fn run(&mut self) {
        // loop {
        //     tokio::select! {
        //         Some(message) = self.ui_channel.receiver().recv() => {
        //             // Handle incoming UI messages
        //         }
        //         // Other event handling can go here
        //     }
        // }
    }
}
