mod catalog;
mod sandbox;
mod scope;
mod spawn;
mod tree;
mod wire;

use std::collections::{BTreeMap, BTreeSet};
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use chrono::Utc;
use glimpse_dbus::notifications::{NotificationUrgency, NotificationsProviderHandle};
use serde_json::{Map, Value};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

use crate::selection::{Offer as SelectionOffer, Selection};
use crate::{CommandError, Ctx, Input, Service, ServiceEndpoint, ServiceError, Sub};

pub use sandbox::{exec_confine, exec_sandbox, exec_state_dir};
pub use scope::{unit_name, unit_pattern};
pub use spawn::{EXEC_HELLO_WINDOW, EXEC_MAX_HELLOS};

pub use catalog::{Catalog, DesktopCatalog, Entry, INTERFACE, expand};
pub use tree::{
    Align, BoxProps, ButtonProps, ClassName, Element, ElementKind, Ellipsize, EntryProps,
    FaderProps, FooterProps, HeroProps, ImageProps, IndicatorProps, LabelProps,
    MAX_CHILDREN_PER_PARENT, MAX_NODES, Node, PlaceholderProps, PopoverProps, ProgressProps, ROOT,
    RowProps, ScaleProps, SectionProps, SeparatorProps, Severity, SpinnerProps, SwitchProps,
    SwitchRowProps, Tree, Violation,
};
pub use wire::{
    Edge, FromApplet, MAX_LINE, Op, Orientation, Outgoing, Placement, SessionVerb, Urgency,
    WireNode, Zone,
};

const MAX_UPDATES_PER_SECOND: u32 = 120;
const RETRY: Duration = Duration::from_secs(5);
const OFFER_TTL: Duration = Duration::from_secs(5);
const GESTURE: Duration = Duration::from_secs(2);
const TEXT: &str = "text/plain;charset=utf-8";
const URI_CAP: usize = 2048;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExecState {
    pub slots: BTreeMap<u64, SlotState>,
    pub offers: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SlotState {
    pub applet: String,
    pub generation: u64,
    pub tree: Arc<Tree>,
    pub status: Status,
    pub title: String,
    pub icon: Option<String>,
    pub requests: Vec<(u64, BarRequest)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Starting,
    Running,
    Ended,
    Restarting { attempt: u32 },
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BarRequest {
    OpenUri(String),
    Session(SessionVerb),
    ClosePopover,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub applets: BTreeMap<String, glimpse_config::ExecAppletConfig>,
    pub state_base: Option<PathBuf>,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            state_base: glimpse_config::state_dir(),
            applets: glimpse_config::placed_applets(document)
                .filter_map(|(name, applet)| match applet.kind {
                    glimpse_config::AppletKind::Exec(exec) => Some((name.to_owned(), *exec)),
                    _ => None,
                })
                .collect(),
        }
    }
}

pub struct Dependencies {
    pub catalog: Arc<dyn Catalog>,
    pub selection: Arc<dyn Selection>,
    pub notifications: NotificationsProviderHandle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Spawned,
    Attached,
}

pub struct Pipes {
    pub to_applet: OwnedFd,
    pub from_applet: OwnedFd,
}

pub struct Offer {
    pub output: Option<String>,
    pub position: Option<Edge>,
    pub entry: Entry,
    pub options: Map<String, Value>,
    pub pipes: Pipes,
}

struct StoredOffer {
    offer: Offer,
    offered_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SubKey {
    Child { slot: u64, spawn: u64 },
    Backoff { slot: u64, spawn: u64 },
    Offer { applet: String, offered_at: Instant },
    Retry,
}

pub enum Event {
    Spawned {
        slot: u64,
        spawn: u64,
        link: Link,
        entry: Entry,
    },
    Hello {
        slot: u64,
        spawn: u64,
        state: String,
    },
    Tree {
        slot: u64,
        spawn: u64,
        tree: Arc<Tree>,
    },
    Request {
        slot: u64,
        spawn: u64,
        request: FromApplet,
    },
    Exited {
        slot: u64,
        spawn: u64,
        spoke: bool,
        ran: Duration,
        reason: String,
        resolved: Option<Entry>,
    },
    Ended {
        slot: u64,
        spawn: u64,
    },
    Backoff {
        slot: u64,
    },
    OfferExpired {
        applet: String,
        offered_at: Instant,
    },
    Retry,
    RetryReady {
        slot: u64,
        spawn: u64,
        resolved: Option<Result<Entry, String>>,
    },
}

#[derive(Clone)]
pub struct Link {
    pub tx: mpsc::Sender<Outgoing>,
    pub stop: CancellationToken,
    pub queue_full: Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Debug, Clone)]
pub struct UserEvent {
    pub slot: u64,
    pub generation: u64,
    pub id: u32,
    pub name: String,
    pub args: Vec<Value>,
    pub seq: Option<u64>,
    pub gesture: bool,
}

pub enum Command {
    Attach {
        slot: u64,
        applet: String,
        placement: Placement,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Offer {
        applet: String,
        offer: Offer,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    OfferOptions {
        applet: String,
        options: Map<String, Value>,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    SetOptions {
        slot: u64,
        options: Map<String, Value>,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Place {
        slot: u64,
        placement: Placement,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Detach {
        slot: u64,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    SendEvent {
        event: UserEvent,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Popover {
        slot: u64,
        open: bool,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
}

#[derive(Clone)]
pub struct ExecHandle(ServiceEndpoint<Exec>);

impl ExecHandle {
    pub fn snapshot(&self) -> ExecState {
        self.0.snapshot()
    }
    pub fn subscribe(&self) -> watch::Receiver<ExecState> {
        self.0.subscribe()
    }
    pub fn health(&self) -> watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    fn submit(
        &self,
        build: impl FnOnce(oneshot::Sender<Result<(), CommandError>>) -> Command,
    ) -> Pending {
        let (reply, answer) = oneshot::channel();
        (self.0.command(build(reply)), answer)
    }

    pub fn attach(
        &self,
        slot: u64,
        applet: &str,
        placement: Placement,
    ) -> impl Future<Output = Result<(), CommandError>> + Send + 'static + use<> {
        answered(self.submit(|reply| Command::Attach {
            slot,
            applet: applet.to_owned(),
            placement,
            reply,
        }))
    }
    pub fn offer(
        &self,
        applet: &str,
        offer: Offer,
    ) -> impl Future<Output = Result<(), CommandError>> + Send + 'static + use<> {
        answered(self.submit(|reply| Command::Offer {
            applet: applet.to_owned(),
            offer,
            reply,
        }))
    }
    pub fn offer_options(
        &self,
        applet: &str,
        options: Map<String, Value>,
    ) -> impl Future<Output = Result<(), CommandError>> + Send + 'static + use<> {
        answered(self.submit(|reply| Command::OfferOptions {
            applet: applet.to_owned(),
            options,
            reply,
        }))
    }
    pub fn set_options(
        &self,
        slot: u64,
        options: Map<String, Value>,
    ) -> impl Future<Output = Result<(), CommandError>> + Send + 'static + use<> {
        answered(self.submit(|reply| Command::SetOptions {
            slot,
            options,
            reply,
        }))
    }
    pub fn place(
        &self,
        slot: u64,
        placement: Placement,
    ) -> impl Future<Output = Result<(), CommandError>> + Send + 'static + use<> {
        answered(self.submit(|reply| Command::Place {
            slot,
            placement,
            reply,
        }))
    }
    pub fn detach(
        &self,
        slot: u64,
    ) -> impl Future<Output = Result<(), CommandError>> + Send + 'static + use<> {
        answered(self.submit(|reply| Command::Detach { slot, reply }))
    }
    pub fn send_event(
        &self,
        event: UserEvent,
    ) -> impl Future<Output = Result<(), CommandError>> + Send + 'static + use<> {
        answered(self.submit(|reply| Command::SendEvent { event, reply }))
    }
    pub fn popover(
        &self,
        slot: u64,
        open: bool,
    ) -> impl Future<Output = Result<(), CommandError>> + Send + 'static + use<> {
        answered(self.submit(|reply| Command::Popover { slot, open, reply }))
    }
}

type Pending = (
    Result<(), CommandError>,
    oneshot::Receiver<Result<(), CommandError>>,
);

async fn answered((sent, answer): Pending) -> Result<(), CommandError> {
    sent?;
    answer
        .await
        .map_err(|_| CommandError::Unavailable("`exec` stopped before answering".to_owned()))?
}

struct Slot {
    origin: Origin,
    pipes: Option<Arc<tokio::sync::Mutex<Option<Pipes>>>>,
    public: SlotState,
    placement: Placement,
    applet_id: String,
    sandbox: glimpse_config::AppletSandbox,
    state: Option<PathBuf>,
    entry: Option<Entry>,
    catalog_refused: bool,
    options: Map<String, Value>,
    spawn: u64,
    attempt: u32,
    backoff_until: Option<chrono::DateTime<Utc>>,
    link: Option<Link>,
    stop: CancellationToken,
    popover_open: bool,
    last_gesture: Option<Instant>,
    last_notify: Option<Instant>,
    warned_gate: bool,
    last_failed_reason: Option<String>,
    serial: u64,
    failed_at: Option<Instant>,
    failures: u32,
    catalog_misses: u32,
    resolving: bool,
}

pub struct Exec {
    state: crate::Publisher<ExecState>,
    config: Config,
    dependencies: Dependencies,
    slots: BTreeMap<u64, Slot>,
    offers: BTreeMap<String, StoredOffer>,
    taken: BTreeMap<String, u64>,
    next_spawn: u64,
    next_generation: u64,
    scope_warned: Arc<AtomicBool>,
}

impl Exec {
    fn desktop_id(&self, name: &str) -> String {
        self.config
            .applets
            .get(name)
            .map_or_else(|| name.to_owned(), |config| config.applet.clone())
    }

    fn attached_slot(&mut self, applet: String, placement: Placement, offer: Offer) -> Slot {
        self.next_spawn += 1;
        let applet_id = self.desktop_id(&applet);
        let state = self
            .config
            .state_base
            .as_ref()
            .map(|base| base.join("applets").join(&applet_id));
        Slot {
            origin: Origin::Attached,
            pipes: Some(Arc::new(tokio::sync::Mutex::new(Some(offer.pipes)))),
            public: SlotState {
                applet: applet.clone(),
                generation: 0,
                tree: Arc::new(Tree::default()),
                status: Status::Starting,
                title: offer.entry.name.clone(),
                icon: offer.entry.icon.clone(),
                requests: Vec::new(),
            },
            placement,
            applet_id,
            sandbox: glimpse_config::AppletSandbox::default(),
            state,
            entry: Some(offer.entry),
            catalog_refused: false,
            options: offer.options,
            spawn: self.next_spawn,
            attempt: 0,
            backoff_until: None,
            link: None,
            stop: CancellationToken::new(),
            popover_open: false,
            last_gesture: None,
            last_notify: None,
            warned_gate: false,
            last_failed_reason: None,
            serial: 0,
            failed_at: None,
            failures: 0,
            catalog_misses: 0,
            resolving: false,
        }
    }

    fn finish_attached(&mut self, id: u64, failed: Option<String>) {
        let Some(slot) = self.slots.get_mut(&id) else {
            return;
        };
        self.taken.remove(&slot.applet_id);
        slot.link = None;
        slot.pipes = None;
        if let Some(config) = self.config.applets.get(&slot.public.applet) {
            tracing::info!(applet = %slot.applet_id, slot = id, "attached applet ended, returning the configured one");
            slot.origin = Origin::Spawned;
            slot.applet_id = config.applet.clone();
            slot.sandbox = config.sandbox.clone();
            slot.options = config.options.clone();
            slot.state = self
                .config
                .state_base
                .as_ref()
                .map(|base| base.join("applets").join(&config.applet));
            slot.entry = None;
            slot.attempt = 0;
            slot.failures = 0;
            slot.last_failed_reason = None;
            self.spawn_slot(id);
        } else if let Some(reason) = failed {
            tracing::info!(applet = %slot.applet_id, slot = id, %reason, "attached applet failed");
            slot.last_failed_reason = Some(reason.clone());
            slot.public.status = Status::Failed(reason);
        } else {
            tracing::info!(applet = %slot.applet_id, slot = id, "attached applet ended");
            slot.public.status = Status::Ended;
        }
    }

