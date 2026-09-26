mod catalog;
mod scope;
mod spawn;
mod tree;
mod wire;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use glimpse_dbus::notifications::{NotificationUrgency, NotificationsProviderHandle};
use serde_json::{Map, Value};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

use crate::selection::{Offer, Selection};
use crate::{CommandError, Ctx, Input, Service, ServiceEndpoint, ServiceError, Sub};

pub use scope::{unit_name, unit_pattern};

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
const GESTURE: Duration = Duration::from_secs(2);
const TEXT: &str = "text/plain;charset=utf-8";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExecState {
    pub slots: BTreeMap<u64, SlotState>,
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
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SubKey {
    Child { slot: u64, spawn: u64 },
    Backoff { slot: u64, spawn: u64 },
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
    Backoff {
        slot: u64,
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

    async fn submit(
        &self,
        build: impl FnOnce(oneshot::Sender<Result<(), CommandError>>) -> Command,
    ) -> Result<(), CommandError> {
        let (reply, answer) = oneshot::channel();
        self.0.command(build(reply))?;
        answer
            .await
            .map_err(|_| CommandError::Unavailable("`exec` stopped before answering".to_owned()))?
    }

    pub async fn attach(
        &self,
        slot: u64,
        applet: &str,
        placement: Placement,
    ) -> Result<(), CommandError> {
        self.submit(|reply| Command::Attach {
            slot,
            applet: applet.to_owned(),
            placement,
            reply,
        })
        .await
    }
    pub async fn place(&self, slot: u64, placement: Placement) -> Result<(), CommandError> {
        self.submit(|reply| Command::Place {
            slot,
            placement,
            reply,
        })
        .await
    }
    pub async fn detach(&self, slot: u64) -> Result<(), CommandError> {
        self.submit(|reply| Command::Detach { slot, reply }).await
    }
    pub async fn send_event(&self, event: UserEvent) -> Result<(), CommandError> {
        self.submit(|reply| Command::SendEvent { event, reply })
            .await
    }
    pub async fn popover(&self, slot: u64, open: bool) -> Result<(), CommandError> {
        self.submit(|reply| Command::Popover { slot, open, reply })
            .await
    }
}

struct Slot {
    public: SlotState,
    placement: Placement,
    applet_id: String,
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
    detached: bool,
}

pub struct Exec {
    state: crate::Publisher<ExecState>,
    config: Config,
    dependencies: Dependencies,
    slots: BTreeMap<u64, Slot>,
    next_spawn: u64,
    next_generation: u64,
}

impl Exec {
    fn publish(&self) {
        self.state.set(ExecState {
            slots: self
                .slots
                .iter()
                .filter(|(_, slot)| !slot.detached)
                .map(|(id, slot)| (*id, slot.public.clone()))
                .collect(),
        });
    }

    fn spawn_slot(&mut self, id: u64) {
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
        }
    }

    fn send(slot: &mut Slot, message: Outgoing) {
        if let Some(link) = &slot.link
            && let Err(mpsc::error::TrySendError::Full(_)) = link.tx.try_send(message)
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
        if slot.last_gesture.is_none_or(|at| at.elapsed() > GESTURE) {
            if !slot.warned_gate {
                tracing::warn!(applet = %slot.public.applet, "applet request without recent gesture");
                slot.warned_gate = true;
            }
            return;
        }
        match request {
            FromApplet::Copy { text } => {
                if text.len() <= 1 << 20 {
                    let offer = Offer {
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
        icon: icon
            .as_deref()
            .or(entry.icon.as_deref())
            .unwrap_or_default()
            .to_owned(),
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
                        stop: slot.stop.clone(),
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
                Status::Failed(_) => {}
            }
        }
        if self
            .slots
            .values()
            .any(|slot| matches!(slot.public.status, Status::Failed(_)))
        {
            subs.push(Sub::stream(SubKey::Retry, |_ctx| async {
                futures_util::stream::unfold((), |_| async {
                    tokio::time::sleep(RETRY).await;
                    Some((Event::Retry, ()))
                })
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
            next_spawn: 0,
            next_generation: 0,
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
                let config = self.config.applets.get(&applet);
                let (applet_id, options, status) = match config {
                    Some(config) => (
                        config.applet.clone(),
                        config.options.clone(),
                        Status::Starting,
                    ),
                    None => (
                        String::new(),
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
                self.next_spawn += 1;
                self.slots.insert(
                    slot,
                    Slot {
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
                        detached: false,
                    },
                );
                let _ = reply.send(Ok(()));
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
                if let Some(state) = self.slots.get_mut(&slot) {
                    state.detached = true;
                    state.stop.cancel();
                    if let Some(link) = &state.link {
                        link.stop.cancel();
                    } else if !matches!(state.public.status, Status::Starting) {
                        self.slots.remove(&slot);
                    }
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
                    if state.detached
                        && let Some(link) = &state.link
                    {
                        link.stop.cancel();
                    }
                } else {
                    link.stop.cancel();
                }
            }
            Input::Event(Event::Hello { slot, spawn }) => {
                if let Some(state) = self.slots.get_mut(&slot).filter(|s| s.spawn == spawn) {
                    self.next_generation += 1;
                    state.public.generation = self.next_generation;
                    state.public.tree = Arc::new(Tree::default());
                    state.public.status = Status::Running;
                    state.last_failed_reason = None;
                    Self::send(
                        state,
                        Outgoing::Hello {
                            v: 1,
                            name: state.public.title.clone(),
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
                if self
                    .slots
                    .get(&slot)
                    .is_some_and(|s| s.spawn == spawn && s.detached)
                {
                    self.slots.remove(&slot);
                } else if let Some(state) = self.slots.get_mut(&slot).filter(|s| s.spawn == spawn) {
                    state.link = None;
                    if let Some(entry) = resolved {
                        state.entry = Some(entry);
                    }
                    state.public.tree = Arc::new(Tree::default());
                    if !self.config.applets.contains_key(&state.public.applet) {
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
                        state.public.status = Status::Failed(reason);
                    }
                }
            }
            Input::Event(Event::Backoff { slot }) => {
                if self.slots.get(&slot).is_some_and(|s| {
                    matches!(s.public.status, Status::Restarting { .. })
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
                for (&slot, state) in &self.slots {
                    if matches!(state.public.status, Status::Failed(_))
                        && self.config.applets.contains_key(&state.public.applet)
                    {
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
                if let Some(state) = self.slots.get_mut(&slot)
                    && state.spawn == spawn
                    && matches!(state.public.status, Status::Failed(_))
                    && self.config.applets.contains_key(&state.public.applet)
                {
                    match resolved {
                        Some(Ok(entry))
                            if state.catalog_refused || state.entry.as_ref() != Some(&entry) =>
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
                    match config.applets.get(&slot.public.applet) {
                        Some(applet) if applet.applet != slot.applet_id => {
                            slot.applet_id = applet.applet.clone();
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

    fn configured() -> Config {
        Config {
            applets: BTreeMap::from([(
                "test".to_owned(),
                glimpse_config::ExecAppletConfig {
                    applet: "me.example.Test".to_owned(),
                    options: Map::from_iter([("color".to_owned(), Value::from("blue"))]),
                },
            )]),
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
        config: Config,
        catalog: Arc<dyn Catalog>,
    ) -> (
        ExecHandle,
        ServiceSender<Exec>,
        CancellationToken,
        tokio::task::JoinHandle<()>,
        FakeSelection,
    ) {
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
        sender.reconfigure(config);
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
        sender.reconfigure(config);
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
        status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
        tokio::time::sleep(Duration::from_secs(6)).await;
        assert!(matches!(
            handle.snapshot().slots.get(&1).map(|s| &s.status),
            Some(Status::Failed(_))
        ));
        assert_eq!(
            tokio::fs::read_to_string(output)
                .await
                .expect("spawns")
                .lines()
                .count(),
            1
        );
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
            "#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nread -r line\nprintf '{\"t\":\"copy\",\"text\":\"blocked\"}\\n{\"t\":\"open-uri\",\"uri\":\"https://example.com/blocked\"}\\n'\nwhile read -r line; do printf '{\"t\":\"copy\",\"text\":\"accepted\"}\\n{\"t\":\"open-uri\",\"uri\":\"https://example.com/open\"}\\n{\"t\":\"session\",\"action\":\"lock\"}\\n{\"t\":\"close-popover\"}\\n'; done\n",
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
                    && slot.requests.len() == 3
                {
                    return slot.clone();
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("requests");
        assert_eq!(
            accepted.requests[0].1,
            BarRequest::OpenUri("https://example.com/open".to_owned())
        );
        assert_eq!(
            accepted.requests[1].1,
            BarRequest::Session(SessionVerb::Lock)
        );
        assert_eq!(accepted.requests[2].1, BarRequest::ClosePopover);
        assert_eq!(selection.offered()[0].data.as_ref(), b"accepted");
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
            3
        );
        assert_eq!(selection.offered().len(), 1);
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
elif mode == "flood":
    for _ in range(500):
        print('{"t":"commit","ops":[]}', flush=True)
elif mode == "request-flood":
    for _ in range(500):
        print('{"t":"close-popover"}', flush=True)
elif mode == "hello-flood":
    for _ in range(200):
        print('{"t":"hello","v":1}', flush=True)
        sys.stdin.readline()
        print('{"t":"commit","ops":[]}', flush=True)
time.sleep(30)
"##;
        let good_source = r##"#!/usr/bin/env python3
import json, sys, time
print('{"t":"hello","v":1}', flush=True)
sys.stdin.readline()
print(json.dumps({"t": "commit", "ops": [{"op": "insert", "parent": 0, "node": {"id": 1, "type": "indicator", "props": {"text": "alive"}}}]}), flush=True)
time.sleep(30)
"##;
        for mode in [
            "garbage",
            "huge",
            "many",
            "cycle",
            "flood",
            "request-flood",
            "hello-flood",
        ] {
            let (_bad_dir, mut bad, _bad_output) = script(bad_source);
            bad.argv.push(mode.to_owned());
            let (_good_dir, good, _good_output) = script(good_source);
            let mut config = configured();
            config.applets.insert(
                "other".to_owned(),
                glimpse_config::ExecAppletConfig {
                    applet: "me.example.Other".to_owned(),
                    options: Map::new(),
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
