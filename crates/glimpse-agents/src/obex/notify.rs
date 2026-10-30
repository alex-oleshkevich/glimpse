use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::time::Duration;

use futures_util::stream::BoxStream;
use futures_util::{StreamExt, future};
use gettextrs::gettext;
use glimpse_dbus::Delivery;
use tokio::time::Instant;
use zbus::message::Type;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::{MatchRule, Message, MessageStream};

use glimpse_dbus::freedesktop_notifications::FreedesktopNotificationsProxy;
use glimpse_dbus::obex::{SERVICE, Transfer1Proxy};

use super::{file, read};
use crate::notifications::Signal;

const APP_NAME: &str = "Bluetooth";
const ICON: &str = "bluetooth-symbolic";
const DEADLINE: Duration = Duration::from_secs(5);

pub(super) struct Notify {
    connection: zbus::Connection,
    proxy: FreedesktopNotificationsProxy<'static>,
    id: AtomicU32,
}

pub(super) struct Watch {
    signals: BoxStream<'static, Delivery<Signal>>,
}

impl Notify {
    pub(super) async fn new(session: &zbus::Connection) -> Option<Self> {
        match FreedesktopNotificationsProxy::new(session).await {
            Ok(proxy) => Some(Self {
                connection: session.clone(),
                proxy,
                id: AtomicU32::new(0),
            }),
            Err(error) => {
                tracing::debug!(%error, "obex: cannot reach the notification service");
                None
            }
        }
    }

    pub(super) async fn incoming(&self, device: &str, file: &str) -> Option<u32> {
        let summary = gettext("Incoming file");
        let body = gettext("“{device}” wants to send you “{file}”")
            .replace("{device}", device)
            .replace("{file}", file);
        let accept = gettext("Accept");
        let decline = gettext("Decline");
        let actions = ["accept", accept.as_str(), "decline", decline.as_str()];
        self.show(summary, body, &actions, None).await
    }

    pub(super) async fn progress(&self, file: &str, percent: u8) -> Option<u32> {
        let summary = gettext("Receiving “{file}”").replace("{file}", file);
        self.show(summary, String::new(), &[], Some(percent)).await
    }

    pub(super) async fn received(&self, file: &str) -> Option<u32> {
        let summary = gettext("Received “{file}”").replace("{file}", file);
        let open = gettext("Open");
        let folder = gettext("Open Folder");
        let actions = ["default", open.as_str(), "folder", folder.as_str()];
        self.show(summary, String::new(), &actions, None).await
    }

    pub(super) async fn failed(&self, file: &str) -> Option<u32> {
        let summary = gettext("Could not receive “{file}”").replace("{file}", file);
        self.show(summary, String::new(), &[], None).await
    }

    pub(super) async fn close(&self, id: u32) {
        let closing = self.proxy.close_notification(id);
        if !matches!(tokio::time::timeout(DEADLINE, closing).await, Ok(Ok(()))) {
            tracing::debug!(id, "obex: could not close the notification");
        }
    }

    async fn show(
        &self,
        summary: String,
        body: String,
        actions: &[&str],
        value: Option<u8>,
    ) -> Option<u32> {
        let hints: HashMap<&str, Value<'_>> = match value {
            Some(value) => HashMap::from([("value", Value::I32(i32::from(value)))]),
            None => HashMap::new(),
        };
        let notify = self.proxy.notify(
            APP_NAME,
            self.id.load(Ordering::Relaxed),
            ICON,
            &summary,
            &body,
            actions,
            hints,
            -1,
        );
        match tokio::time::timeout(DEADLINE, notify).await {
            Ok(Ok(id)) if id != 0 => {
                self.id.store(id, Ordering::Relaxed);
                Some(id)
            }
            Ok(Ok(_)) => {
                tracing::debug!("obex: the notification daemon returned no id");
                None
            }
            Ok(Err(error)) => {
                tracing::debug!(%error, "obex: the notification daemon rejected a notification");
                None
            }
            Err(_) => {
                tracing::debug!("obex: the notification daemon did not answer");
                None
            }
        }
    }

    pub(super) async fn watch(&self) -> zbus::Result<Watch> {
        Ok(Watch {
            signals: crate::notifications::signals(&self.connection).await?,
        })
    }
}

