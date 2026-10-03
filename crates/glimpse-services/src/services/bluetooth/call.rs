use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use zbus::fdo::ObjectManagerProxy;
use zbus::proxy::CacheProperties;
use zbus::zvariant::{ObjectPath, Value};
use zbus::{Connection, Result};

use glimpse_dbus::bluez::{self, Adapter1Proxy, Device1Proxy};

use super::{Action, Failure, classify};

const CONNECT_PROOF_TIMEOUT: Duration = Duration::from_secs(5);

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

pub async fn set_discoverable(connection: &Connection, path: &str, on: bool) -> Result<()> {
    adapter(connection, path).await?.set_discoverable(on).await
}

pub fn filter() -> HashMap<&'static str, Value<'static>> {
    HashMap::from([("Transport", Value::from("auto"))])
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
    let outcome = device(connection, path).await?.connect().await;
    finish_connect(outcome, audio_profile_connected(connection, path)).await
}

pub(super) async fn finish_connect(
    outcome: Result<()>,
    proof: impl Future<Output = Result<bool>>,
) -> Result<()> {
    let error = match outcome {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };
    if classify(Action::Connect, &error) == Err(Failure::Busy)
        && matches!(
            tokio::time::timeout(CONNECT_PROOF_TIMEOUT, proof).await,
            Ok(Ok(true))
        )
    {
        Ok(())
    } else {
        Err(error)
    }
}

pub(super) fn proves_audio_profile<'a>(
    device: &str,
    path: &str,
    interfaces: impl IntoIterator<Item = &'a str>,
) -> bool {
    path.starts_with(&format!("{device}/"))
        && interfaces
            .into_iter()
            .any(|interface| interface == bluez::MEDIA_TRANSPORT1)
}

async fn audio_profile_connected(connection: &Connection, device: &str) -> Result<bool> {
    let manager = ObjectManagerProxy::builder(connection)
        .destination(bluez::SERVICE)?
        .path(bluez::ROOT)?
        .build()
        .await?;
    let mut added = manager.receive_interfaces_added().await?;
    let objects = manager.get_managed_objects().await?;
    if objects.iter().any(|(path, interfaces)| {
        proves_audio_profile(
            device,
            path.as_str(),
            interfaces.keys().map(|name| name.as_str()),
        )
    }) {
        return Ok(true);
    }
    while let Some(signal) = added.next().await {
        let Ok(args) = signal.args() else {
            continue;
        };
        if proves_audio_profile(
            device,
            args.object_path().as_str(),
            args.interfaces_and_properties()
                .keys()
                .map(|name| name.as_str()),
        ) {
            return Ok(true);
        }
    }
    Ok(false)
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
mod tests;
