use anyhow::Context;

#[derive(Clone)]
pub struct Dbus {
    pub session: zbus::Connection,
    pub system: zbus::Connection,
}

impl Dbus {
    pub fn connect() -> anyhow::Result<Self> {
        let session = zbus::blocking::Connection::session()
            .context("failed to connect to D-Bus session bus")?
            .into();
        let system = zbus::blocking::Connection::system()
            .context("failed to connect to D-Bus system bus")?
            .into();
        tracing::info!("connected to D-Bus session and system buses");
        Ok(Self { session, system })
    }

    pub async fn connect_async() -> anyhow::Result<Self> {
        let session = zbus::Connection::session()
            .await
            .context("failed to connect to D-Bus session bus")?;
        let system = zbus::Connection::system()
            .await
            .context("failed to connect to D-Bus system bus")?;
        tracing::info!("connected to D-Bus session and system buses");
        Ok(Self { session, system })
    }
}
