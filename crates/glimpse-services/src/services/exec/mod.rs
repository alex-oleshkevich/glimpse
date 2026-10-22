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
use futures_util::{StreamExt, stream};
use serde_json::{Map, Value};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::{CommandError, Ctx, Input, Service, ServiceEndpoint, Sub};

pub use launcher::{Launcher, SystemdLauncher, UnitState};

pub use catalog::{Catalog, DesktopCatalog, Entry, expand};
pub use tree::{
    ActionRowProps, AddonProps, Align, BarsProps, BoxProps, ButtonProps, CalendarMarker,
    CalendarProps, CheckButtonProps, ChoiceItem, ChoiceListProps, ClassName, ContainerProps,
    DotsProps, DropDownProps, Element, ElementKind, Ellipsize, EndProps, EntryProps, EntryRowProps,
    ExpanderRowProps, FactItem, FactListProps, ImageProps, IndicatorProps, LabelProps,
    MeterRowProps, Node, NoticeProps, Paint, PlaceholderProps, PopoverProps, PopoverSize, ROOT,
    RangeBarProps, ReadoutProps, RevealerProps, RowProps, RowTone, RowTypography, ScaleProps,
    SectionProps, SeparatorProps, Severity, SliderRowProps, SparklineProps, SpinnerProps,
    StackProps, SwatchProps, SwatchSize, SwitchProps, ToggleButtonProps, ToggleControl,
    ToggleRowProps, Tone, Transition, Tree,
};
pub use wire::{
    Edge, FromApplet, HelloEntry, MAX_LINE, Op, Orientation, Outgoing, Placement, WireNode, Zone,
};

const MAX_UPDATES_PER_SECOND: u32 = 120;
const RESET_AFTER: Duration = Duration::from_secs(60);
const URI_CAP: usize = 2048;

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
    pub application_dirs: Vec<PathBuf>,
    pub state_base: Option<PathBuf>,
    pub runtime_base: Option<PathBuf>,
    pub panel: String,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            application_dirs: catalog::application_dirs(),
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
    fn socket_path(&self, slot: u64) -> Option<PathBuf> {
        Some(
            self.runtime_base
                .as_ref()?
                .join("applets")
                .join(format!("{}-{slot}.sock", std::process::id())),
        )
    }

    fn sockets_dir(&self) -> Option<PathBuf> {
        self.runtime_base.as_ref().map(|base| base.join("applets"))
    }
}

