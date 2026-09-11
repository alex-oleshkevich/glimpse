use std::collections::{HashMap, VecDeque};

use chrono::Utc;
use glimpse_contracts::{
    Command as _, DEFAULT_ACTION, DoNotDisturb, Message, NotificationAction, NotificationRecord,
    NotificationUrgency, NotificationsActivate, NotificationsClearAll, NotificationsClearApp,
    NotificationsDismiss, NotificationsDnd, NotificationsInvokeAction, NotificationsList,
    NotificationsRemove, NotificationsSetDnd,
};
use glimpse_ipc::{CallError, ErrorCode};
use serde_json::Value;
use tokio::sync::mpsc;
use zbus::fdo::DBusProxy;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;
use zbus::{Connection, interface};

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{Input, Service, ServiceError, decode_args, unknown_command},
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
const ACTIVATION_TOKEN_MAX_CHARS: usize = 512;

/// `NotificationClosed` reasons, as the specification numbers them.
const CLOSED_BY_SENDER: u32 = 3;
const CLOSED_BY_READER: u32 = 2;

pub struct Incoming {
    pub app_name: String,
    pub app_id: String,
    pub app_pid: Option<i32>,
    pub icon: Option<String>,
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
}

#[derive(Debug)]
pub enum Command {
    Dismiss {
        id: u32,
    },
    Remove {
        id: u32,
    },
    Activate {
        id: u32,
        token: Option<String>,
    },
    InvokeAction {
        id: u32,
        action: String,
        token: Option<String>,
    },
    ClearApp {
        app_id: String,
    },
    ClearAll,
    SetDnd {
        dnd: DoNotDisturb,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    keep: usize,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            keep: document.notifications.keep.clamp(1, 1000) as usize,
        }
    }
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch {}

/// glimpsed is the store here, not a mirror: nothing else on the session bus holds these, so a
/// notification exists exactly as long as this service keeps it.
pub struct Notifications {
    list: Publisher<NotificationsList>,
    quiet: Publisher<NotificationsDnd>,
    store: Store,
    dnd: DoNotDisturb,
    connection: Option<Connection>,
}

/// Everything the service decides about what to keep, with no publisher and no bus in it, so the
/// bound and the replace rule are ordinary tests rather than something only a live daemon shows.
#[derive(Debug, Default)]
pub struct Store {
    held: VecDeque<NotificationRecord>,
    keep: usize,
}

