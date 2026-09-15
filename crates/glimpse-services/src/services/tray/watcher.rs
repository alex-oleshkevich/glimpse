use std::sync::{Arc, Mutex};

use glimpse_dbus::status_notifier_item::StatusNotifierItemProxy;
use glimpse_dbus::status_notifier_watcher::{
    DEFAULT_ITEM_PATH, Registry, StatusNotifierWatcherProxy, WATCHER_ALIAS, WATCHER_NAME,
    WATCHER_PATH,
};
use tokio::sync::mpsc;
use zbus::Connection;
use zbus::object_server::SignalEmitter;

use crate::context::Ctx;
use crate::service::Input;

use super::{Event, Tray};

pub type Shared = Arc<Mutex<Registry>>;

/// The object server calls these from its own task, so the registry is shared rather than owned by
/// the service: a property read has to answer without waiting for the handler queue. Every critical
/// section is a single statement — a `std::sync::Mutex` held across an `.await` would freeze the
/// bus task.
#[derive(Clone)]
pub struct Watcher {
    registry: Shared,
    events: mpsc::Sender<Input<Tray>>,
}

impl Watcher {
    pub fn new(registry: Shared, events: mpsc::Sender<Input<Tray>>) -> Self {
        Self { registry, events }
    }

    fn announce(&self, event: Event) {
        let _ = self.events.try_send(Input::Event(event));
    }
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher", spawn = false)]
impl Watcher {
    async fn register_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let registered = self
            .registry
            .lock()
            .map_err(|_| zbus::fdo::Error::Failed("the tray registry is poisoned".to_owned()))?
            .register(&sender, service);
        if let Some(key) = registered {
            tracing::debug!(%key, "tray item registered");
            self.announce(Event::Registered(key.clone()));
            Self::status_notifier_item_registered(&emitter, &key).await?;
        }
        Ok(())
    }

    async fn register_status_notifier_host(
        &self,
        service: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let fresh = self
            .registry
            .lock()
            .map_err(|_| zbus::fdo::Error::Failed("the tray registry is poisoned".to_owned()))?
            .register_host(&sender);
        if fresh {
            tracing::info!(host = service, "status notifier host registered");
            Self::status_notifier_host_registered(&emitter).await?;
        }
        Ok(())
    }

    async fn unregister_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let removed = self
            .registry
            .lock()
            .map_err(|_| zbus::fdo::Error::Failed("the tray registry is poisoned".to_owned()))?
            .unregister(&sender, service);
        if let Some(key) = removed {
            self.announce(Event::Unregistered(key.clone()));
            Self::status_notifier_item_unregistered(&emitter, &key).await?;
        }
        Ok(())
    }

    #[zbus(property)]
    async fn registered_status_notifier_items(&self) -> Vec<String> {
        self.registry
            .lock()
            .map(|registry| registry.items().to_vec())
            .unwrap_or_default()
    }

    #[zbus(property)]
    async fn is_status_notifier_host_registered(&self) -> bool {
        self.registry
            .lock()
            .map(|registry| registry.host_registered())
            .unwrap_or_default()
    }

    #[zbus(property)]
    async fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(signal)]
    async fn status_notifier_item_registered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_item_unregistered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_registered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_unregistered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

/// The freedesktop spelling, which a minority of toolkits look for instead. It answers reads and
/// registrations; the signals are on the KDE interface, which is what every such toolkit follows.
#[derive(Clone)]
pub struct WatcherAlias {
    registry: Shared,
    events: mpsc::Sender<Input<Tray>>,
}

impl WatcherAlias {
    pub fn new(registry: Shared, events: mpsc::Sender<Input<Tray>>) -> Self {
        Self { registry, events }
    }
}

#[zbus::interface(name = "org.freedesktop.StatusNotifierWatcher", spawn = false)]
impl WatcherAlias {
    async fn register_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let registered = self
            .registry
            .lock()
            .map_err(|_| zbus::fdo::Error::Failed("the tray registry is poisoned".to_owned()))?
            .register(&sender, service);
        if let Some(key) = registered {
            let _ = self.events.try_send(Input::Event(Event::Registered(key)));
        }
        Ok(())
    }

    async fn register_status_notifier_host(&self, _service: &str) -> zbus::fdo::Result<()> {
        Ok(())
    }

    #[zbus(property)]
    async fn registered_status_notifier_items(&self) -> Vec<String> {
        self.registry
            .lock()
            .map(|registry| registry.items().to_vec())
            .unwrap_or_default()
    }

    #[zbus(property)]
    async fn is_status_notifier_host_registered(&self) -> bool {
        self.registry
            .lock()
            .map(|registry| registry.host_registered())
            .unwrap_or_default()
    }

    #[zbus(property)]
    async fn protocol_version(&self) -> i32 {
        0
    }
}

