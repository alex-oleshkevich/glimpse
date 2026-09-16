use std::collections::HashMap;

use zbus::proxy::CacheProperties;
use zbus::zvariant::{ObjectPath, Value};
use zbus::{Connection, Result};

use glimpse_dbus::bluez::{Adapter1Proxy, Device1Proxy};

pub async fn adapter(connection: &Connection, path: &str) -> Result<Adapter1Proxy<'static>> {
    Adapter1Proxy::builder(connection)
        .path(path.to_owned())?
        .cache_properties(CacheProperties::No)
        .build()
        .await
}

pub async fn device(connection: &Connection, path: &str) -> Result<Device1Proxy<'static>> {
    Device1Proxy::builder(connection)
        .path(path.to_owned())?
        .cache_properties(CacheProperties::No)
        .build()
        .await
}

pub async fn set_powered(connection: &Connection, path: &str, powered: bool) -> Result<()> {
    adapter(connection, path).await?.set_powered(powered).await
}

pub fn filter() -> HashMap<&'static str, Value<'static>> {
    HashMap::from([("Transport", Value::from("auto"))]
    )
}

pub async fn start_scan(connection: &Connection, path: &str) -> Result<()> {
    let adapter = adapter(connection, path).await?;
    adapter.set_discovery_filter(filter()).await?;
    adapter.start_discovery().await
}

pub async fn stop_scan(connection: &Connection, path: &str) -> Result<()> {
    adapter(connection, path).await?.stop_discovery().await
}

pub async fn forget(connection: &Connection, adapter_path: &str, device: &str) -> Result<()> {
    let path = ObjectPath::try_from(device.to_owned())?;
    adapter(connection, adapter_path)
        .await?
        .remove_device(path)
        .await
}

pub async fn connect(connection: &Connection, path: &str) -> Result<()> {
    device(connection, path).await?.connect().await
}

pub async fn disconnect(connection: &Connection, path: &str) -> Result<()> {
    device(connection, path).await?.disconnect().await
}

pub async fn cancel_pairing(connection: &Connection, path: &str) -> Result<()> {
    device(connection, path).await?.cancel_pairing().await
}

pub async fn set_trusted(connection: &Connection, path: &str, trusted: bool) -> Result<()> {
    device(connection, path).await?.set_trusted(trusted).await
}

pub async fn pair(connection: &Connection, path: &str) -> Result<()> {
    device(connection, path).await?.pair().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_discovery_filter_names_the_transport_and_nothing_else() {
        let filter = filter();

        assert_eq!(filter.len(), 1);
        assert_eq!(filter.get("Transport"), Some(&Value::from("auto")));
    }
}
