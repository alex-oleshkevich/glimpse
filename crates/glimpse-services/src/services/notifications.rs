use std::{
    collections::{HashMap, VecDeque},
    path::Path,
};

use chrono::{DateTime, Utc};
use gio_unix::{DesktopAppInfo, prelude::*};
use glimpse_contracts::{
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
    service::{CommandError, Input, Service, ServiceEndpoint, ServiceError},
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
const CLOSED_BY_SENDER: u32 = 3;
const CLOSED_BY_READER: u32 = 2;

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
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NotificationsState {
    pub list: Option<NotificationsList>,
    pub dnd: Option<NotificationsDnd>,
}

#[derive(Clone)]
pub struct NotificationsHandle(ServiceEndpoint<Notifications>);

impl NotificationsHandle {
    pub fn snapshot(&self) -> NotificationsState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<NotificationsState> {
        self.0.subscribe()
    }

    pub fn health(&self) -> tokio::sync::watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    async fn call(
        &self,
        command: impl FnOnce(oneshot::Sender<Result<(), CommandError>>) -> Command,
    ) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(command(reply))?;
        result.await.map_err(|_| {
            CommandError::Unavailable(
                "notifications stopped before completing the command".to_owned(),
            )
        })?
    }

    pub async fn dismiss(&self, id: u32) -> Result<(), CommandError> {
        self.call(|reply| Command::Dismiss { id, reply }).await
    }

    pub async fn remove(&self, id: u32) -> Result<(), CommandError> {
        self.call(|reply| Command::Remove { id, reply }).await
    }

    pub async fn activate(&self, id: u32, token: Option<String>) -> Result<(), CommandError> {
        self.call(|reply| Command::Activate {
            id,
            token: valid_activation_token(token),
            reply,
        })
        .await
    }

    pub async fn invoke_action(
        &self,
        id: u32,
        action: String,
        token: Option<String>,
    ) -> Result<(), CommandError> {
        self.call(|reply| Command::InvokeAction {
            id,
            action,
            token: valid_activation_token(token),
            reply,
        })
        .await
    }

    pub async fn clear_app(&self, app_id: String) -> Result<(), CommandError> {
        self.call(|reply| Command::ClearApp { app_id, reply }).await
    }

    pub async fn clear_all(&self) -> Result<(), CommandError> {
        self.call(|reply| Command::ClearAll { reply }).await
    }

    pub async fn set_dnd(&self, dnd: DoNotDisturb) -> Result<(), CommandError> {
        self.call(|reply| Command::SetDnd { dnd, reply }).await
    }
}

pub fn initial_state() -> NotificationsState {
    NotificationsState::default()
}

/// Everything the service decides about what to keep, with no publisher and no bus in it, so the
/// bound and the replace rule are ordinary tests rather than something only a live daemon shows.
#[derive(Debug, Default)]
pub struct Store {
    held: VecDeque<NotificationRecord>,
    keep: usize,
    suppress: Vec<Regex>,
}

impl Service for Notifications {
    const NAME: &'static str = "notifications";

    type Config = Config;
    type State = NotificationsState;
    type Handle = NotificationsHandle;
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

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
        NotificationsHandle(endpoint)
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        (): Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        let mut service = Self {
            state: ctx.publisher(),
            store: Store::with_suppression(config.keep, &config.suppress),
            dnd: DoNotDisturb::default(),
            connection: None,
        };
        service.publish();

        let connection = match ctx.session_bus() {
            Ok(connection) => connection.clone(),
            Err(reason) => {
                ctx.degraded(format!("no session bus: {reason}"));
                return Ok(service);
            }
        };

        let served = Served {
            events: ctx.events(),
            next: 1,
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
            return Ok(service);
        }

        match connection.request_name(BUS_NAME).await {
            Ok(()) => {
                service.connection = Some(connection);
                ctx.running();
            }
            Err(zbus::Error::NameTaken) => ctx.degraded(format!(
                "another notification daemon already owns {BUS_NAME}"
            )),
            Err(error) => ctx.degraded(format!("cannot take {BUS_NAME}: {error}")),
        }

