use std::collections::HashSet;
use std::future::Future;
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use glimpse_dbus::Delivery;
use relm4::Sender;

use crate::ask::Input;

pub async fn spawn_agents(
    system: zbus::Connection,
    input: Sender<Input>,
    cancelled: Arc<Mutex<HashSet<String>>>,
) {
    drop(tokio::spawn(crate::polkit::agent::serve(
        system.clone(),
        input.clone(),
        cancelled,
    )));
    drop(tokio::spawn(crate::bluetooth::spawn(
        system.clone(),
        input.clone(),
    )));
    drop(tokio::spawn(crate::network::start(system, input)));
}

pub async fn on_owner<F, R>(system: &zbus::Connection, name: &'static str, mut register: F)
where
    F: FnMut() -> R,
    R: Future<Output = ()>,
{
    let Ok(dbus) = zbus::fdo::DBusProxy::new(system).await else {
        tracing::warn!(name, "cannot watch the bus");
        return;
    };
    let owners = dbus
        .receive_name_owner_changed_with_args(&[(0, name)])
        .await
        .map(|stream| {
            glimpse_dbus::events(stream.filter_map(|signal| async move {
                Some(signal.args().ok()?.new_owner().is_some())
            }))
        });
    if has_owner(&dbus, name).await {
        register().await;
    }
    let Ok(mut owners) = owners else {
        tracing::warn!(name, "cannot watch the bus name");
        return;
    };
    while let Some(delivery) = owners.next().await {
        let present = match delivery {
            Delivery::Item(present) => present,
            Delivery::Missed => has_owner(&dbus, name).await,
        };
        if present {
            register().await;
        }
    }
}

async fn has_owner(dbus: &zbus::fdo::DBusProxy<'_>, name: &'static str) -> bool {
    let Ok(name) = zbus::names::BusName::try_from(name) else {
        return false;
    };
    dbus.name_has_owner(name).await.unwrap_or(false)
}
