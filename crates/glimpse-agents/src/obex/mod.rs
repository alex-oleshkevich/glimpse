use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use gettextrs::gettext;
use glimpse_dbus::Delivery;
use tokio::sync::oneshot;
use zbus::zvariant::ObjectPath;

use glimpse_dbus::bluez::{DEVICE1, ROOT, SERVICE as BLUEZ};
use glimpse_dbus::obex::{AgentManager1Proxy, MANAGER, SERVICE, Session1Proxy, Transfer1Proxy};

mod file;
mod notify;

use notify::Notify;

pub const PATH: &str = "/me/aresa/glimpse/obex/agent";
const READ_TIMEOUT: Duration = Duration::from_secs(2);
const LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const NAME_CAP: usize = 64;

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.obex.Error")]
pub enum AgentError {
    Rejected(String),
}

#[derive(Clone)]
pub(crate) struct Config {
    pub answer_timeout: Duration,
    pub tick: Duration,
    pub idle: Duration,
    pub downloads: Option<PathBuf>,
    pub special: fn() -> Option<PathBuf>,
    pub home: PathBuf,
    pub root: PathBuf,
    pub reactivate: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            answer_timeout: Duration::from_secs(55),
            tick: Duration::from_secs(1),
            idle: Duration::from_secs(600),
            downloads: None,
            special: || glib::user_special_dir(glib::UserDirectory::Downloads),
            home: glib::home_dir(),
            root: glib::user_cache_dir().join("obexd"),
            reactivate: Duration::from_secs(2),
        }
    }
}

pub struct Agent {
    session: zbus::Connection,
    system: zbus::Connection,
    config: Config,
    pending: tokio::sync::Mutex<Option<oneshot::Sender<()>>>,
}

struct Transfer {
    name: String,
    filename: Option<String>,
    destination: String,
}

struct Device {
    paired: bool,
    name: String,
}

enum Answer {
    Accept,
    Decline,
    Closed,
    Canceled,
    Lost,
    Timeout,
}

impl Agent {
    fn new(session: zbus::Connection, system: zbus::Connection, config: Config) -> Self {
        Self {
            session,
            system,
            config,
            pending: tokio::sync::Mutex::new(None),
        }
    }

    async fn push(&self, transfer: &str) -> Result<String, AgentError> {
        let (cancel_tx, cancel_rx) = oneshot::channel();
        *self.pending.lock().await = Some(cancel_tx);
        let mut name = String::new();
        let result = self.decide(transfer, cancel_rx, &mut name).await;
        *self.pending.lock().await = None;
        match &result {
            Ok(_) => tracing::info!(file = %name, "obex: push accepted"),
            Err(AgentError::Rejected(reason)) => {
                tracing::info!(file = %name, reason = %reason, "obex: push refused");
            }
        }
        result
    }

    async fn decide(
        &self,
        transfer: &str,
        cancel_rx: oneshot::Receiver<()>,
        cleaned: &mut String,
    ) -> Result<String, AgentError> {
        if locked(&self.system).await {
            return Err(rejected("the session is locked"));
        }
        let Some(info) = self.transfer(transfer).await else {
            return Err(rejected("the transfer is unreadable"));
        };
        let Some(device) = device(&self.system, &info.destination).await else {
            return Err(rejected("the sender is not a known device"));
        };
        if !device.paired {
            return Err(rejected("the sender is not paired"));
        }
        let Some(name) = file::sanitize(&info.name) else {
            return Err(rejected("the file name is unusable"));
        };
        cleaned.clone_from(&name);
        let device = match device.name.trim().is_empty() {
            true => gettext("this device"),
            false => device.name,
        };
        let (notify, id) = self.confirm(&device, &name, cancel_rx).await?;
        let downloads = match self.downloads().await {
            Some(downloads) => downloads,
            None => {
                notify.close(id).await;
                return Err(rejected("no download directory"));
            }
        };
        let source = self.source(&info).join(&name);
        let subscription = notify::subscribe(&self.session, transfer).await;
        let receive = notify::Receive {
            source,
            downloads,
            file: name.clone(),
        };
        let follow = notify::follow(
            subscription,
            transfer.to_owned(),
            receive,
            notify,
            self.config.tick,
            self.config.idle,
        );
        drop(tokio::spawn(follow));
        Ok(name)
    }

    fn source(&self, info: &Transfer) -> PathBuf {
        let separated = info.name.contains(['/', '\\']);
        info.filename
            .as_deref()
            .filter(|_| !separated)
            .map(Path::new)
            .and_then(Path::parent)
            .filter(|parent| parent.is_absolute())
            .map_or_else(|| self.config.root.clone(), Path::to_path_buf)
    }

