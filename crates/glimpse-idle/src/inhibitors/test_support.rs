#![cfg(test)]

use std::sync::Arc;

use glimpse_dbus::testing::PrivateBus;
use zbus::Connection;
#[derive(Default, Clone)]
pub(crate) struct FakeLogin1 {
    pub(crate) readers: Arc<std::sync::Mutex<Vec<std::io::PipeReader>>>,
    pub(crate) whats: Arc<std::sync::Mutex<Vec<String>>>,
    pause_started: Option<Arc<tokio::sync::Notify>>,
    pause_release: Option<Arc<tokio::sync::Notify>>,
}

#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl FakeLogin1 {
    async fn inhibit(
        &self,
        what: &str,
        _who: &str,
        _why: &str,
        _mode: &str,
    ) -> zbus::fdo::Result<zbus::zvariant::OwnedFd> {
        let (reader, writer) =
            std::io::pipe().map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
        self.readers.lock().unwrap().push(reader);
        self.whats.lock().unwrap().push(what.to_owned());
        if let (Some(started), Some(release)) = (&self.pause_started, &self.pause_release) {
            started.notify_one();
            release.notified().await;
        }
        let fd: std::os::fd::OwnedFd = writer.into();
        Ok(fd.into())
    }
}

pub(crate) async fn start_fake_login1(bus: &PrivateBus) -> (Connection, FakeLogin1) {
    let connection = bus.connection().await;
    let fake = FakeLogin1::default();
    start_fake_login1_with(connection, fake).await
}

pub(crate) async fn start_paused_fake_login1(
    bus: &PrivateBus,
) -> (
    Connection,
    FakeLogin1,
    Arc<tokio::sync::Notify>,
    Arc<tokio::sync::Notify>,
) {
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let fake = FakeLogin1 {
        readers: Arc::default(),
        whats: Arc::default(),
        pause_started: Some(started.clone()),
        pause_release: Some(release.clone()),
    };
    let connection = bus.connection().await;
    let (connection, fake) = start_fake_login1_with(connection, fake).await;
    (connection, fake, started, release)
}

async fn start_fake_login1_with(
    connection: Connection,
    fake: FakeLogin1,
) -> (Connection, FakeLogin1) {
    connection
        .object_server()
        .at("/org/freedesktop/login1", fake.clone())
        .await
        .unwrap();
    glimpse_dbus::own_name(&connection, "org.freedesktop.login1")
        .await
        .unwrap();
    (connection, fake)
}