        Ok(service)
    }

    async fn handle(&mut self, _ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Posted { id, incoming }) => {
                if self.store.post(id, *incoming) {
                    self.publish();
                }
            }
            Input::Event(Event::Retracted { id }) => self.remove(id, CLOSED_BY_SENDER).await,
            Input::Event(Event::DoNotDisturbLapsed(until)) => {
                if self.dnd.until == Some(until) {
                    self.dnd = DoNotDisturb::default();
                    self.announce_dnd();
                }
            }
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
                let gone = self.store.drain(|record| record.app_id == app_id);
                self.publish();
                for id in gone {
                    self.closed(id, CLOSED_BY_READER).await;
                }
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::ClearAll { reply }) => {
                let gone = self.store.drain(|_| true);
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
        let record = record(id, incoming);
        match self.held.iter().position(|held| held.id == id) {
            Some(at) => self.held[at] = record,
            None => self.held.push_front(record),
        }
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
        self.held.remove(at)
    }

    fn dismiss(&mut self, id: u32) -> bool {
        let Some(record) = self.held.iter_mut().find(|record| record.id == id) else {
            return false;
        };
        if !record.unread {
            return false;
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
        gone
    }

    fn records(&self) -> Vec<NotificationRecord> {
        self.held.iter().cloned().collect()
    }
}

fn compile_patterns(patterns: &[String]) -> Vec<Regex> {
    patterns
        .iter()
        .filter_map(|pattern| match Regex::new(pattern) {
            Ok(compiled) => Some(compiled),
            Err(error) => {
                tracing::warn!(pattern, %error, "ignoring a notification suppression pattern that does not compile");
                None
            }
        })
        .collect()
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
    sender: Option<String>,
    id: u32,
) -> String {
    desktop_entry
        .filter(|app_id| !app_id.is_empty())
        .or(process_desktop_entry)
        .or(sender)
        .unwrap_or_else(|| format!("notification-{id}"))
}

fn process_desktop_entry(pid: Option<i32>) -> Option<String> {
    let executable =
        std::fs::read_link(Path::new("/proc").join(pid?.to_string()).join("exe")).ok()?;
    let app_id = executable.file_name()?.to_str()?;
    DesktopAppInfo::new(&format!("{app_id}.desktop")).map(|_| app_id.to_owned())
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

    fn publish(&mut self) {
        let list = NotificationsList {
            notifications: self.store.records(),
        };
        self.state.update(|state| {
            state.list = Some(list);
            state.dnd = Some(NotificationsDnd { dnd: self.dnd });
        });
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
    } = incoming;

    let app_name = glimpse_utils::text::clean(&app_name, APP_NAME_MAX_CHARS);
    let app_id = glimpse_utils::text::clean(&app_id, APP_NAME_MAX_CHARS);

    let image = local_image_path(image.as_deref());

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
    }
}

