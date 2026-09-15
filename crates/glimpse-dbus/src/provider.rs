use std::future::Future;
use std::marker::PhantomData;

use tokio::task::JoinHandle;
use zbus::Connection;
use zbus::object_server::Interface;

use crate::own_name;

pub struct Exported<I: Interface> {
    connection: Connection,
    name: &'static str,
    path: &'static str,
    changes: JoinHandle<()>,
    interface: PhantomData<fn() -> I>,
}

impl<I: Interface> Exported<I> {
    pub async fn start(
        connection: Connection,
        name: &'static str,
        path: &'static str,
        interface: I,
        follow: impl Future<Output = ()> + Send + 'static,
    ) -> zbus::Result<Self> {
        connection.object_server().at(path, interface).await?;
        if let Err(error) = own_name(&connection, name).await {
            let _ = connection.object_server().remove::<I, _>(path).await;
            return Err(error);
        }
        tracing::info!(bus_name = name, "provider D-Bus name acquired");

        Ok(Self {
            connection,
            name,
            path,
            changes: tokio::spawn(follow),
            interface: PhantomData,
        })
    }

    pub fn cancel(&self) {
        self.changes.abort();
    }

    pub async fn shutdown(self) {
        let Self {
            connection,
            name,
            path,
            changes,
            ..
        } = self;
        changes.abort();
        let _ = changes.await;
        if let Err(error) = connection.release_name(name).await {
            tracing::warn!(%error, bus_name = name, "provider D-Bus name release failed");
        }
        if let Err(error) = connection.object_server().remove::<I, _>(path).await {
            tracing::warn!(%error, bus_name = name, "provider D-Bus object removal failed");
        }
        tracing::info!(bus_name = name, "provider stopped");
    }
}
