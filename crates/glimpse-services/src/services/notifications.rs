use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use gio_unix::{DesktopAppInfo, prelude::*};
use glimpse_dbus::notifications::{
    DEFAULT_ACTION, DoNotDisturb, NotificationAction, NotificationRecord, NotificationUrgency,
    NotificationsDnd, NotificationsList,
};
use regex::Regex;
use tokio::sync::{mpsc, oneshot};
use zbus::fdo::DBusProxy;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;
use zbus::{Connection, interface};

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, Service, ServiceEndpoint},
    subscription::Sub,
};

const BUS_NAME: &str = "org.freedesktop.Notifications";
const OBJECT_PATH: &str = "/org/freedesktop/Notifications";

const APP_NAME_MAX_CHARS: usize = 64;
const SUMMARY_MAX_CHARS: usize = 160;
const ACTION_LABEL_MAX_CHARS: usize = 48;
const ACTION_KEY_MAX_CHARS: usize = 64;
const ACTIONS_MAX: usize = 3;
const ICON_MAX_CHARS: usize = 128;
const IMAGE_PATH_MAX_CHARS: usize = 4096;
const ACTIVATION_TOKEN_MAX_CHARS: usize = 512;

/// `NotificationClosed` reasons, as the specification numbers them.
const NAMED: &str = "name:";
const CLOSED_EXPIRED: u32 = 1;
const CLOSED_BY_SENDER: u32 = 3;
const CLOSED_BY_READER: u32 = 2;

#[derive(Debug)]
pub struct Incoming {
    pub app_name: String,
    pub app_id: String,
    pub app_pid: Option<i32>,
    pub icon: Option<String>,
    pub image: Option<String>,
    pub summary: String,
    pub body: String,
    pub actions: Vec<(String, String)>,
    pub urgency: NotificationUrgency,
    pub progress: Option<f64>,
    pub resident: bool,
    pub expire_timeout: i32,
    pub transient: bool,
}

pub enum Event {
    Posted { id: u32, incoming: Box<Incoming> },
    Retracted { id: u32 },
    DoNotDisturbLapsed(DateTime<Utc>),
}