pub struct Dependencies {
    pub catalog: Arc<dyn Catalog>,
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
    Recheck { slot: u64, spawn: u64 },
    Relaunch { slot: u64, spawn: u64 },
    Retry,
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
    },
    Unit {
        slot: u64,
        spawn: u64,
        state: UnitState,
    },
    Exited {
        slot: u64,
        spawn: u64,
        reason: String,
        resolved: Option<Entry>,
        catalog_refused: bool,
    },
    ResetFailed {
        slot: u64,
        generation: u64,
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
    Restart {
        slot: u64,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    DevTree {
        handle: String,
        reply: oneshot::Sender<Result<Value, CommandError>>,
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

pub type ExecHandle = ServiceEndpoint<Exec>;

impl ServiceEndpoint<Exec> {
    fn submit(
        &self,
        build: impl FnOnce(oneshot::Sender<Result<(), CommandError>>) -> Command,
    ) -> Pending {
        let (reply, answer) = oneshot::channel();
        (self.command(build(reply)), answer)
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
        let sent = self.command(Command::DevAttach {
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
    pub fn restart(
        &self,
        slot: u64,
    ) -> impl Future<Output = Result<(), CommandError>> + Send + 'static + use<> {
        answered(self.submit(|reply| Command::Restart { slot, reply }))
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
    pub fn dev_tree(
        &self,
        handle: &str,
    ) -> impl Future<Output = Result<Value, CommandError>> + Send + 'static + use<> {
        let (reply, answer) = oneshot::channel();
        let sent = self.command(Command::DevTree {
            handle: handle.to_owned(),
            reply,
        });
        async move {
            sent?;
            answer.await.map_err(|_| {
                CommandError::Unavailable("`exec` stopped before answering".to_owned())
            })?
        }
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
    primary: Option<u64>,
    public: SlotState,
    placement: Placement,
    applet_id: String,
    env: BTreeMap<String, String>,
    entry: Option<Entry>,
    dev: Option<DevSpec>,
    catalog_refused: bool,
    launch_failed: bool,
    failed_at: Option<chrono::DateTime<Utc>>,
    options: Map<String, Value>,
    spawn: u64,
    unit: String,
    socket: Option<PathBuf>,
    restarts: u32,
    link: Option<Link>,
    stop: CancellationToken,
    popover_open: bool,
    popover_own: bool,
    last_failed_reason: Option<String>,
    catalog_misses: u32,
    missed_at: Option<chrono::DateTime<Utc>>,
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
    next_serial: u64,
    next_unit: u64,
    next_generation: u64,
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

    fn root(&self, id: u64) -> u64 {
        self.slots
            .get(&id)
            .and_then(|slot| slot.primary)
            .unwrap_or(id)
    }

    fn same_placement(a: &Placement, b: &Placement) -> bool {
        Placement {
            output: None,
            ..a.clone()
        } == Placement {
            output: None,
            ..b.clone()
        }
    }

    fn sync_aliases(&mut self) {
        let primaries: Vec<u64> = self
            .slots
            .values()
            .filter_map(|slot| slot.primary)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        for primary_id in primaries {
            let Some(primary) = self.slots.get(&primary_id) else {
                continue;
            };
            let public = SlotState {
                requests: primary
                    .public
                    .requests
                    .iter()
                    .filter(|(_, request)| matches!(request, BarRequest::ClosePopover))
                    .cloned()
                    .collect(),
                ..primary.public.clone()
            };
            let env = primary.env.clone();
            let options = primary.options.clone();
            let entry = primary.entry.clone();
            let applet_id = primary.applet_id.clone();
            for slot in self.slots.values_mut() {
                if slot.primary == Some(primary_id) {
                    slot.public = public.clone();
                    slot.env = env.clone();
                    slot.options = options.clone();
                    slot.entry = entry.clone();
                    slot.applet_id = applet_id.clone();
                }
            }
        }
        let roots: Vec<u64> = self
            .slots
            .iter()
            .filter(|(_, slot)| slot.primary.is_none())
            .map(|(&id, _)| id)
            .collect();
        for id in roots {
            let open = self.slots.iter().any(|(&other, slot)| {
                (other == id || slot.primary == Some(id)) && slot.popover_own
            });
            if let Some(slot) = self.slots.get_mut(&id)
                && slot.popover_open != open
            {
                slot.popover_open = open;
                if matches!(slot.public.status, Status::Running) {
                    Self::send(slot, Outgoing::Popover { open });
                }
            }
        }
    }

    fn regroup(&mut self, ctx: &Ctx<Self>, applet: &str) {
        let ids: Vec<u64> = self
            .slots
            .iter()
            .filter(|(_, slot)| slot.origin == Origin::Spawned && slot.public.applet == applet)
            .map(|(&id, _)| id)
            .collect();
        let mut roots: Vec<u64> = Vec::new();
        let spawned_roots: Vec<u64> = ids
            .iter()
            .copied()
            .filter(|id| self.slots[id].primary.is_none())
            .collect();
        for id in spawned_roots {
            let target = roots.iter().copied().find(|&root| {
                Self::same_placement(&self.slots[&root].placement, &self.slots[&id].placement)
            });
            let Some(target) = target else {
                roots.push(id);
                continue;
            };
            self.next_spawn += 1;
            let spawn = self.next_spawn;
            let Some(slot) = self.slots.get_mut(&id) else {
                continue;
            };
            tracing::info!(%applet, slot = id, primary = target, "applet slot merged into the instance process");
            if let Some(link) = slot.link.take() {
                link.stop.cancel();
            }
            slot.stop.cancel();
            slot.spawn = spawn;
            slot.primary = Some(target);
            let unit = std::mem::take(&mut slot.unit);
            let socket = slot.socket.clone();
            let launcher = Arc::clone(&self.dependencies.launcher);
            ctx.spawn_detached(move |_ctx| async move {
                launcher.stop(&unit).await;
                if let Some(socket) = socket {
                    let _ = tokio::fs::remove_file(&socket).await;
                }
            });
        }
        let mut started: Vec<u64> = Vec::new();
        for &id in &ids {
            let Some(old) = self.slots[&id].primary else {
                continue;
            };
            let root = self.root(old);
            if Self::same_placement(&self.slots[&root].placement, &self.slots[&id].placement) {
                if let Some(slot) = self.slots.get_mut(&id) {
                    slot.primary = Some(root);
                }
                continue;
            }
            let target = roots.iter().copied().find(|&other| {
                Self::same_placement(&self.slots[&other].placement, &self.slots[&id].placement)
            });
            if let Some(slot) = self.slots.get_mut(&id) {
                slot.primary = target;
            }
            if target.is_none() {
                roots.push(id);
                started.push(id);
            }
        }
        for id in started {
            self.restart_slot(id);
        }
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
        Slot {
            origin: Origin::Dev,
            primary: None,
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
            entry: Some(request.entry.clone()),
            dev: Some(DevSpec {
                entry: request.entry,
                stderr: request.stderr.map(Arc::new),
            }),
            catalog_refused: false,
            launch_failed: false,
            failed_at: None,
            options: request.options,
            spawn: self.next_spawn,
            unit,
            socket: self.config.socket_path(slot_id),
            restarts: 0,
            link: None,
            stop: CancellationToken::new(),
            popover_open: false,
            popover_own: false,
            last_failed_reason: None,
            catalog_misses: 0,
            missed_at: None,
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
            slot.entry = None;
            slot.last_failed_reason = None;
            self.restart_slot(id);
        } else {
            tracing::info!(applet = %slot.applet_id, slot = id, "dev applet detached");
            slot.stop.cancel();
            slot.public.status = Status::Ended;
        }
    }

    fn commit(&mut self) {
        self.sync_aliases();
        self.publish();
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
        let id = self.root(id);
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
            slot.launch_failed = false;
            slot.failed_at = None;
            slot.catalog_misses = 0;
            slot.missed_at = None;
            slot.resolving = false;
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

    fn request(&mut self, slot_id: u64, request: FromApplet) {
        let Some(slot) = self.slots.get_mut(&slot_id) else {
            return;
        };
        let serial = &mut self.next_serial;
        match request {
            FromApplet::OpenUri { uri } => {
                if !allowed_uri(&uri) {
                    tracing::warn!(applet = %slot.public.applet, "applet asked to open a refused uri");
                } else if is_file(&uri) {
                    slot.push_request(serial, BarRequest::OpenFile { uri });
                } else {
                    slot.push_request(serial, BarRequest::OpenUri(uri));
                }
            }
            FromApplet::Launch { id } => {
                if id.strip_suffix(".desktop").unwrap_or(&id).is_empty() {
                    tracing::warn!(applet = %slot.public.applet, "applet asked to launch an empty id");
                } else {
                    slot.push_request(serial, BarRequest::Launch(id));
                }
            }
            FromApplet::ClosePopover => slot.push_request(serial, BarRequest::ClosePopover),
            _ => {}
        }
    }
}

impl Slot {
    fn push_request(&mut self, serial: &mut u64, request: BarRequest) {
        *serial += 1;
        self.public.requests.push((*serial, request));
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
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = SubKey;

    fn initial_state(_: &Self::Config) -> Self::State {
        ExecState::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let mut subs = Vec::new();
        for (&slot_id, slot) in &self.slots {
            if slot.primary.is_some() {
                continue;
            }
            if slot.origin == Origin::Spawned {
                if matches!(slot.public.status, Status::Running)
                    && slot.catalog_misses == 1
                    && let Some(missed_at) = slot.missed_at
                {
                    subs.push(Sub::deadline(
                        SubKey::Recheck {
                            slot: slot_id,
                            spawn: slot.spawn,
                        },
                        missed_at + chrono::Duration::seconds(1),
                        Event::Retry,
                    ));
                }
                if matches!(slot.public.status, Status::Failed(_))
                    && slot.launch_failed
                    && self.config.applets.contains_key(&slot.public.applet)
                    && let Some(failed_at) = slot.failed_at
                {
                    subs.push(Sub::deadline(
                        SubKey::Relaunch {
                            slot: slot_id,
                            spawn: slot.spawn,
                        },
                        failed_at + chrono::Duration::seconds(5),
                        Event::Retry,
                    ));
                }
            }
            if matches!(
                slot.public.status,
                Status::Starting | Status::Running | Status::Restarting { .. }
            ) {
                let catalog = Arc::clone(&self.dependencies.catalog);
                let (entry, stderr) = match &slot.dev {
                    Some(dev) => (
                        Some(dev.entry.clone()),
                        dev.stderr
                            .as_ref()
                            .and_then(|fd| fd.as_fd().try_clone_to_owned().ok()),
                    ),
                    None => (slot.entry.clone(), None),
                };
                let args = spawn::SourceArgs {
                    slot: slot_id,
                    spawn: slot.spawn,
                    id: slot.applet_id.clone(),
                    env: slot.env.clone(),
                    state_base: self.config.state_base.clone(),
                    entry,
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
        if self.slots.values().any(|slot| {
            slot.origin == Origin::Spawned
                && matches!(slot.public.status, Status::Failed(_) | Status::Running)
        }) {
            let dirs = self.config.application_dirs.clone();
            subs.push(Sub::stream(SubKey::Retry, move |_ctx| async move {
                if let Some(user_dir) = dirs.first()
                    && let Err(error) = tokio::fs::create_dir_all(user_dir).await
                {
                    tracing::debug!(%error, dir = %user_dir.display(), "could not create applications directory");
                }
                let updates = Box::pin(glimpse_config::watch_dirs(dirs));
                stream::unfold((updates, false), |(mut updates, mut warned)| async move {
                    let mut changed = false;
                    loop {
                        let update = if changed {
                            match tokio::time::timeout(Duration::from_millis(300), updates.next()).await {
                                Ok(update) => update,
                                Err(_) => return Some((Event::Retry, (updates, warned))),
                            }
                        } else {
                            updates.next().await
                        };
                        match update {
                            Some(glimpse_config::Update::Changed(_) | glimpse_config::Update::Rearmed) => changed = true,
                            Some(glimpse_config::Update::Unavailable(reason)) => {
                                if !warned {
                                    tracing::warn!(reason, "applet entries are not being watched");
                                    warned = true;
                                }
                            }
                            None => return changed.then_some((Event::Retry, (updates, warned))),
                        }
                    }
                })
            }));
        }
        subs
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> Self {
        let prefix = launcher::sweep_prefix(&config.panel);
        if let Some(dir) = config.sockets_dir() {
            let _ = tokio::task::spawn_blocking(move || prune_dead_sockets(&dir)).await;
        }
        dependencies.launcher.sweep(&prefix).await;
        Self {
            state: ctx.publisher(),
            config,
            dependencies,
            slots: BTreeMap::new(),
            pending_dev: BTreeMap::new(),
            taken_dev: BTreeMap::new(),
            next_spawn: 0,
            next_serial: 0,
            next_unit: 0,
            next_generation: 0,
        }
    }

    async fn stop(self, _ctx: &Ctx<Self>) {
        for slot in self.slots.values() {
            if slot.primary.is_some() {
                continue;
            }
            self.dependencies.launcher.stop(&slot.unit).await;
            if let Some(socket) = &slot.socket {
                let _ = tokio::fs::remove_file(socket).await;
            }
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
                    self.commit();
                    let _ = reply.send(Ok(()));
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
                self.next_spawn += 1;
                let primary = if matches!(status, Status::Starting) {
                    self.slots
                        .iter()
                        .find(|(_, other)| {
                            other.primary.is_none()
                                && other.origin == Origin::Spawned
                                && other.public.applet == applet
                                && Self::same_placement(&other.placement, &placement)
                        })
                        .map(|(&id, _)| id)
                } else {
                    None
                };
                let public = match primary.and_then(|id| self.slots.get(&id)) {
                    Some(other) => SlotState {
                        requests: Vec::new(),
                        ..other.public.clone()
                    },
                    None => SlotState {
                        applet: applet.clone(),
                        generation: 0,
                        tree: Arc::new(Tree::default()),
                        status,
                        title: String::new(),
                        icon: None,
                        requests: Vec::new(),
                    },
                };
                if let Some(primary_id) = primary {
                    tracing::info!(%applet, slot, primary = primary_id, "applet slot shares the instance process");
                }
                let unit = if primary.is_some() {
                    String::new()
                } else {
                    self.next_unit(&applet_id, slot)
                };
                self.slots.insert(
                    slot,
                    Slot {
                        origin: Origin::Spawned,
                        primary,
                        public,
                        placement,
                        applet_id,
                        env,
                        entry: None,
                        dev: None,
                        catalog_refused: false,
                        launch_failed: false,
                        failed_at: None,
                        options,
                        spawn: self.next_spawn,
                        unit,
                        socket: self.config.socket_path(slot),
                        restarts: 0,
                        link: None,
                        stop: CancellationToken::new(),
                        popover_open: false,
                        popover_own: false,
                        last_failed_reason,
                        catalog_misses: 0,
                        missed_at: None,
                        resolving: false,
                        running_since: None,
                        reset_done: false,
                    },
                );
                self.commit();
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
                        && slot.primary.is_none()
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
                        slot.env = request.env;
                        slot.dev = Some(DevSpec {
                            entry: request.entry.clone(),
                            stderr: request.stderr.map(Arc::new),
                        });
                        slot.entry = Some(request.entry.clone());
                        slot.options = request.options;
                        slot.public.title = request.entry.name;
                        slot.public.icon = request.entry.icon;
                        slot.last_failed_reason = None;
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
                self.commit();
                let _ = reply.send(result);
            }
            Input::Command(Command::Restart { slot, reply }) => {
                let id = self.root(slot);
                let result = match self.slots.get_mut(&id) {
                    Some(state) if state.origin == Origin::Spawned => {
                        if matches!(state.public.status, Status::Failed(_) | Status::Ended) {
                            state.last_failed_reason = None;
                            self.restart_slot(id);
                        }
                        Ok(())
                    }
                    _ => Err(CommandError::InvalidArgument("unknown applet".to_owned())),
                };
                self.commit();
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
                self.commit();
                let _ = reply.send(result);
            }
            Input::Command(Command::DevTree { handle, reply }) => {
                let result = self
                    .taken_dev
                    .get(&handle)
                    .and_then(|slot| self.slots.get(slot))
                    .map(|slot| slot.public.tree.snapshot())
                    .ok_or_else(|| {
                        CommandError::InvalidArgument("applet is pending or unknown".to_owned())
                    });
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
                self.commit();
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
                let slot = self.root(slot);
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
                let mut changed = None;
                let result = if let Some(slot) = self.slots.get_mut(&slot) {
                    if slot.placement != placement {
                        changed = Some(slot.public.applet.clone());
                        slot.placement = placement.clone();
                        if matches!(slot.public.status, Status::Running) {
                            Self::send(slot, Outgoing::Placement { placement });
                        }
                    }
                    Ok(())
                } else {
                    Err(CommandError::InvalidArgument("unknown slot".to_owned()))
                };
                if let Some(applet) = changed {
                    self.regroup(ctx, &applet);
                }
                let _ = reply.send(result);
            }
            Input::Command(Command::Detach { slot, reply }) => {
                if let Some(state) = self.slots.remove(&slot) {
                    state.stop.cancel();
                    if state.primary.is_none() {
                        if state.origin == Origin::Dev {
                            self.taken_dev.remove(&state.applet_id);
                        }
                        let launcher = Arc::clone(&self.dependencies.launcher);
                        let unit = state.unit.clone();
                        let socket = state.socket.clone();
                        ctx.spawn_detached(move |_ctx| async move {
                            launcher.stop(&unit).await;
                            if let Some(socket) = socket {
                                let _ = tokio::fs::remove_file(&socket).await;
                            }
                        });
                        if let Some(&promoted) = self
                            .slots
                            .iter()
                            .find(|(_, other)| other.primary == Some(slot))
                            .map(|(id, _)| id)
                        {
                            for other in self.slots.values_mut() {
                                if other.primary == Some(slot) {
                                    other.primary = Some(promoted);
                                }
                            }
                            if let Some(slot) = self.slots.get_mut(&promoted) {
                                slot.primary = None;
                            }
                            self.restart_slot(promoted);
                        }
                    }
                }
                self.commit();
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::SendEvent { event, reply }) => {
                let root = self.root(event.slot);
                if let Some(slot) = self.slots.get_mut(&root)
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
                    slot.popover_own = open;
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
                    state.launch_failed = false;
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
                    Self::send(
                        state,
                        Outgoing::Hello {
                            v: 1,
                            name: state.public.applet.clone(),
                            state: state_dir,
                            options: state.options.clone(),
                            placement: state.placement.clone(),
                            entry: state.entry.as_ref().map(|entry| HelloEntry {
                                id: state.applet_id.clone(),
                                name: entry.name.clone(),
                                icon: entry.icon.clone(),
                            }),
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
                    self.request(slot, request);
                }
            }
            Input::Event(Event::Disconnected { slot, spawn }) => {
                if let Some(state) = self.slots.get_mut(&slot).filter(|s| s.spawn == spawn) {
                    state.link = None;
                    state.public.tree = Arc::new(Tree::default());
                    state.running_since = None;
                    state.public.status = if state.origin == Origin::Dev
                        || self.config.applets.contains_key(&state.public.applet)
                    {
                        Status::Restarting {
                            attempt: state.restarts,
                        }
                    } else {
                        Status::Failed("not configured".to_owned())
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
                            current.public.tree = Arc::new(Tree::default());
                            current.public.status = Status::Failed(result);
                        }
                        UnitState::Gone => {
                            current.link = None;
                            current.running_since = None;
                            current.public.tree = Arc::new(Tree::default());
                            if matches!(
                                current.public.status,
                                Status::Starting | Status::Running | Status::Restarting { .. }
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
                catalog_refused,
            }) => {
                if let Some(state) = self.slots.get_mut(&slot).filter(|s| s.spawn == spawn) {
                    state.link = None;
                    state.launch_failed = !catalog_refused;
                    state.catalog_refused = catalog_refused;
                    state.failed_at = Some(Utc::now());
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
                    if state.primary.is_some() {
                        continue;
                    }
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
                    if matches!(&resolved, Some(Err(_))) {
                        let now = Utc::now();
                        let missed_at = state.missed_at.get_or_insert(now);
                        state.catalog_misses = if now >= *missed_at + chrono::Duration::seconds(1) {
                            2
                        } else {
                            1
                        };
                    } else {
                        state.catalog_misses = 0;
                        state.missed_at = None;
                    }
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
                    state.launch_failed = false;
                    state.public.status = Status::Failed(reason.clone());
                } else if let Some(state) = self.slots.get(&slot)
                    && state.spawn == spawn
                    && matches!(state.public.status, Status::Failed(_))
                    && self.config.applets.contains_key(&state.public.applet)
                {
                    match resolved {
                        Some(Ok(entry))
                            if state.launch_failed
                                || state.catalog_refused
                                || state.entry.as_ref() != Some(&entry) =>
                        {
                            self.restart_slot(slot)
                        }
                        Some(Err(_)) => {
                            if let Some(state) = self.slots.get_mut(&slot) {
                                state.catalog_refused = true;
                                state.launch_failed = false;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Input::Config(config) => {
                let mut respawn = Vec::new();
                for (&id, slot) in &mut self.slots {
                    if slot.origin == Origin::Dev || slot.primary.is_some() {
                        continue;
                    }
                    match config.applets.get(&slot.public.applet) {
                        Some(applet) if applet.applet != slot.applet_id => {
                            slot.applet_id = applet.applet.clone();
                            slot.env = applet.env.clone();
                            slot.options = applet.options.clone();
                            respawn.push(id);
                        }
                        Some(applet)
                            if applet.env != slot.env || applet.options != slot.options =>
                        {
                            slot.env = applet.env.clone();
                            slot.options = applet.options.clone();
                            respawn.push(id);
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
        self.commit();
    }
}

#[cfg(test)]
mod tests;
