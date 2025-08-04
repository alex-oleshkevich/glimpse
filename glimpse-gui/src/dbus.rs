use std::{error::Error, future::pending};

use tokio::sync::mpsc;
use zbus::{connection, interface};

use crate::messages::Message;

struct GlimpseService {
    command_sender: mpsc::UnboundedSender<Message>,
}

#[interface(name = "me.aresa.Glimpse")]
impl GlimpseService {
    async fn show(&self) {
        if self.command_sender.send(Message::OpenWindow).is_err() {
            tracing::error!("failed to send OpenWindow message");
        }
    }

    async fn hide(&self) {
        if self.command_sender.send(Message::CloseWindow).is_err() {
            tracing::error!("failed to send CloseWindow message");
        }
    }

    async fn ping(&self) -> String {
        "pong".to_string()
    }
}

pub async fn setup_dbus_service(
    command_sender: mpsc::UnboundedSender<Message>,
) -> Result<(), Box<dyn Error>> {
    let service = GlimpseService { command_sender };
    let _conn = connection::Builder::session()?
        .name("me.aresa.Glimpse")?
        .serve_at("/me/aresa/Glimpse", service)?
        .build()
        .await?;

    // Do other things or go to wait forever
    pending::<()>().await;

    tracing::debug!("DBus service registered at /me/aresa/Glimpse");

    loop {
        pending::<()>().await;
    }
}
