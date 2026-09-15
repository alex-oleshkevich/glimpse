use std::future::Future;
use std::marker::PhantomData;

use tokio::sync::watch;
use tokio_util::task::AbortOnDropHandle;
use zbus::Connection;
use zbus::object_server::{Interface, SignalEmitter};

use crate::own_name;

/// A provider interface whose whole public surface is one `snapshot` property. Implementing it is
/// three lines forwarding to the `snapshot_changed` zbus generates, and it is what lets `Exported`
/// own the re-emit loop instead of every provider writing it again.
pub trait Snapshot: Interface {
    fn emit_snapshot_changed(
        &self,
        emitter: &SignalEmitter<'_>,
    ) -> impl Future<Output = zbus::Result<()>> + Send;
}

pub struct Exported<I: Interface> {
    connection: Connection,
    name: &'static str,
    path: &'static str,
    changes: AbortOnDropHandle<()>,
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
            changes: AbortOnDropHandle::new(tokio::spawn(follow)),
            interface: PhantomData,
        })
    }

    /// Re-emit `snapshot` whenever the service's state or health moves, until either sender is
    /// dropped. The two receivers are opaque here: this crate never interprets a service's state.
    async fn follow<T, H>(
        connection: Connection,
        path: &'static str,
        mut state: watch::Receiver<T>,
        mut health: watch::Receiver<H>,
    ) where
        I: Snapshot,
        T: Send + Sync + 'static,
        H: Send + Sync + 'static,
    {
        loop {
            tokio::select! {
                changed = state.changed() => if changed.is_err() { return },
                changed = health.changed() => if changed.is_err() { return },
            }

            let interface = match connection.object_server().interface::<_, I>(path).await {
                Ok(interface) => interface,
                Err(error) => {
                    tracing::error!(%error, path, "provider object disappeared");
                    return;
                }
            };
            if let Err(error) = interface
                .get()
                .await
                .emit_snapshot_changed(interface.signal_emitter())
                .await
            {
                tracing::warn!(%error, path, "provider snapshot change signal failed");
            }
        }
    }

    pub async fn serve<T, H>(
        connection: Connection,
        name: &'static str,
        path: &'static str,
        interface: I,
        state: watch::Receiver<T>,
        health: watch::Receiver<H>,
    ) -> zbus::Result<Self>
    where
        I: Snapshot,
        T: Send + Sync + 'static,
        H: Send + Sync + 'static,
    {
        let follow = Self::follow(connection.clone(), path, state, health);
        Self::start(connection, name, path, interface, follow).await
    }

    pub async fn shutdown(self) {
        let Self {
            connection,
            name,
            path,
            changes,
            ..
        } = self;
        drop(changes);
        if let Err(error) = connection.release_name(name).await {
            tracing::warn!(%error, bus_name = name, "provider D-Bus name release failed");
        }
        if let Err(error) = connection.object_server().remove::<I, _>(path).await {
            tracing::warn!(%error, bus_name = name, "provider D-Bus object removal failed");
        }
        tracing::info!(bus_name = name, "provider stopped");
    }
}
