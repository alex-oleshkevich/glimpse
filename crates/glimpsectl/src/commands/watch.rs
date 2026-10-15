use anyhow::Result;
use zbus::{Connection, export::futures_core::Stream};

use super::within;
use crate::{errors::Gone, render};

pub(super) async fn watch<F, R>(
    connection: &Connection,
    name: &str,
    path: &str,
    interface: &str,
    mut snapshot: F,
) -> Result<()>
where
    F: FnMut(zbus::names::OwnedUniqueName) -> R,
    R: Future<Output = Result<String>>,
{
    let dbus = within(zbus::fdo::DBusProxy::new(connection)).await?;
    let mut owners =
        Box::pin(within(dbus.receive_name_owner_changed_with_args(&[(0, name)])).await?);
    let owner = within(dbus.get_name_owner(name.try_into()?)).await?;
    let properties = within(
        zbus::fdo::PropertiesProxy::builder(connection)
            .destination(owner.clone())?
            .path(path)?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build(),
    )
    .await?;
    let mut changes = Box::pin(within(properties.receive_properties_changed()).await?);
    let followed = owner.clone();
    let mut lost = Box::pin(async move {
        loop {
            let Some(change) = std::future::poll_fn(|cx| owners.as_mut().poll_next(cx)).await
            else {
                return Err(Gone.into());
            };
            if change
                .args()?
                .old_owner()
                .as_ref()
                .is_some_and(|old| old.as_str() == followed.as_str())
            {
                return Err(Gone.into());
            }
        }
    });
    loop {
        let text = tokio::select! {
            biased;
            result = &mut lost => return result,
            text = snapshot(owner.clone()) => text?,
        };
        if let render::Flow::Stop = render::print(&text)? {
            return Ok(());
        }
        loop {
            tokio::select! {
                biased;
                result = &mut lost => return result,
                change = std::future::poll_fn(|cx| changes.as_mut().poll_next(cx)) => {
                    let Some(change) = change else { return Err(Gone.into()) };
                    if change.args()?.interface_name().as_str() == interface { break; }
                }
            }
        }
    }
}