pub(super) enum Reply {
    Action(String),
    Closed,
    Lost,
}

pub(super) async fn wait_reply(watch: &mut Watch, id: u32) -> Reply {
    while let Some(delivery) = watch.signals.next().await {
        let Delivery::Item(signal) = delivery else {
            return Reply::Lost;
        };
        match signal {
            Signal::Action(got, key) if got == id => return Reply::Action(key),
            Signal::Closed(got, _) if got == id => return Reply::Closed,
            Signal::OwnerChanged => return Reply::Lost,
            _ => {}
        }
    }
    Reply::Lost
}

const TRANSFER1: &str = "org.bluez.obex.Transfer1";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";

pub(super) struct Subscription {
    proxy: Transfer1Proxy<'static>,
    status: Option<String>,
    size: Option<u64>,
    transferred: u64,
    signals: BoxStream<'static, Delivery<Message>>,
}

#[derive(Default)]
struct Update {
    status: Option<String>,
    transferred: Option<u64>,
    gone: bool,
}

pub(super) async fn subscribe(session: &zbus::Connection, transfer: &str) -> Option<Subscription> {
    let path = ObjectPath::try_from(transfer).ok()?.into_owned();
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(SERVICE)
        .ok()?
        .build()
        .to_owned();
    let signals = glimpse_dbus::events(
        MessageStream::for_match_rule(rule, session, None)
            .await
            .ok()?
            .filter_map(|message| future::ready(message.ok())),
    );
    let proxy = Transfer1Proxy::builder(session)
        .path(path)
        .ok()?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .ok()?;
    let status = read(transfer, proxy.status()).await;
    let (size, transferred) = match status.as_deref() {
        Some("complete") | Some("error") | None => (None, 0),
        Some(_) => (
            read(transfer, proxy.size()).await,
            read(transfer, proxy.transferred()).await.unwrap_or(0),
        ),
    };
    Some(Subscription {
        proxy,
        status,
        size,
        transferred,
        signals,
    })
}

fn update(message: &Message, transfer: &str) -> Update {
    let header = message.header();
    let (Some(interface), Some(member), Some(path)) =
        (header.interface(), header.member(), header.path())
    else {
        return Update::default();
    };
    match (interface.as_str(), member.as_str()) {
        (PROPERTIES, "PropertiesChanged") if path.as_str() == transfer => {
            let Ok((name, changed, _)) =
                message
                    .body()
                    .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
            else {
                return Update::default();
            };
            if name != TRANSFER1 {
                return Update::default();
            }
            Update {
                status: changed
                    .get("Status")
                    .and_then(|value| <&str>::try_from(value).ok())
                    .map(str::to_owned),
                transferred: changed
                    .get("Transferred")
                    .and_then(|value| u64::try_from(value).ok()),
                gone: false,
            }
        }
        (OBJECT_MANAGER, "InterfacesRemoved") => {
            let gone = message
                .body()
                .deserialize::<(OwnedObjectPath, Vec<String>)>()
                .is_ok_and(|(removed, interfaces)| {
                    removed.as_str() == transfer && interfaces.iter().any(|name| name == TRANSFER1)
                });
            Update {
                gone,
                ..Update::default()
            }
        }
        _ => Update::default(),
    }
}

pub(super) struct Receive {
    pub(super) source: PathBuf,
    pub(super) downloads: PathBuf,
    pub(super) file: String,
    pub(super) launch: fn(&str) -> Result<(), glib::Error>,
}

impl Receive {
    async fn land(&self, notify: &Notify) {
        match file::relocate(&self.source, &self.downloads, &self.file).await {
            Ok(target) => {
                show_received(notify, &target, &self.downloads, &self.file, self.launch).await
            }
            Err(error) => {
                tracing::warn!(%error, file = %self.file, "obex: could not move the received file to the downloads");
                notify.failed(&self.file).await;
            }
        }
    }

