mod bridge;
mod catalog;
mod launcher;
mod spawn;
mod tree;
mod wire;

use std::collections::{BTreeMap, BTreeSet};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use glimpse_dbus::notifications::{NotificationUrgency, NotificationsProviderHandle};
use serde_json::{Map, Value};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

use crate::selection::{Offer as SelectionOffer, Selection};
use crate::services::compositor::{CompositorHandle, CompositorState};
use crate::services::mpris::{MprisHandle, MprisPlayers};
use crate::{CommandError, Ctx, Input, Service, ServiceEndpoint, ServiceError, Sub};

pub use launcher::{Launcher, SystemdLauncher, UnitSpec, UnitState};
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
    SparklineProps, SpinnerProps, StackProps, SwatchProps, SwatchSize, SwitchProps,
    ToggleButtonProps, ToggleControl, ToggleRowProps, Tone, Transition, Tree, Violation,
};
pub use wire::{
    Edge, FromApplet, MAX_LINE, Op, Orientation, Outgoing, Placement, Urgency, WireNode, Zone,
};

const MAX_UPDATES_PER_SECOND: u32 = 120;
const RETRY: Duration = Duration::from_secs(5);
const RESET_AFTER: Duration = Duration::from_secs(60);
const TEXT: &str = "text/plain;charset=utf-8";
const URI_CAP: usize = 2048;
const CALLS_PER_SECOND: u32 = 10;
const NAME_CAP: usize = 256;

fn protect_state_dir(state: &Path) -> Result<(), String> {
    std::fs::create_dir_all(state)
        .map_err(|error| format!("cannot create applet state directory: {error}"))?;
    std::fs::set_permissions(state, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("cannot protect applet state directory: {error}"))
}

pub fn exec_state_dir(base: &Path, id: &str) -> Result<PathBuf, String> {
    let state = base.join("applets").join(id);
    protect_state_dir(&state)?;
    Ok(state)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExecState {
    pub slots: BTreeMap<u64, SlotState>,
    pub dev: BTreeSet<String>,
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
    OpenFile { uri: String },
    Launch(String),
    ClosePopover,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub applets: BTreeMap<String, glimpse_config::ExecAppletConfig>,
    pub state_base: Option<PathBuf>,
    pub runtime_base: Option<PathBuf>,
    pub panel: String,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            state_base: glimpse_config::state_dir(),
            runtime_base: glimpse_config::runtime_dir(),
            panel: glimpse_dbus::panel_app_id(),
            applets: glimpse_config::placed_applets(document)
                .filter_map(|(name, applet)| match applet.kind {
                    glimpse_config::AppletKind::Exec(exec) => Some((name.to_owned(), *exec)),
                    _ => None,
                })
                .collect(),
        }
    }
}

impl Config {
    fn socket_path(&self, slot: u64) -> PathBuf {
        let base = self.runtime_base.clone().unwrap_or_else(std::env::temp_dir);
        base.join("applets")
            .join(format!("{}-{slot}.sock", std::process::id()))
    }

    fn sockets_dir(&self) -> Option<PathBuf> {
        self.runtime_base.as_ref().map(|base| base.join("applets"))
    }
}

pub struct Dependencies {
    pub catalog: Arc<dyn Catalog>,
    pub selection: Arc<dyn Selection>,
    pub notifications: NotificationsProviderHandle,
    pub mpris: MprisHandle,
    pub compositor: CompositorHandle,
    pub launcher: Arc<dyn Launcher>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Spawned,
    Dev,
}

pub struct DevRequest {
    pub entry: Entry,
    pub output: Option<String>,
    pub position: Option<Edge>,
    pub options: Map<String, Value>,
    pub env: BTreeMap<String, String>,
    pub stderr: Option<OwnedFd>,
}