async fn owned_by_us(connection: &Connection) -> bool {
    let Ok(proxy) = zbus::fdo::DBusProxy::new(connection).await else {
        return false;
    };
    let Ok(target) = zbus::names::BusName::try_from(WATCHER_NAME) else {
        return false;
    };
    let Ok(owner) = proxy.get_name_owner(target).await else {
        return false;
    };
    connection
        .unique_name()
        .is_some_and(|ours| ours.as_str() == owner.as_str())
}

/// Who holds a name, named well enough to act on. "Another watcher owns the name" costs an hour of
/// someone's evening; `:1.42 (pid 8123, plasmashell)` costs one extra call.
pub async fn holder(connection: &Connection, name: &str) -> String {
    let Ok(proxy) = zbus::fdo::DBusProxy::new(connection).await else {
        return name.to_owned();
    };
    let Ok(target) = zbus::names::BusName::try_from(name) else {
        return name.to_owned();
    };
    let Ok(owner) = proxy.get_name_owner(target).await else {
        return name.to_owned();
    };
    let Ok(pid) = proxy
        .get_connection_unix_process_id(owner.clone().into())
        .await
    else {
        return format!("{owner}");
    };
    match tokio::fs::read_to_string(format!("/proc/{pid}/comm")).await {
        Ok(comm) => format!("{owner} (pid {pid}, {})", comm.trim()),
        Err(_) => format!("{owner} (pid {pid})"),
    }
}

/// Put the objects up, take the name, then register as a host. The order matters at both ends: a
/// `Get` arriving between the name and the object would find the name with nothing behind it, and a
/// refused name has to take the objects down again or a later claim cannot export them.
pub async fn claim(ctx: &Ctx<Tray>, registry: &Shared) -> Result<(), String> {
    let connection = ctx.session_bus().map_err(str::to_owned)?.clone();
    let server = connection.object_server();

    server
        .at(WATCHER_PATH, Watcher::new(registry.clone(), ctx.events()))
        .await
        .map_err(|error| error.to_string())?;
    server
        .at(
            WATCHER_PATH,
            WatcherAlias::new(registry.clone(), ctx.events()),
        )
        .await
        .map_err(|error| error.to_string())?;

    if let Err(error) = glimpse_dbus::own_name(&connection, WATCHER_NAME).await {
        // Taken *by us* is success, not a conflict. A second claim can overlap the first while its
        // outcome is still in the inbox, and reporting ourselves as the blocker would degrade a
        // watcher that is working perfectly well.
        if matches!(error, zbus::Error::NameTaken) && owned_by_us(&connection).await {
            tracing::debug!(name = WATCHER_NAME, "already ours; keeping it");
            return Ok(());
        }
        let _ = server.remove::<WatcherAlias, _>(WATCHER_PATH).await;
        let _ = server.remove::<Watcher, _>(WATCHER_PATH).await;
        return Err(match error {
            zbus::Error::NameTaken => {
                format!(
                    "{WATCHER_NAME} is owned by {}",
                    holder(&connection, WATCHER_NAME).await
                )
            }
            error => error.to_string(),
        });
    }
    tracing::info!(name = WATCHER_NAME, "the tray watcher name is ours");

    // A toolkit that only knows the freedesktop spelling looks up that *name*. Exporting the
    // interface without owning the name leaves it unreachable by the very clients it is for.
    // Losing it is not fatal: the KDE name is what almost everything uses.
    if let Err(error) = glimpse_dbus::own_name(&connection, WATCHER_ALIAS).await {
        tracing::debug!(name = WATCHER_ALIAS, %error, "the freedesktop alias name is not ours");
    }

    register_host(&connection).await;
    Ok(())
}

/// Applications check `IsStatusNotifierHostRegistered` and fall back to XEmbed while it is false,
/// so a watcher that never registers a host collects items nothing ever shows.
async fn register_host(connection: &Connection) {
    let name = format!("org.kde.StatusNotifierHost-{}", std::process::id());
    if let Err(error) = glimpse_dbus::own_name(connection, &name).await {
        tracing::warn!(%name, %error, "could not take the host name");
        return;
    }
    match StatusNotifierWatcherProxy::new(connection).await {
        Ok(watcher) => {
            if let Err(error) = watcher.register_status_notifier_host(&name).await {
                tracing::warn!(%name, %error, "the watcher refused our host registration");
            }
        }
        Err(error) => tracing::warn!(%error, "could not reach the watcher we just claimed"),
    }
}