    async fn fail(&self, notify: &Notify) {
        file::discard(&self.source).await;
        notify.failed(&self.file).await;
    }

    async fn settle(&self, notify: &Notify, size: Option<u64>) {
        let complete = match tokio::fs::metadata(&self.source).await {
            Ok(metadata) => match size {
                None | Some(0) => true,
                Some(size) => metadata.len() == size,
            },
            Err(_) => false,
        };
        match complete {
            true => self.land(notify).await,
            false => self.fail(notify).await,
        }
    }
}

pub(super) async fn follow(
    subscription: Option<Subscription>,
    transfer: String,
    receive: Receive,
    notify: Notify,
    tick: Duration,
    idle: Duration,
) {
    let Some(subscription) = subscription else {
        receive.settle(&notify, None).await;
        return;
    };
    let Subscription {
        proxy,
        status,
        size,
        transferred,
        mut signals,
    } = subscription;
    match status.as_deref() {
        Some("complete") => {
            receive.land(&notify).await;
            return;
        }
        Some("error") => {
            receive.fail(&notify).await;
            return;
        }
        None => {
            receive.settle(&notify, None).await;
            return;
        }
        _ => {}
    }
    let file = receive.file.as_str();
    let mut last = match size {
        None | Some(0) => None,
        Some(size) => Some(percent(size, transferred)),
    };
    let mut next = Instant::now();
    let mut deadline = Instant::now() + idle;
    loop {
        let Ok(delivery) = tokio::time::timeout_at(deadline, signals.next()).await else {
            receive.fail(&notify).await;
            return;
        };
        let update = match delivery {
            None => {
                receive.settle(&notify, size).await;
                return;
            }
            Some(Delivery::Missed) => {
                let status = read(&transfer, proxy.status()).await;
                Update {
                    gone: status.is_none(),
                    status,
                    transferred: None,
                }
            }
            Some(Delivery::Item(message)) => update(&message, &transfer),
        };
        match update.status.as_deref() {
            Some("complete") => {
                receive.land(&notify).await;
                return;
            }
            Some("error") => {
                receive.fail(&notify).await;
                return;
            }
            _ => {}
        }
        if update.gone {
            receive.settle(&notify, size).await;
            return;
        }
        let Some(transferred) = update.transferred else {
            continue;
        };
        deadline = Instant::now() + idle;
        let percent = match size {
            None | Some(0) => None,
            Some(size) => Some(percent(size, transferred)),
        };
        if percent != last {
            last = percent;
            if let Some(percent) = percent.filter(|_| Instant::now() >= next) {
                next = Instant::now() + tick;
                notify.progress(file, percent).await;
            }
        }
    }
}

fn percent(size: u64, transferred: u64) -> u8 {
    ((transferred.min(size) * 100) / size) as u8
}

async fn show_received(
    notify: &Notify,
    target: &Path,
    downloads: &Path,
    file: &str,
    launch: fn(&str) -> Result<(), glib::Error>,
) {
    let mut watch = match notify.watch().await {
        Ok(watch) => watch,
        Err(error) => {
            tracing::debug!(%error, "obex: cannot watch the notification service");
            return;
        }
    };
    let Some(id) = notify.received(file).await else {
        return;
    };
    let action = match wait_reply(&mut watch, id).await {
        Reply::Action(action) => Some(action),
        Reply::Closed | Reply::Lost => None,
    };
    match action.as_deref() {
        Some("default") => open(target, launch).await,
        Some("folder") => open(downloads, launch).await,
        _ => {}
    }
    if action.is_some() {
        notify.close(id).await;
    }
}

async fn open(target: &Path, launch: fn(&str) -> Result<(), glib::Error>) {
    use gio::prelude::FileExt;
    let uri = gio::File::for_path(target).uri();
    let result = tokio::task::spawn_blocking(move || launch(&uri)).await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::warn!(%error, "obex: could not open the received file");
        }
        Err(error) => {
            tracing::warn!(%error, "obex: opening the received file failed");
        }
    }
}