struct DevSpec {
    entry: Entry,
    stderr: Option<Arc<OwnedFd>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SubKey {
    Child { slot: u64, spawn: u64 },
    Reset { slot: u64, generation: u64 },
    Retry,
    Mpris,
    Compositor,
}

pub enum Event {
    Started {
        slot: u64,
        spawn: u64,
        entry: Entry,
    },
    Connected {
        slot: u64,
        spawn: u64,
        link: Link,
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
    Disconnected {
        slot: u64,
        spawn: u64,
        spoke: bool,
    },
    Misbehaved {
        slot: u64,
        spawn: u64,
        reason: String,
    },
    Unit {
        slot: u64,
        spawn: u64,
        state: UnitState,
    },
    Exited {
        slot: u64,
        spawn: u64,
        spoke: bool,
        reason: String,
        resolved: Option<Entry>,
    },
    ResetFailed {
        slot: u64,
        generation: u64,
    },
    Retry,
    Mpris(MprisPlayers),
    Compositor(CompositorState),
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
}

pub enum Command {
    Attach {
        slot: u64,
        applet: String,
        placement: Placement,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    DevAttach {
        applet: String,
        request: Box<DevRequest>,
        reply: oneshot::Sender<Result<String, CommandError>>,
    },
    DevRestart {
        handle: String,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    DevDetach {
        handle: String,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    DevSetOptions {
        handle: String,
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
    pub fn attach_dev(
        &self,
        applet: &str,
        request: DevRequest,
    ) -> impl Future<Output = Result<String, CommandError>> + Send + 'static + use<> {
        let (reply, answer) = oneshot::channel();
        let sent = self.0.command(Command::DevAttach {
            applet: applet.to_owned(),
            request: Box::new(request),
            reply,
        });
        async move {
            sent?;
            answer.await.map_err(|_| {
                CommandError::Unavailable("`exec` stopped before answering".to_owned())
            })?
        }
    }
    pub fn restart_dev(
        &self,
        handle: &str,
    ) -> impl Future<Output = Result<(), CommandError>> + Send + 'static + use<> {
        answered(self.submit(|reply| Command::DevRestart {
            handle: handle.to_owned(),
            reply,
        }))
    }
    pub fn detach_dev(
        &self,
        handle: &str,
    ) -> impl Future<Output = Result<(), CommandError>> + Send + 'static + use<> {
        answered(self.submit(|reply| Command::DevDetach {
            handle: handle.to_owned(),
            reply,
        }))
    }
    pub fn dev_set_options(
        &self,
        handle: &str,
        options: Map<String, Value>,
    ) -> impl Future<Output = Result<(), CommandError>> + Send + 'static + use<> {
        answered(self.submit(|reply| Command::DevSetOptions {
            handle: handle.to_owned(),
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
    public: SlotState,
    placement: Placement,
    applet_id: String,
    env: BTreeMap<String, String>,
    state: Option<PathBuf>,
    entry: Option<Entry>,
    dev: Option<DevSpec>,
    catalog_refused: bool,
    options: Map<String, Value>,
    spawn: u64,
    unit: String,
    socket: PathBuf,
    restarts: u32,
    link: Option<Link>,
    stop: CancellationToken,
    popover_open: bool,
    last_notify: Option<Instant>,
    calls: (Instant, u32),
    last_failed_reason: Option<String>,
    serial: u64,
    failed_at: Option<Instant>,
    failures: u32,
    catalog_misses: u32,
    resolving: bool,
    running_since: Option<Instant>,
    reset_done: bool,
}

pub struct Exec {
    state: crate::Publisher<ExecState>,
    config: Config,
    dependencies: Dependencies,
    slots: BTreeMap<u64, Slot>,
    pending_dev: BTreeMap<String, DevRequest>,
    taken_dev: BTreeMap<String, u64>,
    next_spawn: u64,
    next_unit: u64,
    next_generation: u64,
    mpris: Option<Value>,
    compositor: Option<Value>,
}

impl Exec {
    fn desktop_id(&self, name: &str) -> String {
        self.config
            .applets
            .get(name)
            .map_or_else(|| name.to_owned(), |config| config.applet.clone())
    }

    fn next_unit(&mut self, applet_id: &str, slot: u64) -> String {
        self.next_unit += 1;
        launcher::unit_name(&self.config.panel, applet_id, slot, self.next_unit)
    }

    fn dev_slot(
        &mut self,
        slot_id: u64,
        applet: String,
        placement: Placement,
        request: DevRequest,
    ) -> Slot {
        self.next_spawn += 1;
        let applet_id = self.desktop_id(&applet);
        let unit = self.next_unit(&applet_id, slot_id);
        let state = self
            .config
            .state_base
            .as_ref()
            .map(|base| base.join("applets").join(&applet_id));
        Slot {
            origin: Origin::Dev,
            public: SlotState {
                applet,
                generation: 0,
                tree: Arc::new(Tree::default()),
                status: Status::Starting,
                title: request.entry.name.clone(),
                icon: request.entry.icon.clone(),
                requests: Vec::new(),
            },
            placement,
            applet_id,
            env: request.env,
            state,
            entry: Some(request.entry.clone()),
            dev: Some(DevSpec {
                entry: request.entry,
                stderr: request.stderr.map(Arc::new),
            }),
            catalog_refused: false,
            options: request.options,
            spawn: self.next_spawn,
            unit,
            socket: self.config.socket_path(slot_id),
            restarts: 0,
            link: None,
            stop: CancellationToken::new(),
            popover_open: false,
            last_notify: None,
            calls: (Instant::now(), 0),
            last_failed_reason: None,
            serial: 0,
            failed_at: None,
            failures: 0,
            catalog_misses: 0,
            resolving: false,
            running_since: None,
            reset_done: false,
        }
    }

    fn finish_dev(&mut self, id: u64) {
        let Some(slot) = self.slots.get_mut(&id) else {
            return;
        };
        self.taken_dev.remove(&slot.applet_id);
        slot.link = None;
        slot.dev = None;
        if let Some(config) = self.config.applets.get(&slot.public.applet) {
            tracing::info!(applet = %slot.applet_id, slot = id, "dev applet detached, returning the configured one");
            slot.origin = Origin::Spawned;
            slot.applet_id = config.applet.clone();
            slot.env = config.env.clone();
            slot.options = config.options.clone();
            slot.state = self
                .config
                .state_base
                .as_ref()
                .map(|base| base.join("applets").join(&config.applet));
            slot.entry = None;
            slot.failures = 0;
            slot.last_failed_reason = None;
            self.restart_slot(id);
        } else {
            tracing::info!(applet = %slot.applet_id, slot = id, "dev applet detached");
            slot.stop.cancel();
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
            dev: self
                .pending_dev
                .keys()
                .chain(self.taken_dev.keys())
                .cloned()
                .collect(),
        });
    }

    fn restart_slot(&mut self, id: u64) {
        self.next_spawn += 1;
        let spawn = self.next_spawn;
        let Some(applet_id) = self.slots.get(&id).map(|slot| slot.applet_id.clone()) else {
            return;
        };
        let unit = self.next_unit(&applet_id, id);
        if let Some(slot) = self.slots.get_mut(&id) {
            tracing::info!(applet = %slot.applet_id, slot = id, %unit, "applet restarting");
            if let Some(link) = slot.link.take() {
                link.stop.cancel();
            }
            slot.stop.cancel();
            slot.stop = CancellationToken::new();
            slot.spawn = spawn;
            slot.unit = unit;
            slot.restarts = 0;
            slot.public.status = Status::Starting;
            slot.public.tree = Arc::new(Tree::default());
            slot.entry = slot.dev.as_ref().map(|dev| dev.entry.clone());
            slot.catalog_refused = false;
            slot.failed_at = None;
            slot.running_since = None;
            slot.reset_done = false;
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
            tracing::warn!(applet = %slot.public.applet, "not reading its socket");
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
        let oversized = match &request {
            FromApplet::Mpris { player, .. } => player.len() > NAME_CAP,
            FromApplet::Compositor {
                call:
                    wire::CompositorCall::RenameWorkspace {
                        name: Some(name), ..
                    },
            } => name.len() > NAME_CAP,
            _ => false,
        };
        if oversized {
            tracing::warn!(applet = %slot.public.applet, "applet sent an oversized service argument");
            return;
        }
        match request {
            FromApplet::Mpris { .. } | FromApplet::Compositor { .. } => {
                if slot.calls.0.elapsed() >= Duration::from_secs(1) {
                    slot.calls = (Instant::now(), 0);
                }
                if slot.calls.1 >= CALLS_PER_SECOND {
                    tracing::warn!(applet = %slot.public.applet, "service call rate exceeded");
                    return;
                }
                slot.calls.1 += 1;
                self.call(request, ctx);
            }
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
            FromApplet::OpenUri { uri } => {
                if !allowed_uri(&uri) {
                    tracing::warn!(applet = %slot.public.applet, "applet asked to open a refused uri");
                } else if is_file(&uri) {
                    slot.push_request(BarRequest::OpenFile { uri });
                } else {
                    slot.push_request(BarRequest::OpenUri(uri));
                }
            }
            FromApplet::Launch { id } => {
                if id.strip_suffix(".desktop").unwrap_or(&id).is_empty() {
                    tracing::warn!(applet = %slot.public.applet, "applet asked to launch an empty id");
                } else {
                    slot.push_request(BarRequest::Launch(id));
                }
            }
            FromApplet::ClosePopover => slot.push_request(BarRequest::ClosePopover),
            _ => {}
        }
    }
}

impl Exec {
    fn call(&self, request: FromApplet, ctx: &Ctx<Self>) {
        let mpris = self.dependencies.mpris.clone();
        let compositor = self.dependencies.compositor.clone();
        ctx.spawn_detached(move |_ctx| async move {
            let result = match request {
                FromApplet::Mpris { player, call } => bridge::mpris_call(mpris, player, call).await,
                FromApplet::Compositor { call } => bridge::compositor_call(compositor, call).await,
                _ => Ok(()),
            };
            if let Err(error) = result {
                tracing::warn!(%error, "applet service call failed");
            }
        });
    }

    fn share(&mut self, name: &str, value: Value) {
        let slot_value = match name {
            "mpris" => &mut self.mpris,
            _ => &mut self.compositor,
        };
        if slot_value.as_ref() == Some(&value) {
            return;
        }
        *slot_value = Some(value.clone());
        for slot in self.slots.values_mut() {
            if matches!(slot.public.status, Status::Running) {
                Self::send(
                    slot,
                    Outgoing::Service {
                        name: name.to_owned(),
                        value: value.clone(),
                    },
                );
            }
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

fn is_file(uri: &str) -> bool {
    uri.split_once(':')
        .is_some_and(|(scheme, _)| scheme.eq_ignore_ascii_case("file"))
}

fn allowed_uri(uri: &str) -> bool {
    uri.len() <= URI_CAP
        && uri.split_once(':').is_some_and(|(scheme, _)| {
            ["http", "https", "mailto", "file"]
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

fn prune_dead_sockets(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(pid) = name
            .strip_suffix(".sock")
            .and_then(|stem| stem.split('-').next())
            .and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        if pid != std::process::id() && !Path::new(&format!("/proc/{pid}")).exists() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
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
            if matches!(
                slot.public.status,
                Status::Starting | Status::Running | Status::Restarting { .. }
            ) {
                let catalog = Arc::clone(&self.dependencies.catalog);
                let (entry, cwd, stderr) = match &slot.dev {
                    Some(dev) => (
                        Some(dev.entry.clone()),
                        dev.entry.path.parent().map(Path::to_path_buf),
                        dev.stderr
                            .as_ref()
                            .and_then(|fd| fd.as_fd().try_clone_to_owned().ok()),
                    ),
                    None => (slot.entry.clone(), None, None),
                };
                let args = spawn::SourceArgs {
                    slot: slot_id,
                    spawn: slot.spawn,
                    id: slot.applet_id.clone(),
                    env: slot.env.clone(),
                    state_base: self.config.state_base.clone(),
                    entry,
                    cwd,
                    restart: slot.origin == Origin::Spawned,
                    stderr,
                    unit: slot.unit.clone(),
                    socket: slot.socket.clone(),
                    launcher: Arc::clone(&self.dependencies.launcher),
                    stop: slot.stop.clone(),
                };
                subs.push(Sub::stream(
                    SubKey::Child {
                        slot: slot_id,
                        spawn: slot.spawn,
                    },
                    move |ctx| async move {
                        let _ = ctx;
                        spawn::source(catalog, args).await
                    },
                ));
            }
            if matches!(slot.public.status, Status::Running)
                && !slot.reset_done
                && let Some(since) = slot.running_since
            {
                let remaining = RESET_AFTER.saturating_sub(since.elapsed());
                let at = Utc::now() + chrono::Duration::from_std(remaining).unwrap_or_default();
                subs.push(Sub::deadline(
                    SubKey::Reset {
                        slot: slot_id,
                        generation: slot.public.generation,
                    },
                    at,
                    Event::ResetFailed {
                        slot: slot_id,
                        generation: slot.public.generation,
                    },
                ));
            }
        }
        let active = self.slots.values().any(|slot| {
            matches!(
                slot.public.status,
                Status::Starting | Status::Running | Status::Restarting { .. }
            )
        });
        if active {
            subs.push(Sub::watch(
                SubKey::Mpris,
                self.dependencies.mpris.subscribe(),
                Event::Mpris,
                Event::Mpris(MprisPlayers::default()),
            ));
            subs.push(Sub::watch(
                SubKey::Compositor,
                self.dependencies.compositor.subscribe(),
                Event::Compositor,
                Event::Compositor(CompositorState::default()),
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
        let launcher = Arc::clone(&dependencies.launcher);
        let prefix = launcher::sweep_prefix(&config.panel);
        let sockets = config.sockets_dir();
        ctx.spawn_detached(move |_ctx| async move {
            if let Some(dir) = sockets {
                let _ = tokio::task::spawn_blocking(move || prune_dead_sockets(&dir)).await;
            }
            launcher.sweep(&prefix).await;
        });
        Ok(Self {
            state: ctx.publisher(),
            config,
            dependencies,
            slots: BTreeMap::new(),
            pending_dev: BTreeMap::new(),
            taken_dev: BTreeMap::new(),
            next_spawn: 0,
            next_unit: 0,
            next_generation: 0,
            mpris: None,
            compositor: None,
        })
    }

    async fn stop(self, _ctx: &Ctx<Self>) {
        for slot in self.slots.values() {
            self.dependencies.launcher.stop(&slot.unit).await;
            let _ = tokio::fs::remove_file(&slot.socket).await;
        }
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
                if self.pending_dev.get(&desktop_id).is_some_and(|request| {
                    request.output.is_none() || request.output == placement.output
                }) && let Some(request) = self.pending_dev.remove(&desktop_id)
                {
                    tracing::info!(applet = %desktop_id, slot, output = ?placement.output, "dev applet attached");
                    let dev = self.dev_slot(slot, applet, placement, request);
                    self.slots.insert(slot, dev);
                    self.taken_dev.insert(desktop_id, slot);
                    let _ = reply.send(Ok(()));
                    self.publish();
                    return;
                }
                let config = self.config.applets.get(&applet);
                let (applet_id, env, options, status) = match config {
                    Some(config) => (
                        config.applet.clone(),
                        config.env.clone(),
                        config.options.clone(),
                        Status::Starting,
                    ),
                    None => (
                        String::new(),
                        BTreeMap::new(),
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
                let unit = self.next_unit(&applet_id, slot);
                self.slots.insert(
                    slot,
                    Slot {
                        origin: Origin::Spawned,
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
                        env,
                        state,
                        entry: None,
                        dev: None,
                        catalog_refused: false,
                        options,
                        spawn: self.next_spawn,
                        unit,
                        socket: self.config.socket_path(slot),
                        restarts: 0,
                        link: None,
                        stop: CancellationToken::new(),
                        popover_open: false,
                        last_notify: None,
                        calls: (Instant::now(), 0),
                        last_failed_reason,
                        serial: 0,
                        failed_at: None,
                        failures: 0,
                        catalog_misses: 0,
                        resolving: false,
                        running_since: None,
                        reset_done: false,
                    },
                );
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::DevAttach {
                applet,
                request,
                reply,
            }) => {
                let request = *request;
                let id = self.desktop_id(&applet);
                let result = if self.pending_dev.contains_key(&id)
                    || self.taken_dev.contains_key(&id)
                {
                    tracing::info!(applet = %id, "dev attach refused, already attached");
                    Err(CommandError::InvalidArgument(format!(
                        "{id} is already attached"
                    )))
                } else if let Some((&slot_id, _)) = self.slots.iter().find(|(_, slot)| {
                    self.desktop_id(&slot.public.applet) == id
                        && (slot.origin == Origin::Spawned
                            || (slot.origin == Origin::Dev
                                && matches!(slot.public.status, Status::Ended | Status::Failed(_))))
                        && (request.output.is_none() || request.output == slot.placement.output)
                        && request
                            .position
                            .is_none_or(|edge| edge == slot.placement.position)
                }) {
                    if let Some(slot) = self.slots.get_mut(&slot_id) {
                        tracing::info!(applet = %id, slot = slot_id, output = ?slot.placement.output, "applet taken over by a dev slot");
                        slot.origin = Origin::Dev;
                        slot.applet_id = id.clone();
                        slot.state = self
                            .config
                            .state_base
                            .as_ref()
                            .map(|base| base.join("applets").join(&id));
                        slot.env = request.env;
                        slot.dev = Some(DevSpec {
                            entry: request.entry.clone(),
                            stderr: request.stderr.map(Arc::new),
                        });
                        slot.entry = Some(request.entry.clone());
                        slot.options = request.options;
                        slot.public.title = request.entry.name;
                        slot.public.icon = request.entry.icon;
                        slot.failures = 0;
                        slot.last_failed_reason = None;
                        slot.failed_at = None;
                        slot.catalog_refused = false;
                        slot.resolving = false;
                    }
                    self.restart_slot(slot_id);
                    self.taken_dev.insert(id.clone(), slot_id);
                    Ok(id.clone())
                } else {
                    tracing::info!(applet = %id, output = ?request.output, "dev applet waiting for its widget");
                    self.pending_dev.insert(id.clone(), request);
                    Ok(id.clone())
                };
                let _ = reply.send(result);
            }
            Input::Command(Command::DevRestart { handle, reply }) => {
                let result = if let Some(&slot_id) = self.taken_dev.get(&handle) {
                    self.restart_slot(slot_id);
                    Ok(())
                } else if self.pending_dev.contains_key(&handle) {
                    Ok(())
                } else {
                    Err(CommandError::InvalidArgument("unknown applet".to_owned()))
                };
                let _ = reply.send(result);
            }
            Input::Command(Command::DevDetach { handle, reply }) => {
                let result = if let Some(&slot_id) = self.taken_dev.get(&handle) {
                    self.finish_dev(slot_id);
                    Ok(())
                } else if self.pending_dev.remove(&handle).is_some() {
                    Ok(())
                } else {
                    Err(CommandError::InvalidArgument("unknown applet".to_owned()))
                };
                let _ = reply.send(result);
            }
            Input::Command(Command::DevSetOptions {
                handle,
                options,
                reply,
            }) => {
                let result = if let Some(request) = self.pending_dev.get_mut(&handle) {
                    request.options = options;
                    Ok(())
                } else if let Some(slot) = self
                    .taken_dev
                    .get(&handle)
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
                if let Some(state) = self.slots.remove(&slot) {
                    state.stop.cancel();
                    if state.origin == Origin::Dev {
                        self.taken_dev.remove(&state.applet_id);
                    }
                    let launcher = Arc::clone(&self.dependencies.launcher);
                    let unit = state.unit.clone();
                    let socket = state.socket.clone();
                    ctx.spawn_detached(move |_ctx| async move {
                        launcher.stop(&unit).await;
                        let _ = tokio::fs::remove_file(&socket).await;
                    });
                }
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::SendEvent { event, reply }) => {
                if let Some(slot) = self.slots.get_mut(&event.slot)
                    && slot.public.generation == event.generation
                    && matches!(slot.public.status, Status::Running)
                {
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

            Input::Event(Event::Started { slot, spawn, entry }) => {
                if let Some(state) = self.slots.get_mut(&slot).filter(|s| s.spawn == spawn) {
                    state.public.title = entry.name.clone();
                    state.public.icon = entry.icon.clone();
                    state.entry = Some(entry);
                }
            }
            Input::Event(Event::Connected { slot, spawn, link }) => {
                if let Some(state) = self.slots.get_mut(&slot).filter(|s| s.spawn == spawn) {
                    state.link = Some(link);
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
                    state.running_since = Some(Instant::now());
                    state.reset_done = false;
                    state.last_failed_reason = None;
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
                    for (name, value) in [("mpris", &self.mpris), ("compositor", &self.compositor)]
                    {
                        if let Some(value) = value {
                            Self::send(
                                state,
                                Outgoing::Service {
                                    name: name.to_owned(),
                                    value: value.clone(),
                                },
                            );
                        }
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
            Input::Event(Event::Disconnected { slot, spawn, .. })
            | Input::Event(Event::Misbehaved { slot, spawn, .. }) => {
                if let Some(state) = self.slots.get_mut(&slot).filter(|s| s.spawn == spawn) {
                    state.link = None;
                    state.public.tree = Arc::new(Tree::default());
                    state.running_since = None;
                    state.public.status = match state.origin {
                        Origin::Dev => Status::Ended,
                        Origin::Spawned
                            if self.config.applets.contains_key(&state.public.applet) =>
                        {
                            Status::Restarting {
                                attempt: state.restarts,
                            }
                        }
                        Origin::Spawned => Status::Failed("not configured".to_owned()),
                    };
                }
            }
            Input::Event(Event::Unit { slot, spawn, state }) => {
                if let Some(current) = self.slots.get_mut(&slot).filter(|s| s.spawn == spawn) {
                    match state {
                        UnitState::Restarting { restarts } => {
                            current.restarts = restarts;
                            current.link = None;
                            current.running_since = None;
                            if current.origin == Origin::Spawned
                                && !matches!(current.public.status, Status::Failed(_))
                            {
                                current.public.status = Status::Restarting { attempt: restarts };
                            }
                        }
                        UnitState::Failed { result } => {
                            if current.last_failed_reason.as_deref() != Some(result.as_str()) {
                                tracing::warn!(applet = %current.public.applet, %result, "applet unit failed");
                            }
                            current.link = None;
                            current.running_since = None;
                            current.last_failed_reason = Some(result.clone());
                            current.failures = current.failures.saturating_add(1);
                            current.failed_at = Some(Instant::now());
                            current.public.status = Status::Failed(result);
                        }
                        UnitState::Gone => {
                            current.link = None;
                            current.running_since = None;
                            if matches!(
                                current.public.status,
                                Status::Starting | Status::Restarting { .. }
                            ) {
                                current.public.status = Status::Ended;
                            }
                        }
                    }
                }
            }
            Input::Event(Event::Exited {
                slot,
                spawn,
                reason,
                resolved,
                ..
            }) => {
                if let Some(state) = self.slots.get_mut(&slot).filter(|s| s.spawn == spawn) {
                    state.link = None;
                    if let Some(entry) = resolved {
                        state.entry = Some(entry);
                    }
                    state.public.tree = Arc::new(Tree::default());
                    if state.origin == Origin::Spawned
                        && !self.config.applets.contains_key(&state.public.applet)
                    {
                        state.public.status = Status::Failed("not configured".to_owned());
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
            Input::Event(Event::ResetFailed { slot, generation }) => {
                if let Some(state) = self.slots.get_mut(&slot).filter(|s| {
                    s.public.generation == generation
                        && matches!(s.public.status, Status::Running)
                        && !s.reset_done
                }) {
                    state.reset_done = true;
                    state.restarts = 0;
                    let launcher = Arc::clone(&self.dependencies.launcher);
                    let unit = state.unit.clone();
                    ctx.spawn_detached(move |_ctx| async move {
                        launcher.reset_failed(&unit).await;
                    });
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
            Input::Event(Event::Mpris(players)) => {
                self.share("mpris", bridge::mpris_value(&players));
            }
            Input::Event(Event::Compositor(state)) => {
                self.share("compositor", bridge::compositor_value(&state));
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
                } else if let Some(state) = self.slots.get(&slot)
                    && state.spawn == spawn
                    && matches!(state.public.status, Status::Failed(_))
                    && self.config.applets.contains_key(&state.public.applet)
                {
                    match resolved {
                        Some(Ok(entry))
                            if state.catalog_refused || state.entry.as_ref() != Some(&entry) =>
                        {
                            self.restart_slot(slot)
                        }
                        Some(Err(_)) => {
                            if let Some(state) = self.slots.get_mut(&slot) {
                                state.catalog_refused = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Input::Config(config) => {
                let mut respawn = Vec::new();
                for (&id, slot) in &mut self.slots {
                    if slot.origin == Origin::Dev {
                        continue;
                    }
                    match config.applets.get(&slot.public.applet) {
                        Some(applet) if applet.applet != slot.applet_id => {
                            slot.applet_id = applet.applet.clone();
                            slot.env = applet.env.clone();
                            slot.state = config
                                .state_base
                                .as_ref()
                                .map(|base| base.join("applets").join(&applet.applet));
                            slot.options = applet.options.clone();
                            respawn.push(id);
                        }
                        Some(applet) if applet.env != slot.env => {
                            slot.env = applet.env.clone();
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
                            slot.stop.cancel();
                        }
                        _ => {}
                    }
                }
                self.config = config;
                for id in respawn {
                    self.restart_slot(id);
                }
            }
        }
        self.publish();
    }
}

#[cfg(test)]
mod tests;