#[derive(Debug)]
pub enum Command {
    Dismiss {
        id: u32,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Remove {
        id: u32,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Expire {
        id: u32,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Activate {
        id: u32,
        token: Option<String>,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    InvokeAction {
        id: u32,
        action: String,
        token: Option<String>,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    ClearApp {
        app_id: String,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    ClearAll {
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    SetDnd {
        dnd: DoNotDisturb,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Post {
        incoming: Box<Incoming>,
        reply: oneshot::Sender<Result<u32, CommandError>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    keep: usize,
    suppress: Vec<String>,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            keep: document.notifications.keep.clamp(1, 1000) as usize,
            suppress: document.notifications.suppress.clone(),
        }
    }
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    DoNotDisturb(DateTime<Utc>),
}

pub struct Notifications {
    state: Publisher<NotificationsState>,
    store: Store,
    dnd: DoNotDisturb,
    connection: Option<Connection>,
    ids: Option<Ids>,
    images: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NotificationsState {
    pub list: Option<NotificationsList>,
    pub dnd: Option<NotificationsDnd>,
}

pub type NotificationsHandle = ServiceEndpoint<Notifications>;

impl ServiceEndpoint<Notifications> {
    pub async fn post(&self, incoming: Incoming) -> Result<u32, CommandError> {
        self.call("notifications did not answer", |reply| Command::Post {
            incoming: Box::new(incoming),
            reply,
        })
        .await?
    }

    pub async fn dismiss(&self, id: u32) -> Result<(), CommandError> {
        self.call(
            "notifications stopped before completing the command",
            |reply| Command::Dismiss { id, reply },
        )
        .await?
    }

    pub async fn expire(&self, id: u32) -> Result<(), CommandError> {
        self.call(
            "notifications stopped before completing the command",
            |reply| Command::Expire { id, reply },
        )
        .await?
    }

    pub async fn remove(&self, id: u32) -> Result<(), CommandError> {
        self.call(
            "notifications stopped before completing the command",
            |reply| Command::Remove { id, reply },
        )
        .await?
    }

    pub async fn activate(&self, id: u32, token: Option<String>) -> Result<(), CommandError> {
        self.call(
            "notifications stopped before completing the command",
            |reply| Command::Activate {
                id,
                token: valid_activation_token(token),
                reply,
            },
        )
        .await?
    }

    pub async fn invoke_action(
        &self,
        id: u32,
        action: String,
        token: Option<String>,
    ) -> Result<(), CommandError> {
        self.call(
            "notifications stopped before completing the command",
            |reply| Command::InvokeAction {
                id,
                action,
                token: valid_activation_token(token),
                reply,
            },
        )
        .await?
    }

    pub async fn clear_app(&self, app_id: String) -> Result<(), CommandError> {
        self.call(
            "notifications stopped before completing the command",
            |reply| Command::ClearApp { app_id, reply },
        )
        .await?
    }

    pub async fn clear_all(&self) -> Result<(), CommandError> {
        self.call(
            "notifications stopped before completing the command",
            |reply| Command::ClearAll { reply },
        )
        .await?
    }

    pub async fn set_dnd(&self, dnd: DoNotDisturb) -> Result<(), CommandError> {
        self.call(
            "notifications stopped before completing the command",
            |reply| Command::SetDnd { dnd, reply },
        )
        .await?
    }
}

/// Everything the service decides about what to keep, with no publisher and no bus in it, so the
/// bound and the replace rule are ordinary tests rather than something only a live daemon shows.
#[derive(Debug, Default)]
pub struct Store {
    held: VecDeque<NotificationRecord>,
    transient: HashSet<u32>,
    keep: usize,
    suppress: Vec<Regex>,
}

impl Service for Notifications {
    const NAME: &'static str = "notifications";

    type Config = Config;
    type State = NotificationsState;
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Watch;

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let Some(until) = self.dnd.until.filter(|_| self.dnd.enabled) else {
            return Vec::new();
        };
        vec![Sub::deadline(
            Watch::DoNotDisturb(until),
            until,
            Event::DoNotDisturbLapsed(until),
        )]
    }

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    async fn start(ctx: &Ctx<Self>, config: Self::Config, (): Self::Dependencies) -> Self {
        let mut service = Self {
            state: ctx.publisher(),
            store: Store::with_suppression(config.keep, &config.suppress),
            dnd: DoNotDisturb::default(),
            connection: None,
            ids: None,
            images: HashSet::new(),
        };
        service.publish();
        if let Some(dir) = images_dir() {
            let _ = tokio::fs::remove_dir_all(dir).await;
        }

        let connection = match ctx.session_bus() {
            Ok(connection) => connection.clone(),
            Err(reason) => {
                ctx.degraded(format!("no session bus: {reason}"));
                return service;
            }
        };

        let ids = Ids::default();
        let served = Served {
            events: ctx.events(),
            next: ids.clone(),
            dbus: match DBusProxy::new(&connection).await {
                Ok(dbus) => Some(dbus),
                Err(error) => {
                    tracing::warn!(%error, "no sender pid will be recorded");
                    None
                }
            },
        };
        if let Err(error) = connection.object_server().at(OBJECT_PATH, served).await {
            ctx.degraded(format!("cannot serve {OBJECT_PATH}: {error}"));
            return service;
        }

        match glimpse_dbus::own_name(&connection, BUS_NAME).await {
            Ok(()) => {
                service.connection = Some(connection);
                service.ids = Some(ids);
                ctx.running();
            }
            Err(zbus::Error::NameTaken) => ctx.degraded(format!(
                "another notification daemon already owns {BUS_NAME}"
            )),
            Err(error) => ctx.degraded(format!("cannot take {BUS_NAME}: {error}")),
        }

        service
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        self.dispatch(input).await;
        self.forget_images(ctx);
    }
}

impl Notifications {
    async fn dispatch(&mut self, input: Input<Self>) {
        match input {
            Input::Event(Event::Posted { id, incoming }) => self.posted(id, *incoming),
            Input::Event(Event::Retracted { id }) => self.remove(id, CLOSED_BY_SENDER).await,
            Input::Event(Event::DoNotDisturbLapsed(until)) => {
                if self.dnd.until == Some(until) {
                    self.dnd = DoNotDisturb::default();
                    self.announce_dnd();
                }
            }
            Input::Command(Command::Post { incoming, reply }) => match self.ids.as_ref() {
                Some(ids) => {
                    let id = ids.allocate(0);
                    self.posted(id, *incoming);
                    let _ = reply.send(Ok(id));
                }
                None => {
                    let _ = reply.send(Err(CommandError::Unavailable(
                        "the notification server is not exported".to_owned(),
                    )));
                }
            },
            Input::Command(Command::Dismiss { id, reply }) => {
                if self.store.dismiss(id) {
                    self.publish();
                    self.closed(id, CLOSED_BY_READER).await;
                }
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::Remove { id, reply }) => {
                self.remove(id, CLOSED_BY_READER).await;
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::Expire { id, reply }) => {
                if self.store.transient.contains(&id) {
                    self.remove(id, CLOSED_EXPIRED).await;
                }
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::Activate { id, token, reply }) => {
                if let Some(activation) = self.store.activation(id) {
                    if activation.invoke_default {
                        self.invoked(id, DEFAULT_ACTION, token.as_deref()).await;
                    }
                    if !activation.resident && self.store.dismiss(id) {
                        self.publish();
                        self.closed(id, CLOSED_BY_READER).await;
                    }
                }
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::InvokeAction {
                id,
                action,
                token,
                reply,
            }) => match self.store.offered(id, &action) {
                Some(resident) => {
                    self.invoked(id, &action, token.as_deref()).await;
                    if !resident && self.store.dismiss(id) {
                        self.publish();
                        self.closed(id, CLOSED_BY_READER).await;
                    }
                    let _ = reply.send(Ok(()));
                }
                None => {
                    let _ = reply.send(Err(CommandError::InvalidArgument(format!(
                        "no notification {id} offering action {action}"
                    ))));
                }
            },
            Input::Command(Command::ClearApp { app_id, reply }) => {
                let gone = self
                    .store
                    .drain(|record| record.app_id == app_id && !pending(record));
                self.publish();
                for id in gone {
                    self.closed(id, CLOSED_BY_READER).await;
                }
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::ClearAll { reply }) => {
                let gone = self.store.drain(|record| !pending(record));
                self.publish();
                for id in gone {
                    self.closed(id, CLOSED_BY_READER).await;
                }
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::SetDnd { dnd, reply }) => {
                self.dnd = dnd;
                self.announce_dnd();
                let _ = reply.send(Ok(()));
            }
            Input::Config(config) => {
                if self.store.reconfigure(config.keep, &config.suppress) {
                    self.publish();
                }
            }
        }
    }
}

impl Store {
    fn new(keep: usize) -> Self {
        Self {
            held: VecDeque::new(),
            transient: HashSet::new(),
            keep,
            suppress: Vec::new(),
        }
    }

    fn with_suppression(keep: usize, patterns: &[String]) -> Self {
        let mut store = Self::new(keep);
        store.suppress = compile_patterns(patterns);
        store
    }

    fn post(&mut self, id: u32, mut incoming: Incoming) -> bool {
        incoming.app_name = application_name(&incoming.app_name, &incoming.app_id);
        if self.matches(
            &incoming.app_name,
            &incoming.app_id,
            &incoming.summary,
            &incoming.body,
        ) {
            return false;
        }
        match incoming.transient {
            true => self.transient.insert(id),
            false => self.transient.remove(&id),
        };
        let record = record(id, incoming);
        match self.held.iter().position(|held| held.id == id) {
            Some(at) => self.held[at] = record,
            None => self.held.push_front(record),
        }
        let held = &self.held;
        self.transient
            .retain(|id| held.iter().any(|record| record.id == *id));
        true
    }

    fn bound(&mut self) -> bool {
        let before = self.held.len();
        let mut read = 0;
        self.held.retain(|record| {
            if record.unread {
                return true;
            }
            read += 1;
            read <= self.keep
        });
        self.held.len() != before
    }

    fn reconfigure(&mut self, keep: usize, patterns: &[String]) -> bool {
        self.keep = keep;
        self.suppress = compile_patterns(patterns);
        let before = self.held.len();
        let suppress = &self.suppress;
        self.held.retain(|record| {
            !matches_patterns(
                suppress,
                &record.app_name,
                &record.app_id,
                &record.summary,
                record.body.as_deref().unwrap_or_default(),
            )
        });
        self.held.len() != before || self.bound()
    }

    fn matches(&self, app_name: &str, app_id: &str, summary: &str, body: &str) -> bool {
        matches_patterns(&self.suppress, app_name, app_id, summary, body)
    }

    fn take(&mut self, id: u32) -> Option<NotificationRecord> {
        let at = self.held.iter().position(|held| held.id == id)?;
        self.transient.remove(&id);
        self.held.remove(at)
    }

    fn dismiss(&mut self, id: u32) -> bool {
        let Some(record) = self.held.iter_mut().find(|record| record.id == id) else {
            return false;
        };
        if !record.unread {
            return false;
        }
        if self.transient.remove(&id) {
            self.held.retain(|record| record.id != id);
            return true;
        }
        record.unread = false;
        self.bound();
        true
    }

    fn activation(&self, id: u32) -> Option<Activation> {
        let record = self
            .held
            .iter()
            .find(|record| record.id == id && record.unread)?;
        Some(Activation {
            invoke_default: record
                .actions
                .iter()
                .any(|offer| offer.key == DEFAULT_ACTION),
            resident: record.resident,
        })
    }

    fn offered(&self, id: u32, action: &str) -> Option<bool> {
        self.held
            .iter()
            .find(|held| {
                held.id == id && held.unread && held.actions.iter().any(|offer| offer.key == action)
            })
            .map(|held| held.resident)
    }

    fn drain(&mut self, mut doomed: impl FnMut(&NotificationRecord) -> bool) -> Vec<u32> {
        let mut gone = Vec::new();
        self.held.retain(|record| match doomed(record) {
            true => {
                if record.unread {
                    gone.push(record.id);
                }
                false
            }
            false => true,
        });
        let held = &self.held;
        self.transient
            .retain(|id| held.iter().any(|record| record.id == *id));
        gone
    }

    fn records(&self) -> Vec<NotificationRecord> {
        self.held.iter().cloned().collect()
    }
}

fn compile_patterns(patterns: &[String]) -> Vec<Regex> {
    crate::services::compile_patterns(patterns, |pattern, error| {
        tracing::warn!(pattern, %error, "ignoring a notification suppression pattern that does not compile");
    })
}

fn matches_patterns(
    patterns: &[Regex],
    app_name: &str,
    app_id: &str,
    summary: &str,
    body: &str,
) -> bool {
    patterns.iter().any(|pattern| {
        [app_name, app_id, summary, body]
            .into_iter()
            .any(|field| pattern.is_match(field))
    })
}

fn application_name(declared: &str, app_id: &str) -> String {
    let declared = glimpse_utils::text::clean(declared, APP_NAME_MAX_CHARS);
    if !declared.is_empty() {
        return declared;
    }

    let app_id = glimpse_utils::text::clean(app_id, APP_NAME_MAX_CHARS);
    if app_id.is_empty() || app_id.starts_with(':') || app_id.starts_with("notification-") {
        return String::new();
    }

    desktop_name(&app_id).unwrap_or(app_id)
}

fn application_id(
    desktop_entry: Option<String>,
    process_desktop_entry: Option<String>,
    app_name: &str,
    sender: Option<String>,
    id: u32,
) -> String {
    desktop_entry
        .filter(|app_id| !app_id.is_empty())
        .or(process_desktop_entry)
        .or_else(|| {
            Some(glimpse_utils::text::clean(
                app_name,
                APP_NAME_MAX_CHARS - NAMED.len(),
            ))
            .filter(|name| !name.is_empty())
            .map(|name| format!("{NAMED}{name}"))
        })
        .or(sender)
        .unwrap_or_else(|| format!("notification-{id}"))
}

async fn process_desktop_entry(pid: Option<i32>) -> Option<String> {
    let pid = pid?;
    tokio::task::spawn_blocking(move || {
        let executable =
            std::fs::read_link(Path::new("/proc").join(pid.to_string()).join("exe")).ok()?;
        let app_id = executable.file_name()?.to_str()?;
        DesktopAppInfo::new(&format!("{app_id}.desktop")).map(|_| app_id.to_owned())
    })
    .await
    .ok()
    .flatten()
}

fn desktop_name(app_id: &str) -> Option<String> {
    if app_id.contains('/') {
        return None;
    }
    let desktop_id = match app_id.ends_with(".desktop") {
        true => app_id.to_owned(),
        false => format!("{app_id}.desktop"),
    };
    DesktopAppInfo::new(&desktop_id)
        .map(|entry| entry.display_name().to_string())
        .map(|name| glimpse_utils::text::clean(&name, APP_NAME_MAX_CHARS))
        .filter(|name| !name.is_empty())
}

#[derive(Debug, PartialEq, Eq)]
struct Activation {
    invoke_default: bool,
    resident: bool,
}

impl Notifications {
    fn announce_dnd(&mut self) {
        self.state.update(|state| {
            state.dnd = Some(NotificationsDnd { dnd: self.dnd });
        });
    }

    fn forget_images(&mut self, ctx: &Ctx<Self>) {
        let Some(dir) = images_dir() else {
            return;
        };
        let now: HashSet<String> = self
            .store
            .held
            .iter()
            .filter_map(|record| record.image.clone())
            .filter(|image| generated(image, &dir))
            .collect();
        let gone: Vec<String> = self.images.difference(&now).cloned().collect();
        self.images = now;
        if gone.is_empty() {
            return;
        }
        ctx.spawn_detached(|_| async move {
            for path in gone {
                let _ = tokio::fs::remove_file(path).await;
            }
        });
    }

    fn publish(&mut self) {
        let list = NotificationsList {
            notifications: self.store.records(),
        };
        self.state.update(|state| {
            state.list = Some(list);
            state.dnd = Some(NotificationsDnd { dnd: self.dnd });
        });
    }

    fn posted(&mut self, id: u32, incoming: Incoming) {
        if self.store.post(id, incoming) {
            self.publish();
        }
    }

    async fn closed(&self, id: u32, reason: u32) {
        let Some(connection) = &self.connection else {
            return;
        };
        match connection
            .object_server()
            .interface::<_, Served>(OBJECT_PATH)
            .await
        {
            Ok(served) => {
                if let Err(error) = served.notification_closed(id, reason).await {
                    tracing::debug!(%error, id, "cannot report a closed notification");
                }
            }
            Err(error) => tracing::debug!(%error, "the notifications interface is not exported"),
        }
    }

    async fn remove(&mut self, id: u32, reason: u32) {
        let Some(record) = self.store.take(id) else {
            return;
        };
        self.publish();
        if record.unread {
            self.closed(id, reason).await;
        }
    }

    async fn invoked(&self, id: u32, action: &str, token: Option<&str>) {
        let Some(connection) = &self.connection else {
            return;
        };
        match connection
            .object_server()
            .interface::<_, Served>(OBJECT_PATH)
            .await
        {
            Ok(served) => {
                if let Some(token) = token
                    && let Err(error) = served.activation_token(id, token).await
                {
                    tracing::debug!(%error, id, "cannot hand over an activation token");
                }
                if let Err(error) = served.action_invoked(id, action).await {
                    tracing::debug!(%error, id, "cannot report an invoked action");
                }
            }
            Err(error) => tracing::debug!(%error, "the notifications interface is not exported"),
        }
    }
}

/// Every string here is chosen by the sender and unbounded on the wire, so each one is capped
/// before it reaches a payload. The body goes through the markup sanitiser instead of `clean`,
/// which flattens the newline the most common sender shape depends on.
fn record(id: u32, incoming: Incoming) -> NotificationRecord {
    let Incoming {
        app_name,
        app_id,
        app_pid,
        icon,
        image,
        summary,
        body,
        actions,
        urgency,
        progress,
        resident,
        expire_timeout,
        transient: _,
    } = incoming;

    let app_name = glimpse_utils::text::clean(&app_name, APP_NAME_MAX_CHARS);
    let app_id = glimpse_utils::text::clean(&app_id, APP_NAME_MAX_CHARS);

    let image = image
        .map(|image| glimpse_utils::text::clean(&image, IMAGE_PATH_MAX_CHARS))
        .and_then(|image| local_path(&image));

    NotificationRecord {
        id,
        app_id,
        app_name,
        app_pid,
        summary: glimpse_utils::text::clean(&summary, SUMMARY_MAX_CHARS),
        body: match body.trim().is_empty() {
            true => None,
            false => Some(glimpse_utils::markup::sanitize_body(&body)),
        },
        icon: icon
            .map(|icon| glimpse_utils::text::clean(&icon, ICON_MAX_CHARS))
            .filter(|icon| !icon.is_empty()),
        image,
        urgency,
        actions: bounded_actions(actions),
        progress: progress.map(|value| value.clamp(0.0, 1.0)),
        created: Utc::now(),
        unread: true,
        resident,
        expire_timeout,
    }
}

/// The exported half. It owns nothing but the id counter: every decision about what to keep is
/// the service's, which is what lets the store be tested without a bus.
struct Served {
    events: mpsc::Sender<Input<Notifications>>,
    next: Ids,
    dbus: Option<DBusProxy<'static>>,
}

impl Served {
    async fn sender_pid(&self, header: &zbus::message::Header<'_>) -> Option<i32> {
        let pid = self
            .dbus
            .as_ref()?
            .get_connection_unix_process_id(header.sender()?.clone().into())
            .await
            .ok()?;
        i32::try_from(pid).ok().filter(|pid| *pid > 0)
    }
}

#[interface(name = "org.freedesktop.Notifications")]
impl Served {
    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &mut self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> u32 {
        let id = self.next.allocate(replaces_id);
        let app_pid = self.sender_pid(&header).await;

        let app_id = application_id(
            hint_str(&hints, "desktop-entry"),
            process_desktop_entry(app_pid).await,
            &app_name,
            header.sender().map(ToString::to_string),
            id,
        );
        let image_data = match pixels(&hints) {
            Some(pixels) => {
                let stamp = Utc::now().timestamp_nanos_opt().unwrap_or_default();
                write_image(pixels, format!("{id}-{stamp}.png")).await
            }
            None => None,
        };
        let (icon, image) = visuals(&app_icon, image_hint(&hints).as_deref(), image_data);
        let incoming = Incoming {
            app_name,
            app_id,
            app_pid,
            icon,
            image,
            summary,
            body,
            actions: pairs(actions),
            urgency: urgency(&hints),
            progress: hint_i32(&hints, "value").map(normalized_progress),
            resident: hint_bool(&hints, "resident").unwrap_or(false),
            expire_timeout,
            transient: hint_bool(&hints, "transient").unwrap_or(false),
        };

        let _ = self
            .events
            .send(Input::Event(Event::Posted {
                id,
                incoming: Box::new(incoming),
            }))
            .await;

        id
    }

    async fn close_notification(&mut self, id: u32) {
        let _ = self
            .events
            .send(Input::Event(Event::Retracted { id }))
            .await;
    }

    fn get_capabilities(&self) -> Vec<String> {
        ["actions", "body", "body-markup", "persistence"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "glimpse".to_owned(),
            "me.aresa".to_owned(),
            env!("CARGO_PKG_VERSION").to_owned(),
            "1.2".to_owned(),
        )
    }

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn activation_token(
        emitter: &SignalEmitter<'_>,
        id: u32,
        token: &str,
    ) -> zbus::Result<()>;
}

/// Zero means "new" on the wire, so the counter steps over it when it wraps rather than handing
/// it out. A sender naming a replacement gets that id back and spends none.
#[derive(Debug, Clone)]
struct Ids(std::sync::Arc<std::sync::atomic::AtomicU32>);

impl Default for Ids {
    fn default() -> Self {
        Self(std::sync::Arc::new(std::sync::atomic::AtomicU32::new(1)))
    }
}

impl Ids {
    fn allocate(&self, replaces: u32) -> u32 {
        use std::sync::atomic::Ordering;
        if replaces != 0 {
            return replaces;
        }
        let mut current = self.0.load(Ordering::Relaxed);
        loop {
            let next = current.wrapping_add(1).max(1);
            match self
                .0
                .compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return current,
                Err(seen) => current = seen,
            }
        }
    }
}

/// The specification sends actions as a flat list of alternating key and label. An odd trailing
/// entry is a sender's mistake and is dropped rather than being given an empty label.
fn pairs(actions: Vec<String>) -> Vec<(String, String)> {
    actions
        .chunks_exact(2)
        .map(|pair| (pair[0].clone(), pair[1].clone()))
        .collect()
}

fn bounded_actions(actions: Vec<(String, String)>) -> Vec<NotificationAction> {
    let mut default = None;
    let mut named = Vec::new();
    for (key, label) in actions {
        if key.is_empty() || key.chars().count() > ACTION_KEY_MAX_CHARS {
            continue;
        }
        let action = NotificationAction {
            key,
            label: glimpse_utils::text::clean(&label, ACTION_LABEL_MAX_CHARS),
        };
        if action.key == DEFAULT_ACTION {
            default.get_or_insert(action);
        } else if named.len() < ACTIONS_MAX {
            named.push(action);
        }
    }
    default.into_iter().chain(named).collect()
}

fn normalized_progress(value: i32) -> f64 {
    f64::from(value.clamp(0, 100)) / 100.0
}

fn valid_activation_token(token: Option<String>) -> Option<String> {
    token.filter(|token| !token.is_empty() && token.chars().count() <= ACTIVATION_TOKEN_MAX_CHARS)
}

fn hint_str(hints: &HashMap<String, OwnedValue>, name: &str) -> Option<String> {
    hints
        .get(name)
        .and_then(|value| <&str>::try_from(value).ok())
        .map(str::to_owned)
}

fn image_hint(hints: &HashMap<String, OwnedValue>) -> Option<String> {
    hint_str(hints, "image-path").or_else(|| hint_str(hints, "image_path"))
}

fn local_path(raw: &str) -> Option<String> {
    match url::Url::parse(raw) {
        Ok(url) if url.scheme() == "file" => url.to_file_path().ok()?.to_str().map(str::to_owned),
        Ok(_) => None,
        Err(_) => Path::new(raw).is_absolute().then(|| raw.to_owned()),
    }
}

fn icon_name(raw: &str) -> Option<String> {
    (!raw.is_empty() && !raw.contains('/') && url::Url::parse(raw).is_err()).then(|| raw.to_owned())
}

fn visuals(
    app_icon: &str,
    image_path: Option<&str>,
    image_data: Option<String>,
) -> (Option<String>, Option<String>) {
    let icon = icon_name(app_icon)
        .or_else(|| local_path(app_icon))
        .or_else(|| image_path.and_then(icon_name));
    let image = image_data.or_else(|| image_path.and_then(local_path));
    (icon, image)
}

struct Pixels {
    width: u32,
    height: u32,
    alpha: bool,
    rows: Vec<u8>,
}

const IMAGE_DATA_SIDE_MAX: i32 = 1024;

fn pixels(hints: &HashMap<String, OwnedValue>) -> Option<Pixels> {
    let value = ["image-data", "image_data"]
        .iter()
        .find_map(|name| hints.get(*name))
        .or_else(|| {
            (hint_str(hints, "image-path").is_none() && hint_str(hints, "image_path").is_none())
                .then(|| hints.get("icon_data"))
                .flatten()
        })?;
    let (width, height, rowstride, alpha, bits, channels, data) =
        <(i32, i32, i32, bool, i32, i32, Vec<u8>)>::try_from(value.try_clone().ok()?).ok()?;
    let sides = 1..=IMAGE_DATA_SIDE_MAX;
    if bits != 8 || channels != if alpha { 4 } else { 3 } {
        return None;
    }
    if !sides.contains(&width) || !sides.contains(&height) {
        return None;
    }
    let (width, height, rowstride, channels) = (
        width as usize,
        height as usize,
        usize::try_from(rowstride).ok()?,
        channels as usize,
    );
    let row = width * channels;
    if rowstride < row || data.len() < rowstride * (height - 1) + row {
        return None;
    }
    let rows = (0..height)
        .flat_map(|y| &data[y * rowstride..y * rowstride + row])
        .copied()
        .collect();
    Some(Pixels {
        width: width as u32,
        height: height as u32,
        alpha,
        rows,
    })
}

fn encode(pixels: &Pixels) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, pixels.width, pixels.height);
    encoder.set_color(match pixels.alpha {
        true => png::ColorType::Rgba,
        false => png::ColorType::Rgb,
    });
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(&pixels.rows).ok()?;
    writer.finish().ok()?;
    Some(out)
}

fn generated(image: &str, dir: &Path) -> bool {
    Path::new(image).parent() == Some(dir)
}

fn images_dir() -> Option<PathBuf> {
    Some(
        glimpse_config::runtime_dir()?
            .join("notifications")
            .join(std::process::id().to_string()),
    )
}

async fn write_image(pixels: Pixels, name: String) -> Option<String> {
    let path = images_dir()?.join(name);
    let bytes = tokio::task::spawn_blocking(move || encode(&pixels))
        .await
        .ok()??;
    tokio::fs::create_dir_all(path.parent()?).await.ok()?;
    tokio::fs::write(&path, bytes).await.ok()?;
    path.to_str().map(str::to_owned)
}

fn hint_i32(hints: &HashMap<String, OwnedValue>, name: &str) -> Option<i32> {
    hints.get(name).and_then(|value| i32::try_from(value).ok())
}

fn pending(record: &NotificationRecord) -> bool {
    record.unread
        && (record.resident || record.expire_timeout == 0)
        && record
            .actions
            .iter()
            .any(|action| action.key != DEFAULT_ACTION)
}

fn hint_bool(hints: &HashMap<String, OwnedValue>, name: &str) -> Option<bool> {
    hints.get(name).and_then(|value| bool::try_from(value).ok())
}

/// The specification numbers urgency 0, 1, 2. Anything else is a sender that has invented one, and
/// is read as normal rather than refused.
fn urgency(hints: &HashMap<String, OwnedValue>) -> NotificationUrgency {
    match hints.get("urgency").and_then(|v| u8::try_from(v).ok()) {
        Some(0) => NotificationUrgency::Low,
        Some(2) => NotificationUrgency::Critical,
        _ => NotificationUrgency::Normal,
    }
}

#[cfg(test)]
mod tests;