    fn publish(&self) {
        self.state.set(ExecState {
            slots: self
                .slots
                .iter()
                .map(|(id, slot)| (*id, slot.public.clone()))
                .collect(),
            offers: self
                .offers
                .keys()
                .chain(self.taken.keys())
                .cloned()
                .collect(),
        });
    }

    fn spawn_slot(&mut self, id: u64) {
        if self
            .slots
            .get(&id)
            .is_some_and(|slot| slot.origin == Origin::Attached)
        {
            return;
        }
        self.next_spawn += 1;
        if let Some(slot) = self.slots.get_mut(&id) {
            tracing::info!(applet = %slot.applet_id, slot = id, attempt = slot.attempt, "applet restarting");
            if let Some(link) = slot.link.take() {
                link.stop.cancel();
            }
            slot.stop.cancel();
            slot.stop = CancellationToken::new();
            slot.spawn = self.next_spawn;
            slot.backoff_until = None;
            slot.public.status = Status::Starting;
            slot.public.tree = Arc::new(Tree::default());
            slot.entry = None;
            slot.catalog_refused = false;
            slot.last_gesture = None;
            slot.failed_at = None;
        }
    }

    fn send(slot: &mut Slot, message: Outgoing) {
        if let Some(link) = &slot.link
            && let Err(mpsc::error::TrySendError::Full(_)) = link.tx.try_send(message)
            && let Some(link) = slot.link.take()
        {
            link.queue_full
                .store(true, std::sync::atomic::Ordering::Relaxed);
            link.stop.cancel();
            tracing::warn!(applet = %slot.public.applet, "not reading stdin");
        }
    }

    fn request(&mut self, slot_id: u64, request: FromApplet, ctx: &Ctx<Self>) {
        let Some(slot) = self.slots.get_mut(&slot_id) else {
            return;
        };
        if let FromApplet::Notify { .. } = &request {
            if slot
                .last_notify
                .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
            {
                tracing::warn!(applet = %slot.public.applet, "notification rate exceeded");
                return;
            }
            slot.last_notify = Some(Instant::now());
            if let Some(entry) = slot
                .entry
                .as_ref()
                .and_then(|entry| note(entry, &slot.applet_id, &request))
            {
                let provider = self.dependencies.notifications.clone();
                ctx.spawn_detached(move |_ctx| async move {
                    if let Err(error) = provider
                        .post(
                            &entry.name,
                            &entry.id,
                            &entry.icon,
                            &entry.summary,
                            &entry.body,
                            entry.urgency,
                        )
                        .await
                    {
                        tracing::warn!(%error, "applet notification failed");
                    }
                });
            }
            return;
        }
        if let FromApplet::OpenUri { uri } = &request
            && !allowed_uri(uri)
        {
            tracing::warn!(applet = %slot.public.applet, "applet asked to open a refused uri");
            return;
        }
        if slot.last_gesture.is_none_or(|at| at.elapsed() > GESTURE) {
            if !slot.warned_gate {
                tracing::warn!(applet = %slot.public.applet, "applet request without recent gesture");
                slot.warned_gate = true;
            }
            return;
        }
        if !matches!(request, FromApplet::ClosePopover) {
            slot.last_gesture = None;
        }
        match request {
            FromApplet::Copy { text } => {
                if text.len() <= 1 << 20 {
                    let offer = SelectionOffer {
                        mime: TEXT.to_owned(),
                        data: Arc::from(text.into_bytes()),
                    };
                    if let Err(error) = self.dependencies.selection.offer(offer) {
                        tracing::warn!(%error, "applet copy failed");
                    }
                }
            }
            FromApplet::OpenUri { uri } => slot.push_request(BarRequest::OpenUri(uri)),
            FromApplet::Session { action } => slot.push_request(BarRequest::Session(action)),
            FromApplet::ClosePopover => slot.push_request(BarRequest::ClosePopover),
            _ => {}
        }
    }
}

impl Slot {
    fn push_request(&mut self, request: BarRequest) {
        self.serial += 1;
        self.public.requests.push((self.serial, request));
        if self.public.requests.len() > 8 {
            self.public.requests.remove(0);
        }
    }
}

fn allowed_uri(uri: &str) -> bool {
    uri.len() <= URI_CAP
        && uri.split_once(':').is_some_and(|(scheme, _)| {
            ["http", "https", "mailto"]
                .iter()
                .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
        })
}

struct Note {
    name: String,
    id: String,
    icon: String,
    summary: String,
    body: String,
    urgency: NotificationUrgency,
}

fn note(entry: &Entry, id: &str, request: &FromApplet) -> Option<Note> {
    let FromApplet::Notify {
        summary,
        body,
        icon,
        urgency,
    } = request
    else {
        return None;
    };
    Some(Note {
        name: entry.name.clone(),
        id: id.to_owned(),
        icon: tree::icon(icon.clone())
            .or_else(|| entry.icon.clone())
            .unwrap_or_default(),
        summary: glimpse_utils::clean(summary, 256),
        body: glimpse_utils::clean(body, 4096),
        urgency: match urgency {
            Urgency::Low => NotificationUrgency::Low,
            Urgency::Normal => NotificationUrgency::Normal,
            Urgency::Critical => NotificationUrgency::Critical,
        },
    })
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_secs(1_u64 << attempt.min(6)).min(Duration::from_secs(60))
}