impl Service for Notifications {
    const NAME: &'static str = "notifications";
    const TOPICS: &'static [&'static str] = &[NotificationsList::NAME, NotificationsDnd::NAME];
    const METHODS: &'static [&'static str] = &[
        NotificationsDismiss::NAME,
        NotificationsRemove::NAME,
        NotificationsActivate::NAME,
        NotificationsInvokeAction::NAME,
        NotificationsClearApp::NAME,
        NotificationsClearAll::NAME,
        NotificationsSetDnd::NAME,
    ];

    type Config = Config;
    type Command = Command;
    type Event = Event;
    type SubKey = Watch;

    fn decode(method: &str, args: Value) -> Result<Self::Command, CallError> {
        match method {
            NotificationsDismiss::NAME => {
                let NotificationsDismiss { id } = decode_args(args)?;
                Ok(Command::Dismiss { id })
            }
            NotificationsRemove::NAME => {
                let NotificationsRemove { id } = decode_args(args)?;
                Ok(Command::Remove { id })
            }
            NotificationsActivate::NAME => {
                let NotificationsActivate {
                    id,
                    activation_token,
                } = decode_args(args)?;
                Ok(Command::Activate {
                    id,
                    token: valid_activation_token(activation_token),
                })
            }
            NotificationsInvokeAction::NAME => {
                let NotificationsInvokeAction {
                    id,
                    action,
                    activation_token,
                } = decode_args(args)?;
                Ok(Command::InvokeAction {
                    id,
                    action,
                    token: valid_activation_token(activation_token),
                })
            }
            NotificationsClearApp::NAME => {
                let NotificationsClearApp { app_id } = decode_args(args)?;
                Ok(Command::ClearApp { app_id })
            }
            NotificationsClearAll::NAME => Ok(Command::ClearAll),
            NotificationsSetDnd::NAME => {
                let NotificationsSetDnd { dnd } = decode_args(args)?;
                Ok(Command::SetDnd { dnd })
            }
            _ => Err(unknown_command(Self::NAME, method)),
        }
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        Vec::new()
    }

    async fn start(ctx: &Ctx<Self>, config: Self::Config) -> Result<Self, ServiceError> {
        let mut service = Self {
            list: ctx.publisher::<NotificationsList>(),
            quiet: ctx.publisher::<NotificationsDnd>(),
            store: Store::new(config.keep),
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
                self.store.post(id, *incoming);
                self.publish();
            }
            Input::Event(Event::Retracted { id }) => self.remove(id, CLOSED_BY_SENDER).await,
            Input::Command(Command::Dismiss { id }, responder) => {
                if self.store.dismiss(id) {
                    self.publish();
                    self.closed(id, CLOSED_BY_READER).await;
                }
                responder.ok(());
            }
            Input::Command(Command::Remove { id }, responder) => {
                self.remove(id, CLOSED_BY_READER).await;
                responder.ok(());
            }
            Input::Command(Command::Activate { id, token }, responder) => {
                if let Some(activation) = self.store.activation(id) {
                    if activation.invoke_default {
                        self.invoked(id, DEFAULT_ACTION, token.as_deref()).await;
                    }
                    if !activation.resident && self.store.dismiss(id) {
                        self.publish();
                        self.closed(id, CLOSED_BY_READER).await;
                    }
                }
                responder.ok(());
            }
            Input::Command(Command::InvokeAction { id, action, token }, responder) => {
                match self.store.offered(id, &action) {
                    Some(resident) => {
                        self.invoked(id, &action, token.as_deref()).await;
                        if !resident && self.store.dismiss(id) {
                            self.publish();
                            self.closed(id, CLOSED_BY_READER).await;
                        }
                        responder.ok(());
                    }
                    None => responder.fail(CallError::new(
                        ErrorCode::InvalidArgs,
                        format!("no notification {id} offering action {action}"),
                    )),
                }
            }
            Input::Command(Command::ClearApp { app_id }, responder) => {
                let gone = self.store.drain(|record| record.app_id == app_id);
                self.publish();
                for id in gone {
                    self.closed(id, CLOSED_BY_READER).await;
                }
                responder.ok(());
            }
            Input::Command(Command::ClearAll, responder) => {
                let gone = self.store.drain(|_| true);
                self.publish();
                for id in gone {
                    self.closed(id, CLOSED_BY_READER).await;
                }
                responder.ok(());
            }
            Input::Command(Command::SetDnd { dnd }, responder) => {
                self.dnd = dnd;
                self.quiet.set(NotificationsDnd { dnd: self.dnd });
                responder.ok(());
            }
            Input::Config(config) => {
                if self.store.rebound(config.keep) {
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
        }
    }

    fn post(&mut self, id: u32, incoming: Incoming) {
        let record = record(id, incoming);
        match self.held.iter().position(|held| held.id == id) {
            Some(at) => self.held[at] = record,
            None => self.held.push_front(record),
        }
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

    fn rebound(&mut self, keep: usize) -> bool {
        self.keep = keep;
        self.bound()
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

#[derive(Debug, PartialEq, Eq)]
struct Activation {
    invoke_default: bool,
    resident: bool,
}

impl Notifications {
    fn publish(&mut self) {
        self.list.set(NotificationsList {
            notifications: self.store.records(),
        });
        self.quiet.set(NotificationsDnd { dnd: self.dnd });
    }

    /// A signal carries no reply, so emitting one inside the handler costs a socket write rather
    /// than a round trip and does not need the `Responder` moved into a spawn.
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
        summary,
        body,
        actions,
        urgency,
        progress,
        resident,
    } = incoming;

    let app_name = glimpse_utils::text::clean(&app_name, APP_NAME_MAX_CHARS);
    let app_id = glimpse_utils::text::clean(&app_id, APP_NAME_MAX_CHARS);

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
        image: None,
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

        let app_id = hint_str(&hints, "desktop-entry")
            .filter(|app_id| !app_id.is_empty())
            .or_else(|| header.sender().map(ToString::to_string))
            .unwrap_or_else(|| format!("notification-{id}"));
        let incoming = Incoming {
            app_name,
            app_id,
            app_pid: self.sender_pid(&header).await,
            icon: (!app_icon.is_empty()).then_some(app_icon),
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
    use std::sync::Arc;

    use glimpse_dbus::Buses;
    use tokio::sync::oneshot;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{BrokerHandle, MockBroker, broker::Responder, service::ServiceRuntime};

    fn incoming(app: &str, summary: &str) -> Incoming {
        Incoming {
            app_name: app.to_owned(),
            app_id: app.to_owned(),
            app_pid: None,
            icon: None,
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
    fn declared_topics_and_methods_exist() {
        crate::service::assert_declarations::<Notifications>();
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
    fn lowering_the_bound_takes_effect_without_waiting_for_another_notification() {
        let mut held = store(10, &[(1, "A", "one"), (2, "B", "two"), (3, "C", "three")]);
        assert!(held.dismiss(1));
        assert!(held.dismiss(2));
        assert!(held.dismiss(3));
        assert!(
            held.rebound(1),
            "the store shrank, so the topic must be republished"
        );
        assert_eq!(held.records().len(), 1);
        assert!(
            !held.rebound(1),
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
        let mock = Arc::new(MockBroker::default());
        let broker: Arc<dyn BrokerHandle> = mock.clone();
        let cancel = CancellationToken::new();
        let mut runtime = ServiceRuntime::<Notifications>::new(
            broker,
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        let sender = runtime.sender();
        let running = tokio::spawn(async move { runtime.run(Config { keep: 10 }).await });

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
        let (reply, answer) = oneshot::channel();
        sender.dispatch(
            Command::InvokeAction {
                id: 1,
                action: DEFAULT_ACTION.to_owned(),
                token: None,
            },
            Responder::new(reply),
        );
        answer.await.expect("answered").expect("invoked");
        cancel.cancel();
        running.await.expect("joined").expect("stopped");

        let latest = mock
            .published()
            .into_iter()
            .filter(|(topic, _)| topic == NotificationsList::NAME)
            .filter_map(|(_, value)| serde_json::from_value::<NotificationsList>(value).ok())
            .next_back()
            .expect("a list was published");
        assert_eq!(latest.notifications.len(), 1);
        assert!(!latest.notifications[0].unread);
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
    fn a_name_the_service_does_not_declare_is_refused() {
        let error =
            Notifications::decode("notifications.detonate", Value::Null).expect_err("refused");
        assert_eq!(error.code, ErrorCode::UnknownCommand);
    }

    #[test]
    fn a_mistyped_argument_is_refused_as_an_argument_not_as_a_missing_command() {
        let error = Notifications::decode(
            NotificationsDismiss::NAME,
            serde_json::json!({ "id": "first" }),
        )
        .expect_err("refused");
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }

    /// A token is a capability the compositor accepts whole or refuses, so shortening one the way
    /// every other cap here does would hand back a token that fails for reasons nobody can see.
    /// The specification makes the signal optional, so dropping is legal where mangling is not.
    #[test]
    fn a_token_that_is_empty_or_over_the_bound_is_dropped_rather_than_shortened() {
        let decoded = |args: Value| {
            let Ok(Command::InvokeAction { token, .. }) =
                Notifications::decode(NotificationsInvokeAction::NAME, args)
            else {
                panic!("invoke_action decodes to an InvokeAction");
            };
            token
        };
        let with = |token: &str| serde_json::json!({ "id": 1, "action": "default", "activation_token": token });

        assert_eq!(decoded(with("tok-1")).as_deref(), Some("tok-1"));
        assert_eq!(
            decoded(serde_json::json!({ "id": 1, "action": "default" })),
            None,
            "a client that cannot mint one leaves it out"
        );
        assert_eq!(decoded(with("")), None);
        assert_eq!(
            decoded(with(&"t".repeat(ACTIVATION_TOKEN_MAX_CHARS + 1))),
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