/// The exported half. It owns nothing but the id counter: every decision about what to keep is
/// the service's, which is what lets the store be tested without a bus.
struct Served {
    events: mpsc::Sender<Input<Notifications>>,
    next: u32,
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
        _expire_timeout: i32,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> u32 {
        let id = allocate(&mut self.next, replaces_id);
        let app_pid = self.sender_pid(&header).await;

        let app_id = application_id(
            hint_str(&hints, "desktop-entry"),
            process_desktop_entry(app_pid),
            header.sender().map(ToString::to_string),
            id,
        );
        let incoming = Incoming {
            app_name,
            app_id,
            app_pid,
            icon: (!app_icon.is_empty()).then_some(app_icon),
            image: image_hint(&hints),
            summary,
            body,
            actions: pairs(actions),
            urgency: urgency(&hints),
            progress: hint_i32(&hints, "value").map(normalized_progress),
            resident: hint_bool(&hints, "resident").unwrap_or(false),
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
fn allocate(next: &mut u32, replaces: u32) -> u32 {
    match replaces {
        0 => {
            let id = *next;
            *next = next.wrapping_add(1).max(1);
            id
        }
        replaces => replaces,
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

fn local_image_path(image: Option<&str>) -> Option<String> {
    let image = glimpse_utils::text::clean(image?, IMAGE_PATH_MAX_CHARS);
    let path = image.strip_prefix("file://").unwrap_or(&image);
    Path::new(path).is_absolute().then(|| path.to_owned())
}

fn hint_i32(hints: &HashMap<String, OwnedValue>, name: &str) -> Option<i32> {
    hints.get(name).and_then(|value| i32::try_from(value).ok())
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
mod tests {
    use glimpse_dbus::Buses;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::service::ServiceRuntime;

    fn incoming(app: &str, summary: &str) -> Incoming {
        Incoming {
            app_name: app.to_owned(),
            app_id: app.to_owned(),
            app_pid: None,
            icon: None,
            image: None,
            summary: summary.to_owned(),
            body: String::new(),
            actions: Vec::new(),
            urgency: NotificationUrgency::Normal,
            progress: None,
            resident: false,
        }
    }

    fn store(keep: usize, posted: &[(u32, &str, &str)]) -> Store {
        let mut store = Store::new(keep);
        for (id, app, summary) in posted {
            store.post(*id, incoming(app, summary));
        }
        store
    }

    #[test]
    fn image_hints_remain_content_images() {
        let app_image = record(
            1,
            Incoming {
                app_id: "org.example.Chat".to_owned(),
                icon: Some("org.example.Chat".to_owned()),
                image: Some("file:///tmp/org.example.Chat.png".to_owned()),
                ..incoming("Chat", "Marta")
            },
        );
        assert_eq!(
            app_image.image.as_deref(),
            Some("/tmp/org.example.Chat.png")
        );

        let screenshot = record(
            2,
            Incoming {
                image: Some("/tmp/Screenshot_2026-09-12.png".to_owned()),
                ..incoming("Screenshots", "Screenshot captured")
            },
        );
        assert_eq!(
            screenshot.image.as_deref(),
            Some("/tmp/Screenshot_2026-09-12.png")
        );
    }

    #[test]
    fn a_sender_naming_replaces_id_writes_over_the_notification_rather_than_sending_another() {
        let mut held = store(10, &[(1, "Telegram", "First"), (2, "PagerDuty", "Other")]);
        held.post(1, incoming("Telegram", "Edited"));

        let records = held.records();
        assert_eq!(
            records.len(),
            2,
            "a replacement is not a second notification"
        );
        let first = records
            .iter()
            .find(|record| record.id == 1)
            .expect("the replaced notification is still held");
        assert_eq!(first.summary, "Edited");
    }

    /// Newest first, so the bound has to drop from the far end. Truncating the wrong end would
    /// throw away exactly what just arrived, which no test of the length alone would catch.
    #[test]
    fn the_bound_drops_only_old_read_history() {
        let mut held = store(
            2,
            &[(1, "A", "oldest"), (2, "B", "middle"), (3, "C", "newest")],
        );

        assert_eq!(
            held.records().len(),
            3,
            "unread notifications are never evicted"
        );
        assert!(held.dismiss(1));
        assert!(held.dismiss(2));
        assert!(held.dismiss(3));

        let summaries: Vec<_> = held
            .records()
            .into_iter()
            .map(|record| record.summary)
            .collect();
        assert_eq!(summaries, ["newest", "middle"]);
    }

    #[test]
    fn suppression_patterns_match_application_title_and_body_before_storage() {
        let patterns = [
            "(?i)^org\\.example$".to_owned(),
            "(?i)build succeeded".to_owned(),
            "secret token".to_owned(),
        ];
        let mut held = Store::with_suppression(10, &patterns);

        assert!(!held.post(
            1,
            Incoming {
                app_id: "org.example".to_owned(),
                ..incoming("Friendly name", "Visible title")
            }
        ));
        assert!(!held.post(
            2,
            Incoming {
                summary: "Build succeeded".to_owned(),
                ..incoming("Builder", "unimportant")
            }
        ));
        assert!(!held.post(
            3,
            Incoming {
                body: "A secret token arrived".to_owned(),
                ..incoming("Builder", "unimportant")
            }
        ));
        assert!(held.records().is_empty());
    }

    #[test]
    fn an_invalid_suppression_pattern_does_not_disable_valid_patterns() {
        let patterns = ["[".to_owned(), "quiet".to_owned()];
        let mut held = Store::with_suppression(10, &patterns);

        assert!(!held.post(1, incoming("quiet", "shown")));
        assert!(held.post(2, incoming("loud", "shown")));
        assert_eq!(held.records().len(), 1);
    }

    #[test]
    fn adding_a_suppression_pattern_removes_matching_history() {
        let mut held = store(10, &[(1, "Chat", "Keep"), (2, "Build", "Done")]);

        assert!(held.reconfigure(10, &["Build".to_owned()]));
        assert_eq!(
            held.records()
                .iter()
                .map(|record| record.id)
                .collect::<Vec<_>>(),
            [1]
        );
    }

    #[test]
    fn lowering_the_bound_takes_effect_without_waiting_for_another_notification() {
        let mut held = store(10, &[(1, "A", "one"), (2, "B", "two"), (3, "C", "three")]);
        assert!(held.dismiss(1));
        assert!(held.dismiss(2));
        assert!(held.dismiss(3));
        assert!(
            held.reconfigure(1, &[]),
            "the store shrank, so the state must be republished"
        );
        assert_eq!(held.records().len(), 1);
        assert!(
            !held.reconfigure(1, &[]),
            "a bound that changes nothing must not force a republish"
        );
    }

    #[test]
    fn clearing_one_app_leaves_every_other_app_alone() {
        let mut held = store(
            10,
            &[
                (1, "Telegram", "a"),
                (2, "PagerDuty", "b"),
                (3, "Telegram", "c"),
            ],
        );

        let gone = held.drain(|record| record.app_id == "Telegram");
        assert_eq!(gone, [3, 1], "every cleared id is reported, newest first");
        assert_eq!(held.records().len(), 1);
        assert_eq!(held.records()[0].app_id, "PagerDuty");
    }

    #[test]
    fn an_action_is_invocable_only_on_a_notification_that_offered_it() {
        let mut held = Store::new(10);
        held.post(
            1,
            Incoming {
                actions: vec![("reply".to_owned(), "Reply".to_owned())],
                ..incoming("Telegram", "Marta")
            },
        );

        assert_eq!(held.offered(1, "reply"), Some(false));
        assert!(
            held.offered(1, "detonate").is_none(),
            "an action nobody offered is refused"
        );
        assert!(
            held.offered(2, "reply").is_none(),
            "an id nobody holds is refused"
        );
    }

    #[test]
    fn dismissing_retains_a_read_history_entry_and_disables_its_actions() {
        let mut held = Store::new(10);
        held.post(
            1,
            Incoming {
                actions: vec![(DEFAULT_ACTION.to_owned(), "Open".to_owned())],
                ..incoming("Telegram", "Marta")
            },
        );

        assert!(held.dismiss(1));
        assert!(!held.records()[0].unread);
        assert!(held.offered(1, DEFAULT_ACTION).is_none());
        assert!(!held.dismiss(1), "a read entry is not closed twice");
    }

    #[test]
    fn activation_reports_the_default_action_and_resident_policy_without_mutating() {
        let mut held = Store::new(10);
        held.post(1, incoming("Mail", "No action"));
        held.post(
            2,
            Incoming {
                actions: vec![(DEFAULT_ACTION.to_owned(), "Open".to_owned())],
                resident: true,
                ..incoming("Telegram", "Marta")
            },
        );

        assert_eq!(
            held.activation(1),
            Some(Activation {
                invoke_default: false,
                resident: false,
            })
        );
        assert_eq!(
            held.activation(2),
            Some(Activation {
                invoke_default: true,
                resident: true,
            })
        );
        assert!(held.records().iter().all(|record| record.unread));
    }

    #[test]
    fn clearing_history_does_not_report_an_already_closed_id_again() {
        let mut held = store(10, &[(1, "Telegram", "Marta"), (2, "Mail", "New")]);
        assert!(held.dismiss(1));

        assert_eq!(held.drain(|_| true), [2]);
        assert!(held.records().is_empty());
    }

    #[test]
    fn the_default_action_survives_the_named_button_cap_and_long_keys_are_dropped() {
        let too_long = "x".repeat(ACTION_KEY_MAX_CHARS + 1);
        let record = record(
            1,
            Incoming {
                actions: vec![
                    ("one".to_owned(), "One".to_owned()),
                    ("two".to_owned(), "Two".to_owned()),
                    ("three".to_owned(), "Three".to_owned()),
                    ("four".to_owned(), "Four".to_owned()),
                    (too_long, "Wrong".to_owned()),
                    (DEFAULT_ACTION.to_owned(), "Open".to_owned()),
                ],
                ..incoming("Telegram", "Marta")
            },
        );

        let keys: Vec<_> = record
            .actions
            .iter()
            .map(|action| action.key.as_str())
            .collect();
        assert_eq!(keys, [DEFAULT_ACTION, "one", "two", "three"]);
    }

    #[test]
    fn progress_is_bounded_at_the_bus_and_record_boundaries() {
        assert_eq!(normalized_progress(-20), 0.0);
        assert_eq!(normalized_progress(25), 0.25);
        assert_eq!(normalized_progress(250), 1.0);

        let record = record(
            1,
            Incoming {
                progress: Some(2.5),
                ..incoming("Build", "Compiling")
            },
        );
        assert_eq!(record.progress, Some(1.0));
    }

    #[tokio::test]
    async fn invoking_a_non_resident_action_closes_it_into_history() {
        let (handle, sender, mut state, cancel, running) = dnd_service().await;

        sender
            .send(Input::Event(Event::Posted {
                id: 1,
                incoming: Box::new(Incoming {
                    actions: vec![(DEFAULT_ACTION.to_owned(), "Open".to_owned())],
                    ..incoming("Telegram", "Marta")
                }),
            }))
            .await
            .expect("posted");
        state
            .wait_for(|state| {
                state.list.as_ref().is_some_and(|list| {
                    list.notifications
                        .iter()
                        .any(|notification| notification.id == 1 && notification.unread)
                })
            })
            .await
            .expect("posted state");
        handle
            .invoke_action(1, DEFAULT_ACTION.to_owned(), None)
            .await
            .expect("invoked");
        state
            .wait_for(|state| {
                state.list.as_ref().is_some_and(|list| {
                    list.notifications
                        .iter()
                        .any(|notification| notification.id == 1 && !notification.unread)
                })
            })
            .await
            .expect("dismissed state");
        let latest = handle.snapshot();
        cancel.cancel();
        running.await.expect("joined").expect("stopped");

        let list = latest.list.expect("a list was published");
        assert_eq!(list.notifications.len(), 1);
        assert!(!list.notifications[0].unread);
    }

    async fn dnd_service() -> (
        NotificationsHandle,
        crate::service::ServiceSender<Notifications>,
        tokio::sync::watch::Receiver<NotificationsState>,
        CancellationToken,
        tokio::task::JoinHandle<Result<(), ServiceError>>,
    ) {
        let cancel = CancellationToken::new();
        let (mut runtime, handle) = ServiceRuntime::<Notifications>::new(
            initial_state(),
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        let state = handle.subscribe();
        let sender = runtime.sender();
        let running = tokio::spawn(async move {
            runtime
                .run(
                    Config {
                        keep: 10,
                        suppress: Vec::new(),
                    },
                    (),
                )
                .await
        });
        (handle, sender, state, cancel, running)
    }

    fn enabled(state: &NotificationsState) -> bool {
        state.dnd.as_ref().is_some_and(|dnd| dnd.dnd.enabled)
    }

    #[tokio::test]
    async fn do_not_disturb_with_an_expiry_lapses_without_anyone_turning_it_off() {
        let (handle, _sender, mut state, cancel, running) = dnd_service().await;

        handle
            .set_dnd(DoNotDisturb {
                enabled: true,
                until: Some(Utc::now() + chrono::TimeDelta::milliseconds(250)),
            })
            .await
            .expect("do not disturb is set");
        state.wait_for(enabled).await.expect("it is on");

        state
            .wait_for(|state| !enabled(state))
            .await
            .expect("the expiry passes and it turns itself off");

        let latest = handle.snapshot();
        cancel.cancel();
        running.await.expect("joined").expect("stopped");

        let dnd = latest.dnd.expect("do not disturb was published").dnd;
        assert!(!dnd.enabled);
        assert_eq!(dnd.until, None, "a spent expiry is not left behind");
    }

    #[tokio::test]
    async fn do_not_disturb_without_an_expiry_has_nothing_that_could_lapse() {
        let (handle, sender, mut state, cancel, running) = dnd_service().await;

        handle
            .set_dnd(DoNotDisturb {
                enabled: true,
                until: None,
            })
            .await
            .expect("do not disturb is set");
        state.wait_for(enabled).await.expect("it is on");

        sender
            .send(Input::Event(Event::DoNotDisturbLapsed(Utc::now())))
            .await
            .expect("a deadline belonging to no window at all");
        handle
            .clear_all()
            .await
            .expect("a command answered after it proves the event was handled");

        let latest = handle.snapshot();
        cancel.cancel();
        running.await.expect("joined").expect("stopped");

        assert!(
            enabled(&latest),
            "nothing armed an expiry, so nothing can spend one"
        );
    }

    #[tokio::test]
    async fn a_spent_expiry_cannot_cancel_the_window_that_replaced_it() {
        let (handle, sender, mut state, cancel, running) = dnd_service().await;
        let extended = Utc::now() + chrono::TimeDelta::hours(4);

        handle
            .set_dnd(DoNotDisturb {
                enabled: true,
                until: Some(extended),
            })
            .await
            .expect("do not disturb is set");
        state.wait_for(enabled).await.expect("it is on");

        sender
            .send(Input::Event(Event::DoNotDisturbLapsed(
                Utc::now() - chrono::TimeDelta::hours(1),
            )))
            .await
            .expect("a timer that was already torn down fires late");
        handle
            .clear_all()
            .await
            .expect("a command answered after it proves the event was handled");

        let latest = handle.snapshot();
        cancel.cancel();
        running.await.expect("joined").expect("stopped");

        let dnd = latest.dnd.expect("do not disturb was published").dnd;
        assert!(
            dnd.enabled,
            "the stale deadline belonged to a window that no longer exists"
        );
        assert_eq!(dnd.until, Some(extended));
    }

    #[tokio::test]
    async fn an_expiry_that_has_already_passed_lapses_rather_than_sticking() {
        let (handle, _sender, mut state, cancel, running) = dnd_service().await;

        handle
            .set_dnd(DoNotDisturb {
                enabled: true,
                until: Some(Utc::now() - chrono::TimeDelta::hours(1)),
            })
            .await
            .expect("do not disturb is set");

        state
            .wait_for(|state| !enabled(state))
            .await
            .expect("an expiry in the past is due immediately");

        cancel.cancel();
        running.await.expect("joined").expect("stopped");
    }

    #[test]
    fn a_body_is_sanitised_on_the_way_in_and_an_empty_one_is_absent_rather_than_blank() {
        let marked = record(
            1,
            Incoming {
                body: "<b>Marta</b> sent <script>alert(1)</script>a file".to_owned(),
                ..incoming("Telegram", "Marta")
            },
        );
        let body = marked.body.expect("a body that has text");
        assert!(
            body.contains("<b>Marta</b>"),
            "the markup a sender may send survives: {body}"
        );
        assert!(
            !body.contains("script"),
            "the markup it may not does not: {body}"
        );

        let blank = record(
            2,
            Incoming {
                body: "   ".to_owned(),
                ..incoming("Telegram", "Marta")
            },
        );
        assert_eq!(blank.body, None);
    }

    #[test]
    fn the_display_name_does_not_choose_the_group_identity() {
        let declared = record(
            2,
            Incoming {
                app_id: "org.telegram.desktop".to_owned(),
                ..incoming("Telegram", "Marta")
            },
        );
        assert_eq!(
            declared.app_id, "org.telegram.desktop",
            "grouping keys on the sender's identity, not on the name it chose"
        );
    }

    #[test]
    fn a_process_desktop_entry_stabilizes_short_lived_bus_senders() {
        let first = application_id(
            None,
            Some("walz".to_owned()),
            Some(":1.195415".to_owned()),
            1,
        );
        let second = application_id(
            None,
            Some("walz".to_owned()),
            Some(":1.197442".to_owned()),
            2,
        );

        assert_eq!(first, "walz");
        assert_eq!(second, first);
        assert_eq!(
            application_id(
                Some("org.example.Chat".to_owned()),
                Some("walz".to_owned()),
                Some(":1.197442".to_owned()),
                3,
            ),
            "org.example.Chat"
        );
    }

    #[test]
    fn an_empty_app_name_falls_back_to_the_desktop_identity() {
        let mut held = Store::new(10);
        assert!(held.post(
            48,
            Incoming {
                app_name: String::new(),
                app_id: "com.mitchellh.ghostty".to_owned(),
                ..incoming("", "Ghostty")
            }
        ));

        let records = held.records();
        assert!(
            !records[0].app_name.is_empty(),
            "Ghostty supplies a desktop entry but leaves the freedesktop app-name argument empty"
        );
    }

    #[test]
    fn a_declared_app_name_remains_authoritative() {
        assert_eq!(
            application_name("Ghostty Developer Build", "com.mitchellh.ghostty"),
            "Ghostty Developer Build"
        );
    }

    #[test]
    fn more_actions_than_the_cap_are_trimmed_and_a_keyless_one_is_dropped() {
        let many = record(
            1,
            Incoming {
                actions: vec![
                    ("a".to_owned(), "A".to_owned()),
                    ("b".to_owned(), "B".to_owned()),
                    ("c".to_owned(), "C".to_owned()),
                    ("d".to_owned(), "D".to_owned()),
                ],
                ..incoming("Telegram", "Marta")
            },
        );
        assert_eq!(many.actions.len(), ACTIONS_MAX);

        let keyless = record(
            2,
            Incoming {
                actions: vec![(String::new(), "Nameless".to_owned())],
                ..incoming("Telegram", "Marta")
            },
        );
        assert!(
            keyless.actions.is_empty(),
            "an action with no key can never be sent back"
        );
    }

    /// The wire carries actions as one flat list of alternating key and label, so an odd length is
    /// a sender's mistake rather than a final action with no label.
    #[test]
    fn actions_arrive_as_pairs_and_an_odd_tail_is_dropped() {
        assert_eq!(
            pairs(vec![
                "reply".to_owned(),
                "Reply".to_owned(),
                "orphan".to_owned()
            ]),
            [("reply".to_owned(), "Reply".to_owned())]
        );
        assert!(pairs(Vec::new()).is_empty());
    }

    /// `clean` spends the cap on content and then adds `…` as a marker, so an overflowing string
    /// is `cap + 1` characters. Asserting the raw length would pin the marker rather than the
    /// budget, and would move the day the marker changes.
    #[test]
    fn a_summary_from_a_sender_is_capped_and_says_that_it_was() {
        let long = record(1, incoming("Telegram", &"A".repeat(SUMMARY_MAX_CHARS * 2)));

        assert!(long.summary.ends_with('…'));
        assert_eq!(
            long.summary.chars().filter(|glyph| *glyph != '…').count(),
            SUMMARY_MAX_CHARS
        );
    }

    /// A summary is somebody else's text and need not be ASCII. Capping by bytes would cut inside
    /// a character, which is a panic reachable by anyone who can send a notification.
    #[test]
    fn a_multi_byte_summary_is_capped_by_characters_rather_than_by_bytes() {
        let long = record(1, incoming("Telegram", &"é".repeat(SUMMARY_MAX_CHARS * 2)));

        assert_eq!(
            long.summary.chars().filter(|glyph| *glyph != '…').count(),
            SUMMARY_MAX_CHARS
        );
        assert!(
            long.summary.len() > SUMMARY_MAX_CHARS,
            "each character is two bytes here"
        );
    }

    #[test]
    fn a_token_that_is_empty_or_over_the_bound_is_dropped_rather_than_shortened() {
        assert_eq!(
            valid_activation_token(Some("tok-1".to_owned())).as_deref(),
            Some("tok-1")
        );
        assert_eq!(
            valid_activation_token(None),
            None,
            "a client that cannot mint one leaves it out"
        );
        assert_eq!(valid_activation_token(Some(String::new())), None);
        assert_eq!(
            valid_activation_token(Some("t".repeat(ACTIVATION_TOKEN_MAX_CHARS + 1))),
            None,
            "an overlong token is refused rather than truncated into a wrong one"
        );
    }

    #[test]
    fn a_notification_carries_the_pid_of_the_process_that_sent_it() {
        let held = record(
            1,
            Incoming {
                app_pid: Some(4265),
                ..incoming("Telegram", "Marta")
            },
        );

        assert_eq!(held.app_pid, Some(4265));
    }

    #[test]
    fn ids_are_handed_out_in_order_and_a_sender_naming_one_gets_it_back() {
        let mut next = 1;

        assert_eq!(allocate(&mut next, 0), 1);
        assert_eq!(allocate(&mut next, 0), 2);
        assert_eq!(allocate(&mut next, 7), 7);
        assert_eq!(
            allocate(&mut next, 0),
            3,
            "naming a replacement does not spend an id"
        );
    }

    /// Zero means "new", so handing it out would make the next sender's replacement read as a
    /// fresh notification.
    #[test]
    fn the_id_counter_steps_over_zero_when_it_wraps() {
        let mut next = u32::MAX;

        assert_eq!(allocate(&mut next, 0), u32::MAX);
        assert_eq!(allocate(&mut next, 0), 1);
    }
}
