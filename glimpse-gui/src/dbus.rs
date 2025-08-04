use std::{error::Error, future::pending};

use tokio::sync::mpsc;
use zbus::{connection, interface, proxy};

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

    pending::<()>().await;

    tracing::debug!("DBus service registered at /me/aresa/Glimpse");

    loop {
        pending::<()>().await;
    }
}

#[proxy(
    interface = "me.aresa.Glimpse",
    default_service = "me.aresa.Glimpse",
    default_path = "/me/aresa/Glimpse"
)]
trait GlimpseClient {
    async fn show(&self) -> zbus::Result<()>;
    async fn hide(&self) -> zbus::Result<()>;
    async fn ping(&self) -> zbus::Result<String>;
}

pub async fn activate_instance() -> Result<(), Box<dyn Error>> {
    let conn = connection::Builder::session()?.build().await?;

    let proxy = GlimpseClientProxy::builder(&conn)
        .destination("me.aresa.Glimpse")?
        .path("/me/aresa/Glimpse")?
        .build()
        .await?;

    proxy.show().await?;
    Ok(())
}

pub async fn is_running() -> Result<bool, Box<dyn Error>> {
    let conn = connection::Builder::session()?.build().await?;

    let proxy = GlimpseClientProxy::builder(&conn)
        .destination("me.aresa.Glimpse")?
        .path("/me/aresa/Glimpse")?
        .build()
        .await?;

    match proxy.ping().await {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}
