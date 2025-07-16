use iced::futures::SinkExt;
use iced::futures::{Stream, StreamExt, channel::mpsc};
use iced::stream;

use crate::search::SearchItem;
use crate::{extensions::ExtensionManager, search::Search};

#[derive(Debug, Clone)]
pub enum AppMessage {
    Bootstrap(mpsc::Sender<AppMessage>),
    Search(String),
    SearchCompleted(Vec<SearchItem>),
}

pub fn connect() -> impl Stream<Item = AppMessage> {
    stream::channel(100, |mut output| async move {
        let (sender, mut receiver) = mpsc::channel(100);
        let _ = output.send(AppMessage::Bootstrap(sender)).await;

        let mut extensions = ExtensionManager::new();
        extensions.load_extensions();
        let search = Search::new();

        loop {
            let input = receiver.select_next_some().await;
            match input {
                AppMessage::Search(query) => {
                    tracing::info!("searching for: {}", query);
                    let results = search.search(query).await;
                    tracing::debug!("found {} results", results.len());
                    let _ = output.send(AppMessage::SearchCompleted(results)).await;
                }
                _ => {}
            }
        }
    })
}
