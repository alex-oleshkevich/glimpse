use std::collections::HashMap;

use zbus::proxy::CacheProperties;
use zbus::{Connection, Result};

use glimpse_dbus::udisks2::{DriveProxy, FilesystemProxy};

pub async fn drive(connection: &Connection, path: &str) -> Result<DriveProxy<'static>> {
    DriveProxy::builder(connection)
        .path(path.to_owned())?
        .cache_properties(CacheProperties::No)
        .build()
        .await
}

pub async fn filesystem(connection: &Connection, path: &str) -> Result<FilesystemProxy<'static>> {
    FilesystemProxy::builder(connection)
        .path(path.to_owned())?
        .cache_properties(CacheProperties::No)
        .build()
        .await
}

pub async fn mount(connection: &Connection, path: &str) -> Result<()> {
    filesystem(connection, path)
        .await?
        .mount(HashMap::new())
        .await
        .map(|_| ())
}

pub async fn unmount(connection: &Connection, path: &str) -> Result<()> {
    filesystem(connection, path)
        .await?
        .unmount(HashMap::new())
        .await
}

pub async fn eject(connection: &Connection, path: &str) -> Result<()> {
    drive(connection, path).await?.eject(HashMap::new()).await
}

pub async fn power_off(connection: &Connection, path: &str) -> Result<()> {
    drive(connection, path)
        .await?
        .power_off(HashMap::new())
        .await
}
