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
    ActionRowProps, AddonProps, Align, BarsProps, BoxProps, ButtonProps, CalendarMarker,
    CalendarProps, CheckButtonProps, ChoiceItem, ChoiceListProps, ClassName, ContainerProps,
    DotsProps, DropDownProps, Element, ElementKind, Ellipsize, EndProps, EntryProps,
    ExpanderRowProps, FactItem, FactListProps, ImageProps, IndicatorProps, LabelProps,
    MAX_CHILDREN_PER_PARENT, MAX_NODES, MeterRowProps, Node, NoticeProps, Paint, PlaceholderProps,
    PopoverProps, PopoverSize, ROOT, RangeBarProps, ReadoutProps, RevealerProps, RowProps, RowTone,
    RowTypography, ScaleProps, SectionProps, SeparatorProps, Severity, SliderRowProps,
    SparklineProps, SpinnerProps, StackProps, SwitchProps, ToggleButtonProps, ToggleControl,
    ToggleRowProps, Tone, Transition, Tree, Violation,
};
pub use wire::{
    Edge, FromApplet, MAX_LINE, Op, Orientation, Outgoing, Placement, Urgency,
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
mod tests;