    async fn transfer(&self, transfer: &str) -> Option<Transfer> {
        let proxy = read(transfer, async {
            Transfer1Proxy::builder(&self.session)
                .path(transfer)?
                .build()
                .await
        })
        .await?;
        let name = read(transfer, proxy.name()).await?;
        let filename = read(transfer, proxy.filename()).await;
        let session = read(transfer, proxy.session()).await?;
        let proxy = read(transfer, async {
            Session1Proxy::builder(&self.session)
                .path(session.as_str())?
                .build()
                .await
        })
        .await?;
        let destination = read(transfer, proxy.destination()).await?;
        Some(Transfer {
            name,
            filename,
            destination,
        })
    }

    async fn downloads(&self) -> Option<PathBuf> {
        let dir = match &self.config.downloads {
            Some(dir) => dir.clone(),
            None => (self.config.special)().unwrap_or_else(|| self.config.home.join("Downloads")),
        };
        match tokio::fs::create_dir_all(&dir).await {
            Ok(()) => Some(dir),
            Err(error) => {
                tracing::debug!(%error, path = %dir.display(), "obex: cannot create the download directory");
                None
            }
        }
    }

    async fn confirm(
        &self,
        device: &str,
        name: &str,
        cancel_rx: oneshot::Receiver<()>,
    ) -> Result<(Notify, u32), AgentError> {
        let notify = match Notify::new(&self.session).await {
            Some(notify) => notify,
            None => return Err(rejected("no notification service")),
        };
        let watch = match notify.watch().await {
            Ok(watch) => watch,
            Err(error) => {
                tracing::debug!(%error, "obex: cannot watch the notification service");
                return Err(rejected("the notification service is not watchable"));
            }
        };
        let Some(id) = notify.incoming(device, name).await else {
            return Err(rejected("the notification daemon did not answer"));
        };
        let answer = self.wait(id, watch, cancel_rx).await;
        match answer {
            Answer::Accept => Ok((notify, id)),
            Answer::Decline => {
                notify.close(id).await;
                Err(rejected("the file was declined"))
            }
            Answer::Timeout => {
                notify.close(id).await;
                Err(rejected("the prompt timed out"))
            }
            Answer::Closed => {
                notify.close(id).await;
                Err(rejected("the notification was closed"))
            }
            Answer::Canceled => {
                notify.close(id).await;
                Err(rejected("obexd canceled the transfer"))
            }
            Answer::Lost => {
                notify.close(id).await;
                Err(rejected("the notification daemon went away"))
            }
        }
    }

    async fn wait(
        &self,
        id: u32,
        mut watch: notify::Watch,
        cancel_rx: oneshot::Receiver<()>,
    ) -> Answer {
        tokio::select! {
            biased;
            reply = notify::wait_reply(&mut watch, id) => match reply {
                notify::Reply::Action(action) if action == "accept" => Answer::Accept,
                notify::Reply::Action(_) => Answer::Decline,
                notify::Reply::Closed => Answer::Closed,
                notify::Reply::Lost => Answer::Lost,
            },
            _ = cancel_rx => Answer::Canceled,
            _ = tokio::time::sleep(self.config.answer_timeout) => Answer::Timeout,
        }
    }
}

#[zbus::interface(name = "org.bluez.obex.Agent1")]
impl Agent {
    async fn authorize_push(&self, transfer: ObjectPath<'_>) -> Result<String, AgentError> {
        self.push(transfer.as_str()).await
    }

    async fn cancel(&self) {
        if let Some(sender) = self.pending.lock().await.take() {
            let _ = sender.send(());
        }
    }

    async fn release(&self) {
        if let Some(sender) = self.pending.lock().await.take() {
            let _ = sender.send(());
        }
    }
}

pub async fn spawn(session: zbus::Connection, system: zbus::Connection) {
    spawn_with(session, system, Config::default()).await;
}

async fn spawn_with(session: zbus::Connection, system: zbus::Connection, config: Config) {
    let reactivate = config.reactivate;
    let agent = Agent::new(session.clone(), system, config);
    if let Err(error) = session.object_server().at(PATH, agent).await {
        tracing::warn!(%error, "could not export the obex agent");
        return;
    }
    register(&session).await;
    tokio::join!(
        crate::agents::on_owner(&session, SERVICE, || register(&session)),
        revive(&session, reactivate),
    );
}