/// Items register once, at their own startup. A panel restart empties the registry with no error
/// and no way for an item to know, so a fresh owner has to go and find them again.
///
/// Announce **before** sweeping and let the registry dedupe: a well-behaved application
/// re-registers the moment it sees the signal, and a sweep racing that would add the same item
/// twice under two spellings if the key were not canonical.
pub async fn sweep(ctx: &Ctx<Tray>, registry: &Shared) {
    let Ok(connection) = ctx.session_bus() else {
        return;
    };
    announce_host(connection).await;

    let Ok(proxy) = zbus::fdo::DBusProxy::new(connection).await else {
        return;
    };
    let Ok(names) = proxy.list_names().await else {
        return;
    };

    for name in names {
        let name = name.as_str();
        if !name.starts_with("org.kde.StatusNotifierItem-")
            && !name.starts_with("org.freedesktop.StatusNotifierItem-")
        {
            continue;
        }
        let Ok(target) = zbus::names::BusName::try_from(name) else {
            continue;
        };
        let Ok(owner) = proxy.get_name_owner(target).await else {
            continue;
        };
        let owner = owner.as_str();
        if !responds(connection, owner).await {
            continue;
        }
        let registered = registry
            .lock()
            .ok()
            .and_then(|mut registry| registry.register(owner, DEFAULT_ITEM_PATH));
        if let Some(key) = registered {
            tracing::info!(%key, "adopted a tray item that registered before we held the name");
        }
    }
}

/// A well-known name in the range proves nothing on its own — the owner may be a stale activatable
/// entry. Reading one property is what says an item is really there.
async fn responds(connection: &Connection, owner: &str) -> bool {
    let Ok(item) = StatusNotifierItemProxy::builder(connection)
        .destination(owner.to_owned())
        .and_then(|builder| builder.path(DEFAULT_ITEM_PATH))
        .map(|builder| builder.cache_properties(zbus::proxy::CacheProperties::No))
    else {
        return false;
    };
    match item.build().await {
        Ok(item) => item.status().await.is_ok() || item.id().await.is_ok(),
        Err(_) => false,
    }
}

async fn announce_host(connection: &Connection) {
    let server = connection.object_server();
    let Ok(reference) = server.interface::<_, Watcher>(WATCHER_PATH).await else {
        return;
    };
    if let Err(error) = Watcher::status_notifier_host_registered(reference.signal_emitter()).await {
        tracing::warn!(%error, "could not announce the host registration");
    }
}

/// Every `NameOwnerChanged`, which is one match rule rather than one per item — **and the first
/// claim, made from in here**.
///
/// The ordering is the whole point, and `Service::start` cannot supply it. The runtime installs
/// this source only *after* `start` returns, so an incumbent that releases the name in that gap
/// emits a signal nobody is listening for; `own_name` asks `DoNotQueue`, so the failed claim is
/// terminal, and the tray sits degraded over an empty bar until the panel is restarted again —
/// precisely the panel-restart case the retry exists for. Establishing the match rule first and
/// claiming second closes the window.
pub async fn name_changes(
    ctx: Ctx<Tray>,
) -> std::pin::Pin<Box<dyn futures_util::Stream<Item = Event> + Send>> {
    // `Event::Claim` is emitted even when there is no bus to claim on: it is what makes the
    // service report *why* it is degraded. Returning an empty stream first left the tray sitting
    // at `Running` over a bar that could never fill.
    let claim = futures_util::stream::once(async { Event::Claim });
    let Ok(connection) = ctx.session_bus() else {
        return Box::pin(claim);
    };
    let Ok(proxy) = zbus::fdo::DBusProxy::new(connection).await else {
        return Box::pin(claim);
    };
    let Ok(stream) = proxy.receive_name_owner_changed().await else {
        return Box::pin(claim);
    };

    let changes = futures_util::StreamExt::filter_map(stream, |signal| async move {
        let args = signal.args().ok()?;
        Some(Event::NameOwnerChanged {
            name: args.name().to_string(),
            new_owner: args.new_owner().as_ref().map(ToString::to_string),
        })
    });
    Box::pin(futures_util::StreamExt::chain(claim, changes))
}