impl Service for Exec {
    const NAME: &'static str = "exec";
    type Config = Config;
    type State = ExecState;
    type Handle = ExecHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = SubKey;

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
        ExecHandle(endpoint)
    }
    fn initial_state(_: &Self::Config) -> Self::State {
        ExecState::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let mut subs = Vec::new();
        for (&slot_id, slot) in &self.slots {
            let spawn_id = slot.spawn;
            match slot.public.status {
                Status::Starting | Status::Running => {
                    let catalog = Arc::clone(&self.dependencies.catalog);
                    let args = spawn::SourceArgs {
                        slot: slot_id,
                        spawn: spawn_id,
                        id: slot.applet_id.clone(),
                        sandbox: slot.sandbox.clone(),
                        state: slot.state.clone(),
                        state_base: self.config.state_base.clone(),
                        origin: slot.origin,
                        pipes: slot.pipes.clone(),
                        entry: slot.entry.clone(),
                        stop: slot.stop.clone(),
                        scope_warned: Arc::clone(&self.scope_warned),
                    };
                    subs.push(Sub::stream(
                        SubKey::Child {
                            slot: slot_id,
                            spawn: spawn_id,
                        },
                        move |ctx| async move { spawn::source(ctx, catalog, args).await },
                    ));
                }
                Status::Restarting { .. } => {
                    if let Some(at) = slot.backoff_until {
                        subs.push(Sub::deadline(
                            SubKey::Backoff {
                                slot: slot_id,
                                spawn: spawn_id,
                            },
                            at,
                            Event::Backoff { slot: slot_id },
                        ));
                    }
                }
                Status::Failed(_) | Status::Ended => {}
            }
        }
        for (applet, stored) in &self.offers {
            let offered_at = stored.offered_at;
            let remaining = OFFER_TTL.saturating_sub(offered_at.elapsed());
            let at = Utc::now() + chrono::Duration::from_std(remaining).unwrap_or_default();
            subs.push(Sub::deadline(
                SubKey::Offer {
                    applet: applet.clone(),
                    offered_at,
                },
                at,
                Event::OfferExpired {
                    applet: applet.clone(),
                    offered_at,
                },
            ));
        }
        if self.slots.values().any(|slot| {
            slot.origin == Origin::Spawned
                && matches!(slot.public.status, Status::Failed(_) | Status::Running)
        }) {
            subs.push(Sub::interval(SubKey::Retry, RETRY, |_ctx| async {
                Event::Retry
            }));
        }
        subs
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            state: ctx.publisher(),
            config,
            dependencies,
            slots: BTreeMap::new(),
            offers: BTreeMap::new(),
            taken: BTreeMap::new(),
            next_spawn: 0,
            next_generation: 0,
            scope_warned: Arc::new(AtomicBool::new(false)),
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(Command::Attach {
                slot,
                applet,
                placement,
                reply,
            }) => {
                if self.slots.contains_key(&slot) {
                    let _ = reply.send(Err(CommandError::InvalidArgument(
                        "slot already attached".to_owned(),
                    )));
                    return;
                }
                let desktop_id = self.desktop_id(&applet);
                if self.offers.get(&desktop_id).is_some_and(|stored| {
                    stored.offer.output.is_none() || stored.offer.output == placement.output
                }) && let Some(stored) = self.offers.remove(&desktop_id)
                {
                    tracing::info!(applet = %desktop_id, slot, output = ?placement.output, "applet attached from its offer");
                    let attached = self.attached_slot(applet, placement, stored.offer);
                    self.slots.insert(slot, attached);
                    self.taken.insert(desktop_id, slot);
                    let _ = reply.send(Ok(()));
                    self.publish();
                    return;
                }
                let config = self.config.applets.get(&applet);
                let (applet_id, sandbox, options, status) = match config {
                    Some(config) => (
                        config.applet.clone(),
                        config.sandbox.clone(),
                        config.options.clone(),
                        Status::Starting,
                    ),
                    None => (
                        String::new(),
                        glimpse_config::AppletSandbox::default(),
                        Map::new(),
                        Status::Failed("not configured".to_owned()),
                    ),
                };
                let last_failed_reason = match &status {
                    Status::Failed(reason) => {
                        tracing::warn!(%applet, %reason, "applet failed");
                        Some(reason.clone())
                    }
                    _ => None,
                };
                let state = self
                    .config
                    .state_base
                    .as_ref()
                    .map(|base| base.join("applets").join(&applet_id));
                self.next_spawn += 1;
                self.slots.insert(
                    slot,
                    Slot {
                        origin: Origin::Spawned,
                        pipes: None,
                        public: SlotState {
                            applet,
                            generation: 0,
                            tree: Arc::new(Tree::default()),
                            status,
                            title: String::new(),
                            icon: None,
                            requests: Vec::new(),
                        },
                        placement,
                        applet_id,
                        sandbox,
                        state,
                        entry: None,
                        catalog_refused: false,
                        options,
                        spawn: self.next_spawn,
                        attempt: 0,
                        backoff_until: None,
                        link: None,
                        stop: CancellationToken::new(),
                        popover_open: false,
                        last_gesture: None,
                        last_notify: None,
                        warned_gate: false,
                        last_failed_reason,
                        serial: 0,
                        failed_at: None,
                        failures: 0,
                        catalog_misses: 0,
                        resolving: false,
                    },
                );
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::Offer {
                applet,
                offer,
                reply,
            }) => {
                let id = self.desktop_id(&applet);
                let result = if self.offers.contains_key(&id) || self.taken.contains_key(&id) {
                    tracing::info!(applet = %id, "offer refused, already attached");
                    Err(CommandError::InvalidArgument(format!(
                        "{id} is already attached"
                    )))
                } else if let Some((&slot_id, _)) = self.slots.iter().find(|(_, slot)| {
                    self.desktop_id(&slot.public.applet) == id
                        && (slot.origin == Origin::Spawned
                            || (slot.origin == Origin::Attached
                                && matches!(slot.public.status, Status::Ended | Status::Failed(_))))
                        && (offer.output.is_none() || offer.output == slot.placement.output)
                        && offer
                            .position
                            .is_none_or(|edge| edge == slot.placement.position)
                }) {
                    self.next_spawn += 1;
                    if let Some(slot) = self.slots.get_mut(&slot_id) {
                        tracing::info!(applet = %id, slot = slot_id, output = ?slot.placement.output, from = ?slot.origin, "applet taken over by its offer");
                        if let Some(link) = slot.link.take() {
                            link.stop.cancel();
                        }
                        slot.stop.cancel();
                        slot.stop = CancellationToken::new();
                        slot.spawn = self.next_spawn;
                        slot.origin = Origin::Attached;
                        slot.applet_id = id.clone();
                        slot.state = self
                            .config
                            .state_base
                            .as_ref()
                            .map(|base| base.join("applets").join(&id));
                        slot.pipes = Some(Arc::new(tokio::sync::Mutex::new(Some(offer.pipes))));
                        slot.entry = Some(offer.entry.clone());
                        slot.options = offer.options;
                        slot.public.title = offer.entry.name;
                        slot.public.icon = offer.entry.icon;
                        slot.public.status = Status::Starting;
                        slot.attempt = 0;
                        slot.backoff_until = None;
                        slot.failures = 0;
                        slot.last_failed_reason = None;
                        slot.failed_at = None;
                        slot.catalog_refused = false;
                        slot.resolving = false;
                    }
                    self.taken.insert(id, slot_id);
                    Ok(())
                } else {
                    tracing::info!(applet = %id, output = ?offer.output, "applet offered, waiting for its widget");
                    self.offers.insert(
                        id,
                        StoredOffer {
                            offer,
                            offered_at: Instant::now(),
                        },
                    );
                    Ok(())
                };
                let _ = reply.send(result);
            }
            Input::Command(Command::OfferOptions {
                applet,
                options,
                reply,
            }) => {
                let id = self.desktop_id(&applet);
                let result = if let Some(stored) = self.offers.get_mut(&id) {
                    stored.offer.options = options;
                    Ok(())
                } else if let Some(slot) = self
                    .taken
                    .get(&id)
                    .and_then(|slot| self.slots.get_mut(slot))
                {
                    slot.options = options.clone();
                    if matches!(slot.public.status, Status::Running) {
                        Self::send(slot, Outgoing::Options { options });
                    }
                    Ok(())
                } else {
                    Err(CommandError::InvalidArgument("unknown applet".to_owned()))
                };
                let _ = reply.send(result);
            }
            Input::Command(Command::SetOptions {
                slot,
                options,
                reply,
            }) => {
                let result = if let Some(slot) = self.slots.get_mut(&slot) {
                    slot.options = options.clone();
                    if matches!(slot.public.status, Status::Running) {
                        Self::send(slot, Outgoing::Options { options });
                    }
                    Ok(())
                } else {
                    Err(CommandError::InvalidArgument("unknown slot".to_owned()))
                };
                let _ = reply.send(result);
            }
            Input::Command(Command::Place {
                slot,
                placement,
                reply,
            }) => {
                let result = if let Some(slot) = self.slots.get_mut(&slot) {
                    if slot.placement != placement {
                        slot.placement = placement.clone();
                        if matches!(slot.public.status, Status::Running) {
                            Self::send(slot, Outgoing::Placement { placement });
                        }
                    }
                    Ok(())
                } else {
                    Err(CommandError::InvalidArgument("unknown slot".to_owned()))
                };
                let _ = reply.send(result);
            }
            Input::Command(Command::Detach { slot, reply }) => {
                if let Some(state) = self.slots.remove(&slot)
                    && state.origin == Origin::Attached
                {
                    self.taken.remove(&state.applet_id);
                }
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::SendEvent { event, reply }) => {
                if let Some(slot) = self.slots.get_mut(&event.slot)
                    && slot.public.generation == event.generation
                    && matches!(slot.public.status, Status::Running)
                {
                    if event.gesture {
                        slot.last_gesture = Some(Instant::now());
                        slot.warned_gate = false;
                    }
                    Self::send(
                        slot,
                        Outgoing::Event {
                            id: event.id,
                            name: event.name,
                            args: event.args,
                            seq: event.seq,
                        },
                    );
                }
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::Popover { slot, open, reply }) => {
                let result = if let Some(slot) = self.slots.get_mut(&slot) {
                    slot.popover_open = open;
                    if matches!(slot.public.status, Status::Running) {
                        Self::send(slot, Outgoing::Popover { open });
                    }
                    Ok(())
                } else {
                    Err(CommandError::InvalidArgument("unknown slot".to_owned()))
                };
                let _ = reply.send(result);
            }
            Input::Event(Event::Spawned {
                slot,
                spawn,
                link,
                entry,
            }) => {
                if let Some(state) = self.slots.get_mut(&slot).filter(|s| s.spawn == spawn) {
                    state.public.title = entry.name.clone();
                    state.public.icon = entry.icon.clone();
                    state.entry = Some(entry);
                    state.link = Some(link);
                    if state.origin == Origin::Attached {
                        state.pipes = None;
                    }
                } else {
                    link.stop.cancel();
                }
            }
            Input::Event(Event::Hello {
                slot,
                spawn,
                state: state_dir,
            }) => {
                if let Some(state) = self.slots.get_mut(&slot).filter(|s| s.spawn == spawn) {
                    self.next_generation += 1;
                    state.public.generation = self.next_generation;
                    if state.origin == Origin::Spawned {
                        state.public.tree = Arc::new(Tree::default());
                    }
                    state.public.status = Status::Running;
                    state.last_failed_reason = None;
                    state.last_gesture = None;
                    state.failures = 0;
                    Self::send(
                        state,
                        Outgoing::Hello {
                            v: 1,
                            name: state.public.applet.clone(),
                            state: state_dir,
                            options: state.options.clone(),
                            placement: state.placement.clone(),
                        },
                    );
                    if state.popover_open {
                        Self::send(state, Outgoing::Popover { open: true });
                    }
                }
            }
            Input::Event(Event::Tree { slot, spawn, tree }) => {
                if let Some(state) = self
                    .slots
                    .get_mut(&slot)
                    .filter(|s| s.spawn == spawn && matches!(s.public.status, Status::Running))
                {
                    state.public.tree = tree;
                }
            }
            Input::Event(Event::Request {
                slot,
                spawn,
                request,
            }) => {
                if self
                    .slots
                    .get(&slot)
                    .is_some_and(|s| s.spawn == spawn && matches!(s.public.status, Status::Running))
                {
                    self.request(slot, request, ctx);
                }
            }
            Input::Event(Event::Exited {
                slot,
                spawn,
                spoke,
                ran,
                reason,
                resolved,
            }) => {
                if let Some(state) = self.slots.get_mut(&slot).filter(|s| s.spawn == spawn) {
                    state.link = None;
                    if let Some(entry) = resolved {
                        state.entry = Some(entry);
                    }
                    state.public.tree = Arc::new(Tree::default());
                    if state.origin == Origin::Attached {
                        self.finish_attached(slot, Some(reason));
                    } else if !self.config.applets.contains_key(&state.public.applet) {
                        state.public.status = Status::Failed("not configured".to_owned());
                    } else if spoke && !matches!(state.public.status, Status::Failed(_)) {
                        tracing::warn!(applet = %state.public.applet, %reason, "applet exited");
                        state.attempt = if ran >= Duration::from_secs(60) {
                            0
                        } else {
                            state.attempt.saturating_add(1)
                        };
                        state.public.status = Status::Restarting {
                            attempt: state.attempt,
                        };
                        state.backoff_until = Some(
                            Utc::now()
                                + chrono::Duration::from_std(backoff(state.attempt))
                                    .unwrap_or_default(),
                        );
                    } else {
                        if state.last_failed_reason.as_deref() != Some(reason.as_str()) {
                            tracing::warn!(applet = %state.public.applet, %reason, "applet failed");
                        }
                        state.last_failed_reason = Some(reason.clone());
                        state.failures = state.failures.saturating_add(1);
                        state.failed_at = Some(Instant::now());
                        state.public.status = Status::Failed(reason);
                    }
                }
            }
            Input::Event(Event::Ended { slot, spawn }) => {
                if self
                    .slots
                    .get(&slot)
                    .is_some_and(|s| s.spawn == spawn && s.origin == Origin::Attached)
                {
                    self.finish_attached(slot, None);
                }
            }
            Input::Event(Event::OfferExpired { applet, offered_at }) => {
                if self
                    .offers
                    .get(&applet)
                    .is_some_and(|stored| stored.offered_at == offered_at)
                {
                    tracing::info!(%applet, "offer expired, no widget took it");
                    self.offers.remove(&applet);
                }
            }
            Input::Event(Event::Backoff { slot }) => {
                if self.slots.get(&slot).is_some_and(|s| {
                    s.origin == Origin::Spawned
                        && matches!(s.public.status, Status::Restarting { .. })
                        && s.backoff_until.is_some_and(|at| Utc::now() >= at)
                }) {
                    if self
                        .slots
                        .get(&slot)
                        .is_some_and(|s| self.config.applets.contains_key(&s.public.applet))
                    {
                        self.spawn_slot(slot);
                    } else if let Some(state) = self.slots.get_mut(&slot) {
                        state.public.status = Status::Failed("not configured".to_owned());
                    }
                }
            }
            Input::Event(Event::Retry) => {
                for (&slot, state) in &mut self.slots {
                    if state.origin == Origin::Spawned
                        && matches!(state.public.status, Status::Failed(_) | Status::Running)
                        && !state.resolving
                        && self.config.applets.contains_key(&state.public.applet)
                    {
                        state.resolving = true;
                        let catalog = Arc::clone(&self.dependencies.catalog);
                        let id = state.applet_id.clone();
                        let spawn = state.spawn;
                        ctx.spawn_detached(move |ctx| async move {
                            let resolved =
                                tokio::task::spawn_blocking(move || catalog.resolve(&id))
                                    .await
                                    .ok();
                            let _ = ctx
                                .events()
                                .send(Input::Event(Event::RetryReady {
                                    slot,
                                    spawn,
                                    resolved,
                                }))
                                .await;
                        });
                    }
                }
            }
            Input::Event(Event::RetryReady {
                slot,
                spawn,
                resolved,
            }) => {
                if let Some(state) = self
                    .slots
                    .get_mut(&slot)
                    .filter(|s| s.spawn == spawn && s.origin == Origin::Spawned)
                {
                    state.resolving = false;
                    state.catalog_misses = match &resolved {
                        Some(Err(_)) => state.catalog_misses.saturating_add(1),
                        _ => 0,
                    };
                }
                if let Some(Err(reason)) = &resolved
                    && self.slots.get(&slot).is_some_and(|s| s.catalog_misses >= 2)
                    && let Some(state) = self
                        .slots
                        .get_mut(&slot)
                        .filter(|s| s.spawn == spawn && matches!(s.public.status, Status::Running))
                {
                    tracing::warn!(applet = %state.public.applet, %reason, "applet entry vanished");
                    if let Some(link) = state.link.take() {
                        link.stop.cancel();
                    }
                    state.stop.cancel();
                    self.next_spawn += 1;
                    state.spawn = self.next_spawn;
                    state.public.tree = Arc::new(Tree::default());
                    state.last_failed_reason = Some(reason.clone());
                    state.catalog_refused = true;
                    state.failed_at = Some(Instant::now());
                    state.public.status = Status::Failed(reason.clone());
                } else if let Some(state) = self.slots.get_mut(&slot)
                    && state.spawn == spawn
                    && matches!(state.public.status, Status::Failed(_))
                    && self.config.applets.contains_key(&state.public.applet)
                {
                    let due = state
                        .failed_at
                        .is_some_and(|at| at.elapsed() >= backoff(state.failures));
                    match resolved {
                        Some(Ok(entry))
                            if due
                                || state.catalog_refused
                                || state.entry.as_ref() != Some(&entry) =>
                        {
                            self.spawn_slot(slot)
                        }
                        Some(Err(_)) => state.catalog_refused = true,
                        _ => {}
                    }
                }
            }
            Input::Config(config) => {
                let mut respawn = Vec::new();
                for (&id, slot) in &mut self.slots {
                    if slot.origin == Origin::Attached {
                        continue;
                    }
                    match config.applets.get(&slot.public.applet) {
                        Some(applet) if applet.applet != slot.applet_id => {
                            slot.applet_id = applet.applet.clone();
                            slot.sandbox = applet.sandbox.clone();
                            slot.state = config
                                .state_base
                                .as_ref()
                                .map(|base| base.join("applets").join(&applet.applet));
                            slot.options = applet.options.clone();
                            respawn.push(id);
                        }
                        Some(applet) if applet.sandbox != slot.sandbox => {
                            slot.sandbox = applet.sandbox.clone();
                            slot.options = applet.options.clone();
                            respawn.push(id);
                        }
                        Some(applet) if applet.options != slot.options => {
                            slot.options = applet.options.clone();
                            if matches!(slot.public.status, Status::Failed(_)) {
                                respawn.push(id);
                            } else if matches!(slot.public.status, Status::Running) {
                                Self::send(
                                    slot,
                                    Outgoing::Options {
                                        options: slot.options.clone(),
                                    },
                                );
                            }
                        }
                        Some(_) if matches!(&slot.public.status, Status::Failed(reason) if reason == "not configured") =>
                        {
                            respawn.push(id);
                        }
                        None => {
                            if slot.last_failed_reason.as_deref() != Some("not configured") {
                                tracing::warn!(applet = %slot.public.applet, "applet not configured");
                                slot.last_failed_reason = Some("not configured".to_owned());
                            }
                            slot.public.status = Status::Failed("not configured".to_owned());
                            if let Some(link) = slot.link.take() {
                                link.stop.cancel();
                            }
                        }
                        _ => {}
                    }
                }
                self.config = config;
                for id in respawn {
                    self.spawn_slot(id);
                }
            }
        }
        self.publish();
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use glimpse_dbus::{Buses, notifications::NotificationsProvider};
    use rustix::pipe::{PipeFlags, pipe_with};
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::unix::pipe;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::selection::FakeSelection;
    use crate::service::{ServiceRuntime, ServiceSender};

    #[derive(Clone)]
    struct FakeCatalog(Arc<Mutex<Result<Entry, String>>>);

    impl Catalog for FakeCatalog {
        fn resolve(&self, _id: &str) -> Result<Entry, String> {
            self.0.lock().map_err(|error| error.to_string())?.clone()
        }
    }

    struct PairCatalog {
        bad: Entry,
        good: Entry,
    }

    impl Catalog for PairCatalog {
        fn resolve(&self, id: &str) -> Result<Entry, String> {
            match id {
                "me.example.Test" => Ok(self.bad.clone()),
                "me.example.Other" => Ok(self.good.clone()),
                _ => Err("unknown applet".to_owned()),
            }
        }
    }

    fn placement() -> Placement {
        Placement {
            output: Some("DP-1".to_owned()),
            position: Edge::Top,
            orientation: Orientation::Horizontal,
            zone: Zone::Left,
            size: 32,
        }
    }

    fn script(source: &str) -> (tempfile::TempDir, Entry, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("applet.sh");
        std::fs::write(&path, source).expect("script");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("executable");
        let output = dir.path().join("messages");
        let entry = Entry {
            argv: vec![path.display().to_string(), output.display().to_string()],
            cwd: None,
            name: "Test applet".to_owned(),
            icon: None,
            path: path.clone(),
            exec: path.display().to_string(),
        };
        (dir, entry, output)
    }

    struct TestConfig {
        config: Config,
        state_dir: tempfile::TempDir,
    }

    impl std::ops::Deref for TestConfig {
        type Target = Config;

        fn deref(&self) -> &Config {
            &self.config
        }
    }

    impl std::ops::DerefMut for TestConfig {
        fn deref_mut(&mut self) -> &mut Config {
            &mut self.config
        }
    }

    fn configured() -> TestConfig {
        let state_dir = tempfile::tempdir().expect("state dir");
        let config = Config {
            state_base: Some(state_dir.path().to_path_buf()),
            applets: BTreeMap::from([(
                "test".to_owned(),
                glimpse_config::ExecAppletConfig {
                    applet: "me.example.Test".to_owned(),
                    options: Map::from_iter([("color".to_owned(), Value::from("blue"))]),
                    sandbox: glimpse_config::AppletSandbox {
                        read: vec![std::env::temp_dir().display().to_string()],
                        write: vec![std::env::temp_dir().display().to_string()],
                        ..Default::default()
                    },
                },
            )]),
        };
        TestConfig { config, state_dir }
    }

    fn confined(entry: &Entry) -> TestConfig {
        let mut config = configured();
        let path = entry
            .path
            .parent()
            .expect("script parent")
            .display()
            .to_string();
        let sandbox = &mut config.applets.get_mut("test").expect("test applet").sandbox;
        sandbox.read = vec![path.clone()];
        sandbox.write = vec![path];
        config
    }

    fn landlock_available() -> bool {
        if sandbox::abi() < 10 {
            eprintln!("skipped: Landlock ABI < 10");
            false
        } else {
            true
        }
    }

    fn running(
        catalog: FakeCatalog,
    ) -> (
        ExecHandle,
        ServiceSender<Exec>,
        CancellationToken,
        tokio::task::JoinHandle<()>,
        FakeSelection,
    ) {
        running_with(configured(), Arc::new(catalog))
    }

    fn running_with(
        config: TestConfig,
        catalog: Arc<dyn Catalog>,
    ) -> (
        ExecHandle,
        ServiceSender<Exec>,
        CancellationToken,
        tokio::task::JoinHandle<()>,
        FakeSelection,
    ) {
        let TestConfig { config, state_dir } = config;
        let cancel = CancellationToken::new();
        let (mut runtime, handle) = ServiceRuntime::<Exec>::new(
            config,
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        let sender = runtime.sender();
        let selection = FakeSelection::default();
        let deps = Dependencies {
            catalog,
            selection: Arc::new(selection.clone()),
            notifications: NotificationsProvider::unavailable("no bus in tests").handle(),
        };
        let task = tokio::spawn(async move {
            let _state_dir = state_dir;
            let _ = runtime.run(deps).await;
        });
        (handle, sender, cancel, task, selection)
    }

    async fn status(
        handle: &ExecHandle,
        slot: u64,
        predicate: impl Fn(&Status) -> bool,
    ) -> SlotState {
        let mut receiver = handle.subscribe();
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let Some(state) = receiver
                    .borrow()
                    .slots
                    .get(&slot)
                    .filter(|state| predicate(&state.status))
                {
                    return state.clone();
                }
                receiver.changed().await.expect("service state");
            }
        })
        .await
        .expect("status changed")
    }

    async fn file_contains(path: &PathBuf, needle: &str) -> String {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Ok(content) = tokio::fs::read_to_string(path).await
                    && content.contains(needle)
                {
                    return content;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("message written")
    }

    async fn next_generation(handle: &ExecHandle, slot: u64, generation: u64) -> SlotState {
        tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                if let Some(state) =
                    handle.snapshot().slots.get(&slot).filter(|s| {
                        matches!(s.status, Status::Running) && s.generation > generation
                    })
                {
                    return state.clone();
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("next generation")
    }

    fn attached_entry() -> Entry {
        Entry {
            argv: Vec::new(),
            cwd: None,
            name: "Attached applet".to_owned(),
            icon: Some("test-symbolic".to_owned()),
            path: PathBuf::new(),
            exec: String::new(),
        }
    }

    fn attached_peer() -> (Pipes, pipe::Sender, BufReader<pipe::Receiver>) {
        let (from_applet, peer_writer) = pipe_with(PipeFlags::CLOEXEC).expect("applet pipe");
        let (peer_reader, to_applet) = pipe_with(PipeFlags::CLOEXEC).expect("host pipe");
        (
            Pipes {
                to_applet,
                from_applet,
            },
            pipe::Sender::from_owned_fd(peer_writer).expect("peer writer"),
            BufReader::new(pipe::Receiver::from_owned_fd(peer_reader).expect("peer reader")),
        )
    }

    async fn offer_and_attach(
        handle: &ExecHandle,
        slot: u64,
        applet: &str,
        options: Map<String, Value>,
        pipes: Pipes,
    ) {
        handle
            .offer(
                applet,
                Offer {
                    output: None,
                    position: None,
                    entry: attached_entry(),
                    options,
                    pipes,
                },
            )
            .await
            .expect("offer");
        handle
            .attach(slot, applet, placement())
            .await
            .expect("attach offer");
    }

    #[tokio::test]
    async fn untaken_offer_expires_and_closes_its_pipes() {
        let (handle, _sender, cancel, task, _) = running_with(
            configured(),
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
                "not installed".to_owned()
            ))))),
        );
        let (pipes, _peer_writer, mut peer_reader) = attached_peer();
        handle
            .offer(
                "me.x.A",
                Offer {
                    output: None,
                    position: None,
                    entry: attached_entry(),
                    options: Map::new(),
                    pipes,
                },
            )
            .await
            .expect("offer");
        assert!(handle.snapshot().offers.contains("me.x.A"));
        tokio::time::timeout(
            Duration::from_secs(7),
            peer_reader.read_to_end(&mut Vec::new()),
        )
        .await
        .expect("host pipe EOF")
        .expect("host pipe read");
        assert!(handle.snapshot().offers.is_empty());
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn reoffer_revives_an_ended_attached_slot() {
        let (handle, _sender, cancel, task, _) = running_with(
            configured(),
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
                "not installed".to_owned()
            ))))),
        );
        let (pipes, mut peer_writer, mut peer_reader) = attached_peer();
        offer_and_attach(&handle, 1, "me.x.A", Map::new(), pipes).await;
        peer_writer
            .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
            .await
            .expect("hello");
        let mut line = String::new();
        peer_reader.read_line(&mut line).await.expect("host hello");
        let first = status(&handle, 1, |s| matches!(s, Status::Running)).await;
        drop(peer_writer);
        status(&handle, 1, |s| matches!(s, Status::Ended)).await;
        assert!(handle.snapshot().offers.is_empty());
        let (pipes, mut peer_writer, mut peer_reader) = attached_peer();
        handle
            .offer(
                "me.x.A",
                Offer {
                    output: None,
                    position: None,
                    entry: attached_entry(),
                    options: Map::new(),
                    pipes,
                },
            )
            .await
            .expect("reoffer");
        assert_eq!(handle.snapshot().slots.len(), 1);
        assert_eq!(
            handle.snapshot().slots.get(&1).expect("slot").status,
            Status::Starting
        );
        assert!(handle.snapshot().offers.contains("me.x.A"));
        peer_writer
            .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
            .await
            .expect("second hello");
        let mut line = String::new();
        peer_reader
            .read_line(&mut line)
            .await
            .expect("second host hello");
        next_generation(&handle, 1, first.generation).await;
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn offer_revives_a_not_configured_slot_left_by_a_rebuild() {
        let config = configured();
        let base = config.state_base.clone().expect("state base");
        let (handle, _sender, cancel, task, _) = running_with(
            config,
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
                "not installed".to_owned()
            ))))),
        );
        let (pipes, _peer_writer, _peer_reader) = attached_peer();
        offer_and_attach(&handle, 1, "me.x.A", Map::new(), pipes).await;
        handle
            .attach(2, "me.x.A", placement())
            .await
            .expect("replacement attach");
        assert_eq!(
            handle.snapshot().slots.get(&2).expect("replacement").status,
            Status::Failed("not configured".to_owned())
        );
        handle.detach(1).await.expect("old detach");
        let (pipes, mut peer_writer, mut peer_reader) = attached_peer();
        handle
            .offer(
                "me.x.A",
                Offer {
                    output: None,
                    position: None,
                    entry: attached_entry(),
                    options: Map::new(),
                    pipes,
                },
            )
            .await
            .expect("reoffer");
        assert_eq!(handle.snapshot().slots.len(), 1);
        assert_eq!(
            handle.snapshot().slots.get(&2).expect("replacement").status,
            Status::Starting
        );
        peer_writer
            .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
            .await
            .expect("hello");
        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(2), peer_reader.read_line(&mut line))
            .await
            .expect("host hello")
            .expect("host pipe read");
        let hello: Value = serde_json::from_str(&line).expect("json");
        assert_eq!(hello["t"], "hello");
        assert_eq!(
            hello["state"],
            base.join("applets/me.x.A").display().to_string()
        );
        status(&handle, 2, |s| matches!(s, Status::Running)).await;
        handle.detach(2).await.expect("replacement detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn attach_takes_offer_for_its_output() {
        let (_dir, entry, _output) = script(
            "#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do :; done\n",
        );
        let (handle, _sender, cancel, task, _) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        let (pipes, mut peer_writer, mut peer_reader) = attached_peer();
        handle
            .offer(
                "me.example.Test",
                Offer {
                    output: Some("DP-1".to_owned()),
                    position: None,
                    entry: attached_entry(),
                    options: Map::new(),
                    pipes,
                },
            )
            .await
            .expect("offer");
        let mut other = placement();
        other.output = Some("HDMI-1".to_owned());
        handle.attach(1, "test", other).await.expect("other attach");
        assert_eq!(
            status(&handle, 1, |s| matches!(s, Status::Running))
                .await
                .title,
            "Test applet"
        );
        assert!(handle.snapshot().offers.contains("me.example.Test"));
        handle
            .attach(2, "test", placement())
            .await
            .expect("matching attach");
        assert_eq!(handle.snapshot().slots.len(), 2);
        assert_eq!(
            handle.snapshot().slots.get(&2).expect("slot").status,
            Status::Starting
        );
        peer_writer
            .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
            .await
            .expect("hello");
        let mut line = String::new();
        peer_reader.read_line(&mut line).await.expect("host hello");
        assert_eq!(
            serde_json::from_str::<Value>(&line).expect("json")["t"],
            "hello"
        );
        assert_eq!(
            status(&handle, 2, |s| matches!(s, Status::Running))
                .await
                .title,
            "Attached applet"
        );
        handle.detach(1).await.expect("detach other");
        handle.detach(2).await.expect("detach match");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn second_offer_for_the_same_id_is_refused() {
        let (handle, _sender, cancel, task, _) = running_with(
            configured(),
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
                "not installed".to_owned()
            ))))),
        );
        for attached in [false, true] {
            let (pipes, _writer, _reader) = attached_peer();
            let result = handle
                .offer(
                    "me.x.A",
                    Offer {
                        output: None,
                        position: None,
                        entry: attached_entry(),
                        options: Map::new(),
                        pipes,
                    },
                )
                .await;
            if attached {
                assert!(
                    matches!(result, Err(CommandError::InvalidArgument(reason)) if reason == "me.x.A is already attached")
                );
            } else {
                result.expect("first offer");
            }
        }
        handle
            .attach(1, "me.x.A", placement())
            .await
            .expect("attach");
        let (pipes, _writer, _reader) = attached_peer();
        assert!(matches!(handle.offer("me.x.A", Offer {
            output: None, position: None, entry: attached_entry(), options: Map::new(), pipes,
        }).await, Err(CommandError::InvalidArgument(reason)) if reason == "me.x.A is already attached"));
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn offer_takes_over_the_slot_on_its_panel_edge() {
        let (_dir, entry, _pid_file) = script(
            "#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do :; done\n",
        );
        let (handle, _sender, cancel, task, _) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle
            .attach(1, "test", placement())
            .await
            .expect("attach top");
        let bottom = Placement {
            position: Edge::Bottom,
            ..placement()
        };
        handle
            .attach(2, "test", bottom)
            .await
            .expect("attach bottom");
        status(&handle, 1, |s| matches!(s, Status::Running)).await;
        status(&handle, 2, |s| matches!(s, Status::Running)).await;
        let (pipes, _peer_writer, _peer_reader) = attached_peer();
        handle
            .offer(
                "me.example.Test",
                Offer {
                    output: Some("DP-1".to_owned()),
                    position: Some(Edge::Bottom),
                    entry: attached_entry(),
                    options: Map::new(),
                    pipes,
                },
            )
            .await
            .expect("take over");
        let snapshot = handle.snapshot();
        assert_ne!(
            snapshot.slots.get(&1).expect("top").title,
            "Attached applet"
        );
        assert_eq!(
            snapshot.slots.get(&2).expect("bottom").title,
            "Attached applet"
        );
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn offer_takes_over_spawned_slot_and_returns_it() {
        let (_dir, entry, pid_file) = script(
            "#!/bin/sh\necho $$ > \"$1\"\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nread -r line\nprintf '{\"t\":\"commit\",\"ops\":[{\"op\":\"insert\",\"parent\":0,\"node\":{\"id\":1,\"type\":\"indicator\"}}]}\\n'\nwhile read -r line; do :; done\n",
        );
        let (handle, _sender, cancel, task, _) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle.attach(1, "test", placement()).await.expect("attach");
        let first = status(&handle, 1, |s| matches!(s, Status::Running)).await;
        tokio::time::timeout(Duration::from_secs(2), async {
            while handle
                .snapshot()
                .slots
                .get(&1)
                .expect("slot")
                .tree
                .children(ROOT)
                != [1]
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("first tree");
        let pid = file_contains(&pid_file, "").await;
        let (pipes, mut peer_writer, mut peer_reader) = attached_peer();
        handle
            .offer(
                "me.example.Test",
                Offer {
                    output: Some("DP-1".to_owned()),
                    position: None,
                    entry: attached_entry(),
                    options: Map::new(),
                    pipes,
                },
            )
            .await
            .expect("take over");
        assert!(handle.snapshot().offers.contains("me.example.Test"));
        assert_eq!(handle.snapshot().slots.len(), 1);
        assert_eq!(
            handle.snapshot().slots.get(&1).expect("slot").title,
            "Attached applet"
        );
        assert_eq!(
            handle
                .snapshot()
                .slots
                .get(&1)
                .expect("slot")
                .tree
                .children(ROOT),
            [1]
        );
        tokio::time::timeout(Duration::from_secs(2), async {
            while PathBuf::from(format!("/proc/{}", pid.trim())).exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("child stopped");
        peer_writer
            .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
            .await
            .expect("peer hello");
        let mut line = String::new();
        peer_reader.read_line(&mut line).await.expect("host hello");
        next_generation(&handle, 1, first.generation).await;
        drop(peer_writer);
        status(&handle, 1, |s| matches!(s, Status::Starting)).await;
        assert!(handle.snapshot().offers.is_empty());
        next_generation(&handle, 1, first.generation + 1).await;
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn detach_of_attached_slot_closes_pipes_and_drops_offer() {
        for fail in [false, true] {
            let (handle, _sender, cancel, task, _) = running_with(
                configured(),
                Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
                    "not installed".to_owned()
                ))))),
            );
            let (pipes, mut peer_writer, mut peer_reader) = attached_peer();
            offer_and_attach(&handle, 1, "me.x.A", Map::new(), pipes).await;
            assert!(handle.snapshot().offers.contains("me.x.A"));
            peer_writer
                .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
                .await
                .expect("hello");
            let mut line = String::new();
            peer_reader.read_line(&mut line).await.expect("host hello");
            if fail {
                for _ in 0..EXEC_MAX_HELLOS {
                    peer_writer
                        .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
                        .await
                        .expect("hello");
                }
                status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
            } else {
                handle.detach(1).await.expect("detach");
            }
            tokio::time::timeout(
                Duration::from_secs(1),
                peer_reader.read_to_end(&mut Vec::new()),
            )
            .await
            .expect("host pipe EOF")
            .expect("host pipe read");
            if fail {
                handle.detach(1).await.expect("detach");
            }
            assert!(handle.snapshot().offers.is_empty());
            drop(peer_writer);
            cancel.cancel();
            let _ = task.await;
        }
    }

    #[tokio::test]
    async fn offer_options_reach_running_attached_slot() {
        let config = configured();
        let base = config.state_base.clone().expect("state base");
        let (handle, sender, cancel, task, _) = running_with(
            config,
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
                "not installed".to_owned()
            ))))),
        );
        let (pipes, mut peer_writer, mut peer_reader) = attached_peer();
        let options = Map::from_iter([("color".to_owned(), Value::from("blue"))]);
        offer_and_attach(&handle, 1, "me.x.A", options, pipes).await;
        peer_writer
            .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
            .await
            .expect("hello");
        let mut line = String::new();
        peer_reader.read_line(&mut line).await.expect("host hello");
        let hello: Value = serde_json::from_str(&line).expect("json");
        assert_eq!(
            hello["state"],
            base.join("applets/me.x.A").display().to_string()
        );
        assert_eq!(hello["options"]["color"], "blue");
        let first = status(&handle, 1, |s| matches!(s, Status::Running)).await;
        assert_eq!(first.title, "Attached applet");
        assert_eq!(first.icon.as_deref(), Some("test-symbolic"));
        peer_writer.write_all(b"{\"t\":\"commit\",\"ops\":[{\"op\":\"insert\",\"parent\":0,\"node\":{\"id\":1,\"type\":\"indicator\"}}]}\n").await.expect("commit");
        tokio::time::timeout(Duration::from_secs(2), async {
            while handle
                .snapshot()
                .slots
                .get(&1)
                .is_none_or(|s| s.tree.children(ROOT) != [1])
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("tree");
        peer_writer
            .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
            .await
            .expect("second hello");
        line.clear();
        peer_reader
            .read_line(&mut line)
            .await
            .expect("second host hello");
        let second = next_generation(&handle, 1, first.generation).await;
        assert_eq!(second.tree.children(ROOT), [1]);
        assert_eq!(second.status, Status::Running);
        sender.reconfigure(Config {
            applets: BTreeMap::new(),
            state_base: Some(base),
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            handle.snapshot().slots.get(&1).expect("slot").status,
            Status::Running
        );
        handle
            .offer_options(
                "me.x.A",
                Map::from_iter([("color".to_owned(), Value::from("red"))]),
            )
            .await
            .expect("set options");
        line.clear();
        peer_reader.read_line(&mut line).await.expect("options");
        let options: Value = serde_json::from_str(&line).expect("json");
        assert_eq!(options["t"], "options");
        assert_eq!(options["options"]["color"], "red");
        assert!(
            matches!(handle.offer_options("unknown", Map::new()).await, Err(CommandError::InvalidArgument(reason)) if reason == "unknown applet")
        );
        drop(peer_writer);
        status(&handle, 1, |s| matches!(s, Status::Ended)).await;
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn attached_eof_is_terminal() {
        let (handle, _sender, cancel, task, _) = running_with(
            configured(),
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
                "not installed".to_owned()
            ))))),
        );
        let (pipes, mut peer_writer, mut peer_reader) = attached_peer();
        offer_and_attach(&handle, 1, "me.x.A", Map::new(), pipes).await;
        peer_writer
            .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
            .await
            .expect("hello");
        let mut line = String::new();
        peer_reader.read_line(&mut line).await.expect("host hello");
        drop(peer_writer);
        let ended = status(&handle, 1, |s| matches!(s, Status::Ended)).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            handle.snapshot().slots.get(&1).expect("slot").status,
            Status::Ended
        );
        assert_eq!(ended.generation, 1);
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn attached_read_write_fifo_is_refused() {
        let (handle, _sender, cancel, task, _) = running_with(
            configured(),
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
                "not installed".to_owned()
            ))))),
        );
        let dir = tempfile::tempdir().expect("tempdir");
        let fifo = dir.path().join("fifo");
        rustix::fs::mknodat(
            rustix::fs::CWD,
            &fifo,
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            0,
        )
        .expect("fifo");
        let from_applet = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&fifo)
            .expect("open fifo");
        let (_host_reader, to_applet) = std::io::pipe().expect("pipe");
        offer_and_attach(
            &handle,
            1,
            "me.x.A",
            Map::new(),
            Pipes {
                to_applet: to_applet.into(),
                from_applet: from_applet.into(),
            },
        )
        .await;
        let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
        assert_eq!(
            failed.status,
            Status::Failed("applet pipe must be opened read-only".to_owned())
        );
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn attached_hello_cap_and_missing_state_base() {
        let (handle, _sender, cancel, task, _) = running_with(
            configured(),
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
                "not installed".to_owned()
            ))))),
        );
        let (pipes, mut peer_writer, _peer_reader) = attached_peer();
        offer_and_attach(&handle, 1, "me.x.A", Map::new(), pipes).await;
        for _ in 0..=EXEC_MAX_HELLOS {
            peer_writer
                .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
                .await
                .expect("hello");
        }
        let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
        assert_eq!(failed.status, Status::Failed("too many hellos".to_owned()));
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;

        let mut config = configured();
        config.state_base = None;
        let (handle, _sender, cancel, task, _) = running_with(
            config,
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
                "not installed".to_owned()
            ))))),
        );
        let (pipes, _peer_writer, _peer_reader) = attached_peer();
        offer_and_attach(&handle, 1, "me.x.A", Map::new(), pipes).await;
        let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
        assert_eq!(
            failed.status,
            Status::Failed("cannot determine applet state directory".to_owned())
        );
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn hello_is_answered() {
        let (_dir, entry, output) = script(
            "#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nread -r line\nprintf '%s\\n' \"$line\" > \"$1\"\nwhile read -r line; do printf '%s\\n' \"$line\" >> \"$1\"; done\n",
        );
        let (handle, _sender, cancel, task, _selection) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle.attach(1, "test", placement()).await.expect("attach");
        assert!(matches!(
            status(&handle, 1, |s| matches!(s, Status::Running))
                .await
                .status,
            Status::Running
        ));
        let line = file_contains(&output, "\"t\":\"hello\"").await;
        let reply: Outgoing =
            serde_json::from_str(line.lines().next().expect("line")).expect("reply");
        assert!(
            matches!(reply, Outgoing::Hello { v: 1, options, placement: p, .. } if options.get("color") == Some(&Value::from("blue")) && p == placement())
        );
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn sandbox_denies_ungranted_file() {
        if !landlock_available() {
            return;
        }
        let denied = tempfile::tempdir().expect("denied dir");
        let secret = denied.path().join("secret");
        std::fs::write(&secret, "private\n").expect("secret");
        for granted in [false, true] {
            let (_dir, mut entry, output) = script(
                r#"#!/bin/sh
if read -r value < "$2"; then printf 'allowed' > "$1"; else printf 'denied' > "$1"; fi
printf '{"t":"hello","v":1}\n'
read -r line
sleep 30
"#,
            );
            let mut config = confined(&entry);
            if granted {
                config
                    .applets
                    .get_mut("test")
                    .expect("test applet")
                    .sandbox
                    .read
                    .push(denied.path().display().to_string());
            }
            entry.argv.push(secret.display().to_string());
            let (handle, _, cancel, task, _) = running_with(
                config,
                Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
            );
            handle.attach(1, "test", placement()).await.expect("attach");
            let wanted = if granted { "allowed" } else { "denied" };
            assert_eq!(file_contains(&output, wanted).await, wanted);
            handle.detach(1).await.expect("detach");
            cancel.cancel();
            let _ = task.await;
        }
    }

    #[tokio::test]
    async fn symlink_grant_to_root_fails() {
        if !landlock_available() {
            return;
        }
        let (dir, entry, _) = script("#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\n");
        let link = dir.path().join("root-link");
        std::os::unix::fs::symlink("/", &link).expect("symlink");
        let mut config = confined(&entry);
        config
            .applets
            .get_mut("test")
            .expect("test applet")
            .sandbox
            .read
            .push(link.display().to_string());
        let (handle, _, cancel, task, _) = running_with(
            config,
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
        );
        handle.attach(1, "test", placement()).await.expect("attach");
        let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
        assert!(
            matches!(failed.status, Status::Failed(reason) if reason.contains(&link.display().to_string()))
        );
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn sandbox_denies_ungranted_unix_socket() {
        if !landlock_available() {
            return;
        }
        let denied = tempfile::tempdir().expect("denied dir");
        let socket = denied.path().join("socket");
        let _listener = std::os::unix::net::UnixListener::bind(&socket).expect("socket");
        let (_dir, mut entry, output) = script(
            r#"#!/usr/bin/env python3
import errno, socket, sys, time
try:
    socket.socket(socket.AF_UNIX).connect(sys.argv[2])
    result = "allowed"
except OSError as error:
    result = "denied" if error.errno == errno.EACCES else f"other:{error.errno}"
open(sys.argv[1], "w").write(result)
print('{"t":"hello","v":1}', flush=True)
sys.stdin.readline()
time.sleep(30)
"#,
        );
        let config = confined(&entry);
        entry.argv.push(socket.display().to_string());
        let (handle, _, cancel, task, _) = running_with(
            config,
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
        );
        handle.attach(1, "test", placement()).await.expect("attach");
        assert_eq!(file_contains(&output, "denied").await, "denied");
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn sandbox_denies_signal_to_panel() {
        if !landlock_available() {
            return;
        }
        let (_dir, mut entry, output) = script(
            r#"#!/bin/sh
if kill -0 "$2"; then printf 'allowed' > "$1"; else printf 'denied' > "$1"; fi
printf '{"t":"hello","v":1}\n'
read -r line
sleep 30
"#,
        );
        let config = confined(&entry);
        entry.argv.push(std::process::id().to_string());
        let (handle, _, cancel, task, _) = running_with(
            config,
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
        );
        handle.attach(1, "test", placement()).await.expect("attach");
        assert_eq!(file_contains(&output, "denied").await, "denied");
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn network_grant_controls_tcp_connect() {
        if !landlock_available() {
            return;
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let port = listener.local_addr().expect("address").port();
        let udp = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("UDP listener");
        let udp_port = udp.local_addr().expect("UDP address").port();
        for (network, wanted) in [(false, "denied,denied"), (true, "connected,sent")] {
            let (_dir, mut entry, output) = script(
                r#"#!/usr/bin/env python3
import errno, socket, sys, time
try:
    socket.create_connection(("127.0.0.1", int(sys.argv[2])), timeout=1).close()
    tcp = "connected"
except OSError as error:
    tcp = "denied" if error.errno == errno.EACCES else f"other:{error.errno}"
try:
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.sendto(b"test", ("127.0.0.1", int(sys.argv[3])))
    udp = "sent"
except OSError as error:
    udp = "denied" if error.errno == errno.EACCES else f"other:{error.errno}"
open(sys.argv[1], "w").write(f"{tcp},{udp}")
print('{"t":"hello","v":1}', flush=True)
sys.stdin.readline()
time.sleep(30)
"#,
            );
            let mut config = confined(&entry);
            config
                .applets
                .get_mut("test")
                .expect("test applet")
                .sandbox
                .network = network;
            entry.argv.push(port.to_string());
            entry.argv.push(udp_port.to_string());
            let (handle, _, cancel, task, _) = running_with(
                config,
                Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
            );
            handle.attach(1, "test", placement()).await.expect("attach");
            assert_eq!(file_contains(&output, wanted).await, wanted);
            if network {
                let mut received = [0; 4];
                let (length, _) =
                    tokio::time::timeout(Duration::from_secs(2), udp.recv_from(&mut received))
                        .await
                        .expect("UDP datagram")
                        .expect("UDP receive");
                assert_eq!(&received[..length], b"test");
            }
            handle.detach(1).await.expect("detach");
            cancel.cancel();
            let _ = task.await;
        }
    }

    #[tokio::test]
    async fn hello_state_is_private_writable_and_persistent() {
        if !landlock_available() {
            return;
        }
        let (_dir, entry, output) = script(
            r#"#!/usr/bin/env python3
import json, pathlib, sys, time
print('{"t":"hello","v":1}', flush=True)
hello = json.loads(sys.stdin.readline())
state = pathlib.Path(hello["state"])
with (state / "marker").open("a") as marker:
    marker.write("x")
pathlib.Path(sys.argv[1]).write_text(hello["state"])
time.sleep(30)
"#,
        );
        let config = confined(&entry);
        let state = config
            .state_base
            .as_ref()
            .expect("state base")
            .join("applets/me.example.Test");
        let marker = state.join("marker");
        let (handle, _, cancel, task, _) = running_with(
            config,
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
        );
        handle.attach(1, "test", placement()).await.expect("attach");
        file_contains(&output, &state.display().to_string()).await;
        file_contains(&marker, "x").await;
        assert_eq!(
            std::fs::metadata(&state)
                .expect("state dir")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        handle.detach(1).await.expect("detach");
        handle
            .attach(1, "test", placement())
            .await
            .expect("reattach");
        file_contains(&marker, "xx").await;
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn ungranted_executable_fails_with_sandbox_reason() {
        if !landlock_available() {
            return;
        }
        let (_dir, entry, _) = script("#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\n");
        let mut config = confined(&entry);
        let sandbox = &mut config.applets.get_mut("test").expect("test applet").sandbox;
        sandbox.read.clear();
        sandbox.write.clear();
        let (handle, _, cancel, task, _) = running_with(
            config,
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
        );
        handle.attach(1, "test", placement()).await.expect("attach");
        let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
        assert!(
            matches!(failed.status, Status::Failed(reason) if reason.contains("not granted by sandbox"))
        );
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn missing_state_directory_fails_before_spawn() {
        let (_dir, entry, _) = script("#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\n");
        let mut config = confined(&entry);
        config.state_base = None;
        let (handle, _, cancel, task, _) = running_with(
            config,
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
        );
        handle.attach(1, "test", placement()).await.expect("attach");
        let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
        assert!(
            matches!(failed.status, Status::Failed(reason) if reason == "cannot determine applet state directory")
        );
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn non_utf8_state_directory_fails_before_spawn() {
        use std::os::unix::ffi::OsStringExt;

        let (_dir, entry, output) = script("#!/bin/sh\nprintf x > \"$1\"\n");
        let mut config = confined(&entry);
        config.state_base = Some(
            config
                .state_dir
                .path()
                .join(std::ffi::OsString::from_vec(vec![0xff])),
        );
        let (handle, _, cancel, task, _) = running_with(
            config,
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
        );
        handle.attach(1, "test", placement()).await.expect("attach");
        let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
        assert!(
            matches!(failed.status, Status::Failed(reason) if reason == "cannot determine applet state directory")
        );
        assert!(!output.exists());
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn sandbox_change_respawns() {
        let (_dir, entry, output) = script(
            r#"#!/bin/sh
printf x >> "$1"
printf '{"t":"hello","v":1}\n'
read -r line
sleep 30
"#,
        );
        let config = confined(&entry);
        let mut updated = config.config.clone();
        let (handle, sender, cancel, task, _) = running_with(
            config,
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
        );
        handle.attach(1, "test", placement()).await.expect("attach");
        file_contains(&output, "x").await;
        updated
            .applets
            .get_mut("test")
            .expect("test applet")
            .sandbox
            .read
            .push("/missing-glimpse-test".to_owned());
        sender.reconfigure(updated);
        file_contains(&output, "xx").await;
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn unconfined_runs_ungranted_executable() {
        let (_dir, entry, output) = script(
            "#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nread -r line\nprintf '%s' \"$line\" > \"$1\"\nsleep 30\n",
        );
        let mut config = confined(&entry);
        let sandbox = &mut config.applets.get_mut("test").expect("test applet").sandbox;
        sandbox.read.clear();
        sandbox.write.clear();
        sandbox.unconfined = true;
        let (handle, _, cancel, task, _) = running_with(
            config,
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
        );
        handle.attach(1, "test", placement()).await.expect("attach");
        status(&handle, 1, |s| matches!(s, Status::Running)).await;
        file_contains(&output, "\"state\"").await;
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn placement_change_does_not_respawn() {
        let (_dir, entry, output) = script(
            "#!/bin/sh\necho $$ > \"$1.pid\"\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do printf '%s\\n' \"$line\" >> \"$1\"; done\n",
        );
        let (handle, _sender, cancel, task, _selection) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle.attach(1, "test", placement()).await.expect("attach");
        status(&handle, 1, |s| matches!(s, Status::Running)).await;
        file_contains(&output, "\"t\":\"hello\"").await;
        let pid = tokio::fs::read_to_string(format!("{}.pid", output.display()))
            .await
            .expect("pid");
        let mut changed = placement();
        changed.size = 48;
        handle.place(1, changed).await.expect("place");
        file_contains(&output, "\"t\":\"placement\"").await;
        assert_eq!(
            tokio::fs::read_to_string(format!("{}.pid", output.display()))
                .await
                .expect("pid"),
            pid
        );
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn options_change_does_not_respawn() {
        let (_dir, entry, output) = script(
            "#!/bin/sh\necho $$ > \"$1.pid\"\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do printf '%s\\n' \"$line\" >> \"$1\"; done\n",
        );
        let (handle, sender, cancel, task, _selection) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle.attach(1, "test", placement()).await.expect("attach");
        status(&handle, 1, |s| matches!(s, Status::Running)).await;
        file_contains(&output, "\"t\":\"hello\"").await;
        let pid = tokio::fs::read_to_string(format!("{}.pid", output.display()))
            .await
            .expect("pid");
        let mut config = configured();
        config
            .applets
            .get_mut("test")
            .expect("config")
            .options
            .insert("color".to_owned(), Value::from("red"));
        sender.reconfigure(config.config);
        file_contains(&output, "\"t\":\"options\"").await;
        assert_eq!(
            tokio::fs::read_to_string(format!("{}.pid", output.display()))
                .await
                .expect("pid"),
            pid
        );
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn options_changed_while_starting_follow_hello() {
        let (_dir, entry, output) = script(
            "#!/bin/sh\nsleep 0.3\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nread -r line\nprintf '%s\\n' \"$line\" > \"$1\"\nsleep 1\n",
        );
        let (handle, sender, cancel, task, _selection) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle.attach(1, "test", placement()).await.expect("attach");
        let mut config = configured();
        config
            .applets
            .get_mut("test")
            .expect("config")
            .options
            .insert("color".to_owned(), Value::from("red"));
        sender.reconfigure(config.config);
        file_contains(&output, "\"t\":\"hello\"").await;
        assert!(
            tokio::fs::read_to_string(output)
                .await
                .expect("first line")
                .contains("\"red\"")
        );
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn removed_instance_stays_failed_without_respawning() {
        let (_dir, entry, output) = script(
            "#!/bin/sh\necho $$ >> \"$1\"\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do :; done\n",
        );
        let (handle, sender, cancel, task, _selection) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle.attach(1, "test", placement()).await.expect("attach");
        status(&handle, 1, |s| matches!(s, Status::Running)).await;
        let pid = tokio::fs::read_to_string(&output).await.expect("pid");
        sender.reconfigure(Config {
            applets: BTreeMap::new(),
            state_base: None,
        });
        tokio::time::sleep(Duration::from_secs(6)).await;
        assert!(
            matches!(handle.snapshot().slots.get(&1).map(|s| &s.status), Some(Status::Failed(reason)) if reason == "not configured")
        );
        assert_eq!(
            tokio::fs::read_to_string(&output)
                .await
                .expect("spawns")
                .lines()
                .count(),
            1
        );
        assert!(!PathBuf::from(format!("/proc/{}", pid.trim())).exists());
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn dropping_live_child_stream_reaps_process() {
        let (_dir, entry, output) = script(
            "#!/bin/sh\necho $$ > \"$1\"\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nsleep 30\n",
        );
        let (handle, _sender, cancel, task, _selection) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle.attach(1, "test", placement()).await.expect("attach");
        status(&handle, 1, |s| matches!(s, Status::Running)).await;
        let pid = tokio::fs::read_to_string(output).await.expect("pid");
        cancel.cancel();
        let _ = task.await;
        tokio::time::timeout(Duration::from_secs(1), async {
            while PathBuf::from(format!("/proc/{}", pid.trim())).exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("child reaped");
    }

    #[tokio::test]
    async fn exits_before_hello_fails() {
        let (_dir, entry, output) = script("#!/bin/sh\necho $$ >> \"$1\"\nexit 3\n");
        let catalog = FakeCatalog(Arc::new(Mutex::new(Ok(entry))));
        let (handle, _sender, cancel, task, _selection) = running(catalog.clone());
        handle.attach(1, "test", placement()).await.expect("attach");
        let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
        assert_eq!(
            failed.status,
            Status::Failed("exited with status 3".to_owned())
        );
        tokio::time::timeout(Duration::from_secs(12), async {
            loop {
                if tokio::fs::read_to_string(&output)
                    .await
                    .is_ok_and(|spawns| spawns.lines().count() >= 2)
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("respawned on backoff with an unchanged entry");
        let (_good_dir, good, _good_output) = script(
            "#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do :; done\n",
        );
        *catalog.0.lock().expect("catalog") = Ok(good);
        status(&handle, 1, |s| matches!(s, Status::Running)).await;
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[test]
    fn backoff_table() {
        assert_eq!(backoff(0), Duration::from_secs(1));
        assert_eq!(backoff(1), Duration::from_secs(2));
        assert_eq!(backoff(5), Duration::from_secs(32));
        assert_eq!(backoff(6), Duration::from_secs(60));
        assert_eq!(backoff(30), Duration::from_secs(60));
    }

    #[tokio::test]
    async fn retry_recovers() {
        let (_dir, entry, _output) = script(
            "#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do :; done\n",
        );
        let catalog = FakeCatalog(Arc::new(Mutex::new(Err("not installed".to_owned()))));
        let (handle, _sender, cancel, task, _selection) = running(catalog.clone());
        handle.attach(1, "test", placement()).await.expect("attach");
        assert!(
            matches!(status(&handle, 1, |s| matches!(s, Status::Failed(_))).await.status, Status::Failed(reason) if reason == "not installed")
        );
        *catalog.0.lock().expect("catalog") = Ok(entry);
        status(&handle, 1, |s| matches!(s, Status::Running)).await;
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn stale_generation_is_dropped_and_detach_kills_child() {
        let (_dir, entry, output) = script(
            "#!/bin/sh\necho $$ > \"$1.pid\"\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do printf '%s\\n' \"$line\" >> \"$1\"; done\n",
        );
        let (handle, _sender, cancel, task, _selection) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle.attach(1, "test", placement()).await.expect("attach");
        let state = status(&handle, 1, |s| matches!(s, Status::Running)).await;
        file_contains(&output, "\"t\":\"hello\"").await;
        handle
            .send_event(UserEvent {
                slot: 1,
                generation: state.generation.saturating_sub(1),
                id: 4,
                name: "press".to_owned(),
                args: Vec::new(),
                seq: None,
                gesture: true,
            })
            .await
            .expect("send");
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !tokio::fs::read_to_string(&output)
                .await
                .expect("messages")
                .contains("\"t\":\"event\"")
        );
        let pid: u32 = tokio::fs::read_to_string(format!("{}.pid", output.display()))
            .await
            .expect("pid")
            .trim()
            .parse()
            .expect("numeric pid");
        handle.detach(1).await.expect("detach");
        assert!(!handle.snapshot().slots.contains_key(&1));
        tokio::time::timeout(Duration::from_secs(2), async {
            while std::path::Path::new(&format!("/proc/{pid}")).exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("child reaped");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn requests_require_a_recent_gesture() {
        let (_dir, entry, _output) = script(
            "#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nread -r line\nprintf '{\"t\":\"copy\",\"text\":\"blocked\"}\\n{\"t\":\"open-uri\",\"uri\":\"https://example.com/blocked\"}\\n'\nwhile read -r line; do printf '{\"t\":\"close-popover\"}\\n{\"t\":\"open-uri\",\"uri\":\"file:///x\"}\\n{\"t\":\"open-uri\",\"uri\":\"https://example.com/open\"}\\n{\"t\":\"copy\",\"text\":\"second\"}\\n{\"t\":\"session\",\"action\":\"lock\"}\\n'; done\n",
        );
        let (handle, _sender, cancel, task, selection) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle.attach(1, "test", placement()).await.expect("attach");
        let state = status(&handle, 1, |s| matches!(s, Status::Running)).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(selection.offered().is_empty());
        assert!(
            handle
                .snapshot()
                .slots
                .get(&1)
                .expect("slot")
                .requests
                .is_empty()
        );
        handle
            .send_event(UserEvent {
                slot: 1,
                generation: state.generation,
                id: 1,
                name: "changed".to_owned(),
                args: Vec::new(),
                seq: None,
                gesture: false,
            })
            .await
            .expect("change");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(selection.offered().is_empty());
        handle
            .send_event(UserEvent {
                slot: 1,
                generation: state.generation,
                id: 1,
                name: "clicked".to_owned(),
                args: Vec::new(),
                seq: None,
                gesture: true,
            })
            .await
            .expect("click");
        let accepted = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let state = handle.snapshot();
                if let Some(slot) = state.slots.get(&1)
                    && slot.requests.len() == 2
                {
                    return slot.clone();
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("requests");
        assert_eq!(accepted.requests[0].1, BarRequest::ClosePopover);
        assert_eq!(
            accepted.requests[1].1,
            BarRequest::OpenUri("https://example.com/open".to_owned())
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(selection.offered().is_empty());
        tokio::time::sleep(Duration::from_secs(3)).await;
        handle
            .send_event(UserEvent {
                slot: 1,
                generation: state.generation,
                id: 1,
                name: "changed".to_owned(),
                args: Vec::new(),
                seq: None,
                gesture: false,
            })
            .await
            .expect("late change");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            handle
                .snapshot()
                .slots
                .get(&1)
                .expect("slot")
                .requests
                .len(),
            2
        );
        assert!(selection.offered().is_empty());
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[test]
    fn note_uses_the_entry_name_and_icon() {
        let request = FromApplet::Notify {
            summary: "Done".to_owned(),
            body: "Saved".to_owned(),
            icon: None,
            urgency: Urgency::Normal,
        };
        let entry = Entry {
            argv: Vec::new(),
            cwd: None,
            name: "Pomodoro".to_owned(),
            icon: Some("timer-symbolic".to_owned()),
            path: PathBuf::new(),
            exec: String::new(),
        };
        let note = note(&entry, "me.example.Pomodoro", &request).expect("note");
        assert_eq!(note.name, "Pomodoro");
        assert_eq!(note.id, "me.example.Pomodoro");
        assert_eq!(note.icon, "timer-symbolic");
        assert_eq!(note.summary, "Done");
    }

    #[tokio::test]
    async fn rejects_bad_lines_trees_and_flood_without_stopping_sibling() {
        let bad_source = r##"#!/usr/bin/env python3
import json, sys, time
print('{"t":"hello","v":1}', flush=True)
sys.stdin.readline()
mode = sys.argv[2]
if mode == "garbage":
    print("garbage", flush=True)
elif mode == "huge":
    print("x" * (2 << 20), flush=True)
elif mode == "many":
    nodes = [{"id": i, "type": "box", "children": [{"id": i * 200 + j, "type": "label"} for j in range(1, 201)]} for i in range(2, 18)]
    print(json.dumps({"t": "commit", "ops": [{"op": "insert", "parent": 0, "node": {"id": 1, "type": "popover", "children": nodes}}]}), flush=True)
elif mode == "cycle":
    print(json.dumps({"t": "commit", "ops": [{"op": "insert", "parent": 0, "node": {"id": 1, "type": "popover", "children": [{"id": 2, "type": "box", "children": [{"id": 3, "type": "box"}]}]}}]}), flush=True)
    print(json.dumps({"t": "commit", "ops": [{"op": "move", "parent": 3, "id": 2}]}), flush=True)
time.sleep(30)
"##;
        let good_source = r##"#!/usr/bin/env python3
import json, sys, time
print('{"t":"hello","v":1}', flush=True)
sys.stdin.readline()
print(json.dumps({"t": "commit", "ops": [{"op": "insert", "parent": 0, "node": {"id": 1, "type": "indicator", "props": {"text": "alive"}}}]}), flush=True)
time.sleep(30)
"##;
        for mode in ["garbage", "huge", "many", "cycle"] {
            let (_bad_dir, mut bad, _bad_output) = script(bad_source);
            bad.argv.push(mode.to_owned());
            let (_good_dir, good, _good_output) = script(good_source);
            let mut config = configured();
            config.applets.insert(
                "other".to_owned(),
                glimpse_config::ExecAppletConfig {
                    applet: "me.example.Other".to_owned(),
                    options: Map::new(),
                    sandbox: glimpse_config::AppletSandbox {
                        read: vec![std::env::temp_dir().display().to_string()],
                        write: vec![std::env::temp_dir().display().to_string()],
                        ..Default::default()
                    },
                },
            );
            let (handle, _sender, cancel, task, _selection) =
                running_with(config, Arc::new(PairCatalog { bad, good }));
            handle
                .attach(1, "test", placement())
                .await
                .expect("bad attach");
            handle
                .attach(2, "other", placement())
                .await
                .expect("good attach");
            status(&handle, 1, |s| matches!(s, Status::Restarting { .. })).await;
            let sibling = tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    if let Some(state) = handle.snapshot().slots.get(&2).filter(|s| {
                        matches!(s.status, Status::Running) && s.tree.children(ROOT).len() == 1
                    }) {
                        return state.clone();
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("sibling update");
            assert_eq!(sibling.tree.children(ROOT), &[1], "{mode}");
            handle.detach(1).await.expect("bad detach");
            handle.detach(2).await.expect("good detach");
            cancel.cancel();
            let _ = task.await;
        }
    }

    #[tokio::test]
    async fn full_queue_kills_child() {
        let (_dir, entry, _output) = script(
            "#!/usr/bin/env python3\nimport time\nprint('{\"t\":\"hello\",\"v\":1}', flush=True)\ntime.sleep(30)\n",
        );
        let (handle, _sender, cancel, task, _selection) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle.attach(1, "test", placement()).await.expect("attach");
        let state = status(&handle, 1, |s| matches!(s, Status::Running)).await;
        for _ in 0..200 {
            handle
                .send_event(UserEvent {
                    slot: 1,
                    generation: state.generation,
                    id: 1,
                    name: "changed".to_owned(),
                    args: vec![Value::String("x".repeat(1024))],
                    seq: None,
                    gesture: false,
                })
                .await
                .expect("event");
        }
        status(&handle, 1, |s| matches!(s, Status::Restarting { .. })).await;
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn ten_second_child_restarts_after_backoff() {
        let (_dir, entry, output) = script(
            "#!/usr/bin/env python3\nimport os, sys, time\nmarker = sys.argv[1] + '.marker'\nprint('{\"t\":\"hello\",\"v\":1}', flush=True)\nif not os.path.exists(marker):\n    open(marker, 'w').close()\n    time.sleep(10)\nelse:\n    sys.stdin.readline()\n    time.sleep(30)\n",
        );
        let (handle, _sender, cancel, task, _selection) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle.attach(1, "test", placement()).await.expect("attach");
        let first = status(&handle, 1, |s| matches!(s, Status::Running)).await;
        let restarting = status(&handle, 1, |s| {
            matches!(s, Status::Restarting { attempt: 1 })
        })
        .await;
        assert!(matches!(
            restarting.status,
            Status::Restarting { attempt: 1 }
        ));
        let second = next_generation(&handle, 1, first.generation).await;
        assert!(second.generation > first.generation);
        assert!(output.with_extension("marker").exists());
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn a_second_hello_resets_the_tree_and_bumps_generation_without_respawn() {
        let (_dir, entry, output) = script(
            "#!/usr/bin/env python3\nimport json, os, sys, time\nopen(sys.argv[1] + '.pid', 'w').write(str(os.getpid()))\nprint('{\"t\":\"hello\",\"v\":1}', flush=True)\nsys.stdin.readline()\nprint(json.dumps({\"t\":\"commit\",\"ops\":[{\"op\":\"insert\",\"parent\":0,\"node\":{\"id\":1,\"type\":\"indicator\"}}]}), flush=True)\ntime.sleep(0.05)\nprint('{\"t\":\"hello\",\"v\":1}', flush=True)\nsys.stdin.readline()\ntime.sleep(30)\n",
        );
        let (handle, _sender, cancel, task, _selection) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle.attach(1, "test", placement()).await.expect("attach");
        let first = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(state) = handle
                    .snapshot()
                    .slots
                    .get(&1)
                    .filter(|s| s.tree.children(ROOT).len() == 1)
                {
                    return state.clone();
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("first tree");
        let pid = tokio::fs::read_to_string(format!("{}.pid", output.display()))
            .await
            .expect("pid");
        let second = next_generation(&handle, 1, first.generation).await;
        assert!(second.tree.children(ROOT).is_empty());
        assert_eq!(
            tokio::fs::read_to_string(format!("{}.pid", output.display()))
                .await
                .expect("pid"),
            pid
        );
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    #[ignore = "runs for 61 seconds"]
    async fn child_survives_the_blocking_pool_retirement_window() {
        let (_dir, entry, output) = script(
            "#!/usr/bin/env python3\nimport os, sys, time\nopen(sys.argv[1] + '.pid', 'w').write(str(os.getpid()))\nprint('{\"t\":\"hello\",\"v\":1}', flush=True)\nsys.stdin.readline()\ntime.sleep(70)\n",
        );
        let (handle, _sender, cancel, task, _selection) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle.attach(1, "test", placement()).await.expect("attach");
        status(&handle, 1, |s| matches!(s, Status::Running)).await;
        let pid: u32 = tokio::fs::read_to_string(format!("{}.pid", output.display()))
            .await
            .expect("pid")
            .trim()
            .parse()
            .expect("numeric pid");
        tokio::time::sleep(Duration::from_secs(61)).await;
        assert!(std::path::Path::new(&format!("/proc/{pid}")).exists());
        assert!(matches!(
            handle.snapshot().slots.get(&1).map(|s| &s.status),
            Some(Status::Running)
        ));
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn an_unknown_message_and_a_flood_do_not_stop_the_child() {
        let (_dir, entry, _output) = script(
            "#!/usr/bin/env python3\nimport json, sys, time\nprint('{\"t\":\"hello\",\"v\":1}', flush=True)\nsys.stdin.readline()\nprint('{\"t\":\"dance\"}', flush=True)\nfor _ in range(300):\n    print('{\"t\":\"commit\",\"ops\":[]}', flush=True)\nprint(json.dumps({\"t\": \"commit\", \"ops\": [{\"op\": \"insert\", \"parent\": 0, \"node\": {\"id\": 1, \"type\": \"indicator\"}}]}), flush=True)\ntime.sleep(30)\n",
        );
        let (handle, _sender, cancel, task, _selection) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle.attach(1, "test", placement()).await.expect("attach");
        let state = tokio::time::timeout(Duration::from_secs(6), async {
            loop {
                if let Some(state) = handle
                    .snapshot()
                    .slots
                    .get(&1)
                    .filter(|s| s.tree.children(ROOT).len() == 1)
                {
                    return state.clone();
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("tree after the flood");
        assert_eq!(state.status, Status::Running);
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[test]
    fn only_web_and_mail_uris_are_allowed() {
        assert!(allowed_uri("https://example.com"));
        assert!(allowed_uri("MAILTO:me@example.com"));
        assert!(!allowed_uri("file:///x"));
        assert!(!allowed_uri("javascript:alert(1)"));
        assert!(!allowed_uri(&format!("https://{}", "a".repeat(2048))));
    }

    #[tokio::test]
    async fn a_thousand_stderr_lines_cannot_block_hello() {
        let (_dir, entry, _output) = script(
            "#!/usr/bin/env python3\nimport sys, time\nfor i in range(1000):\n    sys.stderr.write(f'line {i}\\n')\nsys.stderr.write('y' * 10000 + '\\n')\nsys.stderr.flush()\nprint('{\"t\":\"hello\",\"v\":1}', flush=True)\nsys.stdin.readline()\ntime.sleep(30)\n",
        );
        let (handle, _sender, cancel, task, _selection) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle.attach(1, "test", placement()).await.expect("attach");
        status(&handle, 1, |s| matches!(s, Status::Running)).await;
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn a_sixth_hello_within_ten_seconds_restarts_the_child() {
        let (_dir, entry, _output) = script(
            "#!/usr/bin/env python3\nimport time\nfor _ in range(6):\n    print('{\"t\":\"hello\",\"v\":1}', flush=True)\n    time.sleep(0.05)\ntime.sleep(30)\n",
        );
        let (handle, _sender, cancel, task, _selection) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle.attach(1, "test", placement()).await.expect("attach");
        status(&handle, 1, |s| matches!(s, Status::Restarting { .. })).await;
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn a_vanished_entry_stops_the_running_child() {
        let (_dir, entry, output) = script(
            "#!/bin/sh\necho $$ > \"$1\"\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do :; done\n",
        );
        let catalog = FakeCatalog(Arc::new(Mutex::new(Ok(entry))));
        let (handle, _sender, cancel, task, _selection) = running(catalog.clone());
        handle.attach(1, "test", placement()).await.expect("attach");
        status(&handle, 1, |s| matches!(s, Status::Running)).await;
        let pid = tokio::fs::read_to_string(&output).await.expect("pid");
        *catalog.0.lock().expect("catalog") = Err("not installed".to_owned());
        let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
        assert_eq!(failed.status, Status::Failed("not installed".to_owned()));
        tokio::time::timeout(Duration::from_secs(2), async {
            while PathBuf::from(format!("/proc/{}", pid.trim())).exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("child killed");
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn stderr_output_cannot_block_hello() {
        let (_dir, entry, _output) = script(
            "#!/usr/bin/env python3\nimport sys, time\nsys.stderr.write('x' * (2 << 20))\nsys.stderr.flush()\nprint('{\"t\":\"hello\",\"v\":1}', flush=True)\nsys.stdin.readline()\ntime.sleep(30)\n",
        );
        let (handle, _sender, cancel, task, _selection) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        handle
            .attach(9193, "test", placement())
            .await
            .expect("attach");
        status(&handle, 9193, |s| matches!(s, Status::Running)).await;
        handle.detach(9193).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }
}
