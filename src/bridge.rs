use iced::futures::Stream;
use iced::stream;
use tokio::sync::mpsc;

use crate::app::{App, AppMessage};

pub fn connect() -> impl Stream<Item = AppMessage> {
    stream::channel(100, |mut output| async move {
        use iced::futures::SinkExt;

        let (to_ui, mut from_app) = mpsc::channel::<AppMessage>(1);
        let (to_app, from_ui) = mpsc::channel::<AppMessage>(1);
        let _ = output.send(AppMessage::Bootstrap(to_app)).await;

        tokio::spawn(async move {
            while let Some(input) = from_app.recv().await {
                tracing::debug!("forwarding message app -> ui: {:?}", input);
                output.send(input).await.ok();
            }
        });

        let app = App::new();
        app.run(to_ui, from_ui).await;
        tracing::debug!("app run completed");
    })
}