async fn revive(session: &zbus::Connection, delay: Duration) {
    let Ok(dbus) = zbus::fdo::DBusProxy::new(session).await else {
        return;
    };
    let owners = dbus
        .receive_name_owner_changed_with_args(&[(0, SERVICE)])
        .await
        .map(|stream| {
            glimpse_dbus::events(stream.filter_map(|signal| async move {
                Some(signal.args().ok()?.new_owner().is_some())
            }))
        });
    let Ok(mut owners) = owners else {
        return;
    };
    while let Some(delivery) = owners.next().await {
        let present = match delivery {
            Delivery::Item(present) => present,
            Delivery::Missed => crate::agents::has_owner(&dbus, SERVICE).await,
        };
        if !present {
            tokio::time::sleep(delay).await;
            register(session).await;
        }
    }
}

async fn register(session: &zbus::Connection) {
    let Ok(path) = ObjectPath::try_from(PATH) else {
        return;
    };
    let manager = match AgentManager1Proxy::builder(session)
        .destination(SERVICE)
        .and_then(|builder| builder.path(MANAGER))
    {
        Ok(builder) => match builder.build().await {
            Ok(manager) => manager,
            Err(error) => {
                tracing::warn!(%error, "could not reach the obex agent manager");
                return;
            }
        },
        Err(error) => {
            tracing::warn!(%error, "could not reach the obex agent manager");
            return;
        }
    };
    match manager.register_agent(&path).await {
        Ok(()) => tracing::info!("obex: registered the file push agent"),
        Err(error) if already_registered(&error) => {}
        Err(error) if service_unknown(&error) => {
            tracing::debug!("bluez-obex is not installed; incoming file pushes stay disabled");
        }
        Err(error) => {
            tracing::warn!(%error, "could not register the obex agent");
        }
    }
}

fn already_registered(error: &zbus::Error) -> bool {
    match error {
        zbus::Error::MethodError(name, _, _) => {
            name.as_str() == "org.bluez.obex.Error.AlreadyExists"
        }
        _ => false,
    }
}

fn service_unknown(error: &zbus::Error) -> bool {
    match error {
        zbus::Error::MethodError(name, _, _) => {
            name.as_str() == "org.freedesktop.DBus.Error.ServiceUnknown"
        }
        _ => false,
    }
}

async fn locked(system: &zbus::Connection) -> bool {
    let session = match tokio::time::timeout(LOCK_TIMEOUT, crate::lock::session(system)).await {
        Ok(Ok(session)) => session,
        Ok(Err(error)) => {
            tracing::debug!(%error, "obex: the logind session is unreadable");
            return true;
        }
        Err(_) => {
            tracing::debug!("obex: the logind session did not answer");
            return true;
        }
    };
    match tokio::time::timeout(LOCK_TIMEOUT, session.locked_hint()).await {
        Ok(Ok(locked)) => locked,
        Ok(Err(error)) => {
            tracing::debug!(%error, "obex: LockedHint is unreadable");
            true
        }
        Err(_) => {
            tracing::debug!("obex: LockedHint did not answer");
            true
        }
    }
}

async fn device(system: &zbus::Connection, destination: &str) -> Option<Device> {
    let manager = read(ROOT, async {
        zbus::fdo::ObjectManagerProxy::builder(system)
            .destination(BLUEZ)?
            .path(ROOT)?
            .build()
            .await
    })
    .await?;
    let objects = read(ROOT, async {
        manager
            .get_managed_objects()
            .await
            .map_err(zbus::Error::from)
    })
    .await?;
    for (_, interfaces) in objects {
        let Some(properties) = interfaces.get(DEVICE1) else {
            continue;
        };
        let Some(address) = properties
            .get("Address")
            .and_then(|value| <&str>::try_from(value).ok())
        else {
            continue;
        };
        if !address.eq_ignore_ascii_case(destination) {
            continue;
        }
        let paired = properties
            .get("Paired")
            .and_then(|value| bool::try_from(value).ok())
            .unwrap_or(false);
        let name = properties
            .get("Alias")
            .or_else(|| properties.get("Name"))
            .and_then(|value| <&str>::try_from(value).ok())
            .map(|name| glimpse_utils::clean(name, NAME_CAP))
            .unwrap_or_default();
        return Some(Device { paired, name });
    }
    None
}

fn rejected(reason: &str) -> AgentError {
    AgentError::Rejected(reason.to_owned())
}

async fn read<T>(path: &str, read: impl Future<Output = zbus::Result<T>>) -> Option<T> {
    match tokio::time::timeout(READ_TIMEOUT, read).await {
        Ok(Ok(value)) => Some(value),
        Ok(Err(error)) => {
            tracing::debug!(%error, path, "obex: read failed");
            None
        }
        Err(_) => {
            tracing::debug!(path, "obex: read timed out");
            None
        }
    }
}

#[cfg(test)]
mod tests;
