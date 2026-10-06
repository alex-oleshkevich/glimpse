mod call;
mod capacity;
mod failure;
mod source;

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use tokio::sync::oneshot;
use zbus::zvariant::OwnedValue;

use glimpse_dbus::udisks2::{
    self, BlockProperties, ConnectionBus, DriveProperties, FilesystemProperties, Media,
};

pub use failure::Failure;
use failure::{Action, classify};

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, Service, ServiceError},
    subscription::Sub,
};

pub(crate) type Properties = HashMap<String, OwnedValue>;
pub(crate) type Interfaces = HashMap<String, Properties>;
pub(crate) type Objects = BTreeMap<String, Interfaces>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DriveId(String);

impl DriveId {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VolumeId(String);

impl VolumeId {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Busy {
    Mounting,
    Unmounting,
    Ejecting,
    PoweringOff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encryption {
    None,
    Unlocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capacity {
    pub total: u64,
    pub available: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub at: PathBuf,
    pub capacity: Option<Capacity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub id: VolumeId,
    pub label: String,
    pub fs: Option<String>,
    pub size: u64,
    pub read_only: bool,
    pub encryption: Encryption,
    pub mount: Option<Mount>,
    pub busy: Option<Busy>,
    pub failure: Option<Failure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drive {
    pub id: DriveId,
    pub name: String,
    pub media: Media,
    pub size: u64,
    pub ejectable: bool,
    pub can_power_off: bool,
    pub media_available: bool,
    pub busy: Option<Busy>,
    pub failure: Option<Failure>,
    pub volumes: Vec<Volume>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemovableState {
    pub drives: Vec<Drive>,
}

impl RemovableState {
    pub fn drive(&self, id: &DriveId) -> Option<&Drive> {
        self.drives.iter().find(|drive| &drive.id == id)
    }

    pub fn volume(&self, id: &VolumeId) -> Option<&Volume> {
        self.drives
            .iter()
            .flat_map(|drive| &drive.volumes)
            .find(|volume| &volume.id == id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    capacity_interval: Option<std::time::Duration>,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            capacity_interval: (document.removable.capacity_interval > 0)
                .then(|| std::time::Duration::from_secs(document.removable.capacity_interval)),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Watch {
    NameOwner,
    Objects(u64),
    Properties(u64),
    Capacity,
}

type Reply = oneshot::Sender<Result<(), RemovableError>>;

#[derive(Debug, thiserror::Error)]
pub enum RemovableError {
    #[error("the removable device refused the command: {0:?}")]
    Failed(Failure),
    #[error(transparent)]
    Service(#[from] CommandError),
}

impl RemovableError {
    pub fn failure(&self) -> Option<Failure> {
        match self {
            Self::Failed(failure) => Some(*failure),
            Self::Service(_) => None,
        }
    }
}

pub enum Command {
    Mount { id: VolumeId, reply: Reply },
    Unmount { id: VolumeId, reply: Reply },
    Eject { id: DriveId, reply: Reply },
    PowerOff { id: DriveId, reply: Reply },
    Remove { id: DriveId, reply: Reply },
}

pub enum Event {
    Enumerated(Box<Objects>),
    NameOwner(Option<String>),
    InterfacesAdded {
        path: String,
        interfaces: Interfaces,
    },
    InterfacesRemoved {
        path: String,
        interfaces: Vec<String>,
    },
    PropertiesChanged {
        path: String,
        interface: String,
        changed: Properties,
    },
    VolumeSettled {
        id: VolumeId,
        busy: Busy,
        failure: Option<Failure>,
    },
    DriveSettled {
        id: DriveId,
        busy: Busy,
        failure: Option<Failure>,
    },
    CapacityTick,
    CapacitySampled(Vec<(VolumeId, Capacity)>),
    Unavailable(String),
}

#[derive(Debug, Default)]
struct BlockRecord {
    block: Option<BlockProperties>,
    filesystem: Option<FilesystemProperties>,
    capacity: Option<Capacity>,
    busy: Option<Busy>,
    failure: Option<Failure>,
}

#[derive(Debug, Default)]
struct DriveRecord {
    drive: Option<DriveProperties>,
    busy: Option<Busy>,
    failure: Option<Failure>,
}

pub struct Removable {
    state: Publisher<RemovableState>,
    config: Config,
    generation: u64,
    adopted: bool,
    drives: BTreeMap<String, DriveRecord>,
    blocks: BTreeMap<String, BlockRecord>,
}

#[derive(Clone)]
pub struct RemovableHandle(crate::ServiceEndpoint<Removable>);

impl RemovableHandle {
    pub fn snapshot(&self) -> RemovableState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<RemovableState> {
        self.0.subscribe()
    }

    pub fn health(&self) -> tokio::sync::watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    pub async fn mount(&self, id: VolumeId) -> Result<(), RemovableError> {
        self.call(|reply| Command::Mount { id, reply }).await
    }

    pub async fn unmount(&self, id: VolumeId) -> Result<(), RemovableError> {
        self.call(|reply| Command::Unmount { id, reply }).await
    }

    pub async fn eject(&self, id: DriveId) -> Result<(), RemovableError> {
        self.call(|reply| Command::Eject { id, reply }).await
    }

    pub async fn power_off(&self, id: DriveId) -> Result<(), RemovableError> {
        self.call(|reply| Command::PowerOff { id, reply }).await
    }

    pub async fn remove(&self, id: DriveId) -> Result<(), RemovableError> {
        self.call(|reply| Command::Remove { id, reply }).await
    }

    async fn call(&self, command: impl FnOnce(Reply) -> Command) -> Result<(), RemovableError> {
        let (reply, result) = oneshot::channel();
        self.0.command(command(reply))?;
        result.await.map_err(|_| {
            CommandError::Unavailable("removable stopped before completing the command".to_owned())
        })?
    }
}

fn settle(action: Action, outcome: zbus::Result<()>) -> Result<(), Failure> {
    let Err(error) = outcome else {
        return Ok(());
    };
    let settled = classify(action, &error);
    if let Err(failure) = settled {
        let (name, detail) = match &error {
            zbus::Error::MethodError(name, detail, _) => {
                (name.as_str(), detail.as_deref().unwrap_or_default())
            }
            _ => ("", ""),
        };
        tracing::warn!(
            ?action,
            ?failure,
            name,
            detail,
            "a removable-device command was refused"
        );
    }
    settled
}

impl Service for Removable {
    const NAME: &'static str = "removable";
    type Config = Config;
    type State = RemovableState;
    type Handle = RemovableHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Watch;

    fn from_endpoint(endpoint: crate::ServiceEndpoint<Self>) -> Self::Handle {
        RemovableHandle(endpoint)
    }

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let mut subs = vec![
            Sub::stream(Watch::NameOwner, source::name_owner),
            Sub::stream(Watch::Objects(self.generation), source::objects),
            Sub::stream(Watch::Properties(self.generation), source::properties),
        ];
        if let Some(period) = self.config.capacity_interval
            && self.any_mounted()
        {
            subs.push(Sub::interval(Watch::Capacity, period, |_ctx| async {
                Event::CapacityTick
            }));
        }
        subs
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        _: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            state: ctx.publisher(),
            config,
            generation: 0,
            adopted: false,
            drives: BTreeMap::new(),
            blocks: BTreeMap::new(),
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(command) => self.run(ctx, command).await,
            Input::Config(config) => self.config = config,
            Input::Event(Event::Enumerated(objects)) => {
                self.adopted = true;
                self.adopt(*objects);
                ctx.running();
                self.publish();
            }
            Input::Event(Event::NameOwner(owner)) => match owner {
                Some(_) => {
                    self.generation = self.generation.wrapping_add(1);
                    self.adopted = false;
                }
                None => {
                    ctx.degraded("org.freedesktop.UDisks2 left the bus");
                }
            },
            Input::Event(Event::InterfacesAdded { path, interfaces }) if self.adopted => {
                self.add(&path, interfaces);
                self.publish();
            }
            Input::Event(Event::InterfacesRemoved { path, interfaces }) if self.adopted => {
                self.remove(&path, &interfaces);
                self.publish();
            }
            Input::Event(Event::PropertiesChanged {
                path,
                interface,
                changed,
            }) if self.adopted => {
                self.change(&path, &interface, &changed);
                self.publish();
            }
            Input::Event(Event::VolumeSettled { id, busy, failure }) => {
                if let Some(record) = self.blocks.get_mut(id.as_str())
                    && record.busy == Some(busy)
                {
                    record.busy = None;
                    record.failure = failure;
                }
                self.publish();
            }
            Input::Event(Event::DriveSettled { id, busy, failure }) => {
                if let Some(record) = self.drives.get_mut(id.as_str())
                    && record.busy == Some(busy)
                {
                    record.busy = None;
                    record.failure = failure;
                }
                self.publish();
            }
            Input::Event(Event::CapacityTick) => {
                let mounts = self.mounted_paths();
                if !mounts.is_empty() {
                    ctx.spawn_detached(move |ctx| async move {
                        let sampled = tokio::task::spawn_blocking(move || {
                            mounts
                                .into_iter()
                                .filter_map(|(id, at)| {
                                    capacity::sample(&at).map(|capacity| (id, capacity))
                                })
                                .collect::<Vec<_>>()
                        })
                        .await
                        .unwrap_or_default();
                        let _ = ctx
                            .events()
                            .send(Input::Event(Event::CapacitySampled(sampled)))
                            .await;
                    });
                }
            }
            Input::Event(Event::CapacitySampled(sampled)) => {
                for (id, capacity) in sampled {
                    if let Some(record) = self.blocks.get_mut(id.as_str()) {
                        record.capacity = Some(capacity);
                    }
                }
                self.publish();
            }
            Input::Event(Event::Unavailable(reason)) => ctx.degraded(reason),
            Input::Event(
                Event::InterfacesAdded { .. }
                | Event::InterfacesRemoved { .. }
                | Event::PropertiesChanged { .. },
            ) => {}
        }
    }
}

impl Removable {
    async fn run(&mut self, ctx: &Ctx<Self>, command: Command) {
        let connection = match ctx.system_bus() {
            Ok(connection) => connection.clone(),
            Err(reason) => return reject(command, reason),
        };

        match command {
            Command::Mount { id, reply } => {
                let path = id.as_str().to_owned();
                self.spawned_volume(ctx, id, Busy::Mounting, Action::Mount, reply, async move {
                    call::mount(&connection, &path).await
                });
            }
            Command::Unmount { id, reply } => {
                let path = id.as_str().to_owned();
                self.spawned_volume(
                    ctx,
                    id,
                    Busy::Unmounting,
                    Action::Unmount,
                    reply,
                    async move { call::unmount(&connection, &path).await },
                );
            }
            Command::Eject { id, reply } => {
                let path = id.as_str().to_owned();
                let (mounted, locks) = (self.mounted_on(&path), self.unlocked_on(&path));
                self.spawned_drive(ctx, id, Busy::Ejecting, Action::Eject, reply, async move {
                    remove(mounted, locks, Finish::Eject, |step| {
                        step.run(&connection, &path)
                    })
                    .await
                });
            }
            Command::Remove { id, reply } => {
                let path = id.as_str().to_owned();
                let (mounted, locks) = (self.mounted_on(&path), self.unlocked_on(&path));
                let finish = self.finish_for(&path);
                let (busy, action) = match finish {
                    Finish::Eject => (Busy::Ejecting, Action::Eject),
                    Finish::PowerOff => (Busy::PoweringOff, Action::PowerOff),
                    Finish::Nothing => (Busy::Unmounting, Action::Unmount),
                };
                self.spawned_drive(ctx, id, busy, action, reply, async move {
                    remove(mounted, locks, finish, |step| step.run(&connection, &path)).await
                });
            }
            Command::PowerOff { id, reply } => {
                let path = id.as_str().to_owned();
                self.spawned_drive(
                    ctx,
                    id,
                    Busy::PoweringOff,
                    Action::PowerOff,
                    reply,
                    async move { call::power_off(&connection, &path).await },
                );
            }
        }
    }

    fn spawned_volume(
        &mut self,
        ctx: &Ctx<Self>,
        id: VolumeId,
        busy: Busy,
        action: Action,
        reply: Reply,
        work: impl Future<Output = zbus::Result<()>> + Send + 'static,
    ) {
        let Some(record) = self.blocks.get_mut(id.as_str()) else {
            let _ = reply.send(Err(CommandError::InvalidArgument(
                "there is no such volume".to_owned(),
            )
            .into()));
            return;
        };
        let drive_busy = record
            .block
            .as_ref()
            .and_then(|block| block.drive.as_deref())
            .and_then(|drive| self.drives.get(drive))
            .is_some_and(|drive| drive.busy.is_some());
        if record.busy.is_some() || drive_busy {
            let _ = reply.send(Err(CommandError::Unavailable(
                "that volume is already busy".to_owned(),
            )
            .into()));
            return;
        }
        record.busy = Some(busy);
        self.publish();

        ctx.spawn_detached(move |ctx| async move {
            let outcome = settle(action, work.await);
            let _ = ctx
                .events()
                .send(Input::Event(Event::VolumeSettled {
                    id,
                    busy,
                    failure: outcome.err(),
                }))
                .await;
            let _ = reply.send(outcome.map_err(RemovableError::Failed));
        });
    }

    fn spawned_drive(
        &mut self,
        ctx: &Ctx<Self>,
        id: DriveId,
        busy: Busy,
        action: Action,
        reply: Reply,
        work: impl Future<Output = zbus::Result<()>> + Send + 'static,
    ) {
        let Some(record) = self.drives.get_mut(id.as_str()) else {
            let _ = reply.send(Err(CommandError::InvalidArgument(
                "there is no such drive".to_owned(),
            )
            .into()));
            return;
        };
        let volume_busy = self.blocks.values().any(|block| {
            block.busy.is_some()
                && block.block.as_ref().and_then(|b| b.drive.as_deref()) == Some(id.as_str())
        });
        if record.busy.is_some() || volume_busy {
            let _ = reply.send(Err(CommandError::Unavailable(
                "that drive is already busy".to_owned(),
            )
            .into()));
            return;
        }
        record.busy = Some(busy);
        self.publish();

        ctx.spawn_detached(move |ctx| async move {
            let outcome = settle(action, work.await);
            let _ = ctx
                .events()
                .send(Input::Event(Event::DriveSettled {
                    id,
                    busy,
                    failure: outcome.err(),
                }))
                .await;
            let _ = reply.send(outcome.map_err(RemovableError::Failed));
        });
    }

    fn finish_for(&self, drive: &str) -> Finish {
        let properties = self
            .drives
            .get(drive)
            .and_then(|record| record.drive.as_ref());
        if properties.and_then(|drive| drive.ejectable) == Some(true) {
            Finish::Eject
        } else if properties.and_then(|drive| drive.can_power_off) == Some(true) {
            Finish::PowerOff
        } else {
            Finish::Nothing
        }
    }

    fn any_mounted(&self) -> bool {
        self.blocks.values().any(|record| {
            record
                .filesystem
                .as_ref()
                .and_then(|fs| fs.mount_points.as_ref())
                .is_some_and(|points| !points.is_empty())
        })
    }

    fn mounted_on(&self, drive: &str) -> Vec<String> {
        self.blocks
            .iter()
            .filter(|(_, record)| {
                record.block.as_ref().and_then(|b| b.drive.as_deref()) == Some(drive)
                    && record
                        .filesystem
                        .as_ref()
                        .and_then(|fs| fs.mount_points.as_ref())
                        .is_some_and(|points| !points.is_empty())
            })
            .map(|(path, _)| path.clone())
            .collect()
    }

    fn unlocked_on(&self, drive: &str) -> Vec<String> {
        self.blocks
            .values()
            .filter_map(|record| {
                let block = record.block.as_ref()?;
                (block.drive.as_deref() == Some(drive))
                    .then(|| block.crypto_backing_device.clone())
                    .flatten()
            })
            .collect()
    }

    fn mounted_paths(&self) -> Vec<(VolumeId, PathBuf)> {
        self.blocks
            .iter()
            .filter_map(|(path, record)| {
                let points = record.filesystem.as_ref()?.mount_points.as_ref()?;
                let at = points.first()?;
                Some((VolumeId(path.clone()), PathBuf::from(at)))
            })
            .collect()
    }

    fn adopt(&mut self, objects: Objects) {
        self.drives.clear();
        self.blocks.clear();
        for (path, interfaces) in objects {
            self.add(&path, interfaces);
        }
    }

    fn add(&mut self, path: &str, interfaces: Interfaces) {
        for (interface, properties) in interfaces {
            self.change(path, &interface, &properties);
        }
    }

    fn change(&mut self, path: &str, interface: &str, properties: &Properties) {
        match interface {
            udisks2::DRIVE1 => {
                let held = self.drives.entry(path.to_owned()).or_default();
                let decoded = udisks2::decode_drive(properties);
                merge_drive(
                    held.drive.get_or_insert_with(DriveProperties::default),
                    decoded,
                );
            }
            udisks2::BLOCK1 => {
                let held = self.blocks.entry(path.to_owned()).or_default();
                let decoded = udisks2::decode_block(properties);
                merge_block(
                    held.block.get_or_insert_with(BlockProperties::default),
                    decoded,
                );
            }
            udisks2::FILESYSTEM1 => {
                let held = self.blocks.entry(path.to_owned()).or_default();
                let decoded = udisks2::decode_filesystem(properties);
                let was_mounted = held
                    .filesystem
                    .as_ref()
                    .and_then(|fs| fs.mount_points.as_ref())
                    .is_some_and(|points| !points.is_empty());
                merge_filesystem(
                    held.filesystem
                        .get_or_insert_with(FilesystemProperties::default),
                    properties,
                    decoded,
                );
                let now_mounted = held
                    .filesystem
                    .as_ref()
                    .and_then(|fs| fs.mount_points.as_ref())
                    .is_some_and(|points| !points.is_empty());
                if was_mounted && !now_mounted {
                    held.capacity = None;
                }
            }
            _ => {}
        }
    }

    fn remove(&mut self, path: &str, interfaces: &[String]) {
        for interface in interfaces {
            match interface.as_str() {
                udisks2::DRIVE1 => {
                    self.drives.remove(path);
                }
                udisks2::BLOCK1 => {
                    self.blocks.remove(path);
                }
                udisks2::FILESYSTEM1 => {
                    if let Some(held) = self.blocks.get_mut(path) {
                        held.filesystem = None;
                        held.capacity = None;
                    }
                }
                _ => {}
            }
        }
    }

    fn publish(&mut self) {
        let drives: Vec<Drive> = self
            .drives
            .iter()
            .filter(|(_, record)| record.drive.as_ref().is_some_and(removable_drive))
            .map(|(path, record)| self.project_drive(path, record))
            .collect();

        self.state.set(RemovableState { drives });
    }

    fn project_drive(&self, path: &str, record: &DriveRecord) -> Drive {
        let properties = record.drive.clone().unwrap_or_default();
        let volumes: Vec<Volume> = self
            .blocks
            .iter()
            .filter(|(_, block)| {
                block.block.as_ref().and_then(|b| b.drive.as_deref()) == Some(path)
            })
            .filter_map(|(block_path, block)| self.project_volume(block_path, block))
            .collect();

        Drive {
            id: DriveId(path.to_owned()),
            name: drive_name(&properties),
            media: properties.media.unwrap_or_default(),
            size: properties.size.unwrap_or_default(),
            ejectable: properties.ejectable.unwrap_or_default(),
            can_power_off: properties.can_power_off.unwrap_or_default(),
            media_available: properties.media_available.unwrap_or_default(),
            busy: record.busy,
            failure: record.failure,
            volumes,
        }
    }

    fn project_volume(&self, path: &str, record: &BlockRecord) -> Option<Volume> {
        let block = record.block.as_ref()?;
        let filesystem = record.filesystem.as_ref()?;
        if block.hint_ignore == Some(true) || block.hint_system == Some(true) {
            return None;
        }

        let mount = filesystem
            .mount_points
            .as_ref()
            .and_then(|points| points.first())
            .map(|at| Mount {
                at: PathBuf::from(at),
                capacity: record.capacity,
            });

        Some(Volume {
            id: VolumeId(path.to_owned()),
            label: block.id_label.clone().unwrap_or_default(),
            fs: block.id_type.clone(),
            size: block.size.unwrap_or_default(),
            read_only: block.read_only.unwrap_or_default(),
            encryption: if block.crypto_backing_device.is_some() {
                Encryption::Unlocked
            } else {
                Encryption::None
            },
            mount,
            busy: record.busy,
            failure: record.failure,
        })
    }
}

fn reject(command: Command, reason: &str) {
    let refused: Result<(), RemovableError> =
        Err(CommandError::Unavailable(reason.to_owned()).into());
    match command {
        Command::Mount { reply, .. }
        | Command::Unmount { reply, .. }
        | Command::Eject { reply, .. }
        | Command::PowerOff { reply, .. }
        | Command::Remove { reply, .. } => {
            let _ = reply.send(refused);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Finish {
    Eject,
    PowerOff,
    Nothing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Unmount(String),
    Lock(String),
    Eject,
    PowerOff,
}

impl Step {
    async fn run(self, connection: &zbus::Connection, drive: &str) -> zbus::Result<()> {
        match self {
            Self::Unmount(volume) => call::unmount(connection, &volume).await,
            Self::Lock(backing) => call::lock(connection, &backing).await,
            Self::Eject => call::eject(connection, drive).await,
            Self::PowerOff => call::power_off(connection, drive).await,
        }
    }
}

async fn remove<F, Fut>(
    volumes: Vec<String>,
    locks: Vec<String>,
    finish: Finish,
    mut step: F,
) -> zbus::Result<()>
where
    F: FnMut(Step) -> Fut,
    Fut: Future<Output = zbus::Result<()>>,
{
    for volume in volumes {
        if let Err(error) = step(Step::Unmount(volume)).await
            && classify(Action::Unmount, &error).is_err()
        {
            return Err(error);
        }
    }
    for backing in locks {
        step(Step::Lock(backing)).await?;
    }
    match finish {
        Finish::Eject => step(Step::Eject).await,
        Finish::PowerOff => step(Step::PowerOff).await,
        Finish::Nothing => Ok(()),
    }
}

fn removable_drive(properties: &DriveProperties) -> bool {
    properties.removable == Some(true)
        || properties.media_removable == Some(true)
        || matches!(
            properties.connection_bus,
            Some(ConnectionBus::Usb) | Some(ConnectionBus::Sdio)
        )
}

fn drive_name(properties: &DriveProperties) -> String {
    let vendor = properties.vendor.as_deref().unwrap_or_default().trim();
    let model = properties.model.as_deref().unwrap_or_default().trim();
    let combined: Vec<&str> = [vendor, model]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect();
    if !combined.is_empty() {
        return combined.join(" ");
    }
    properties
        .serial
        .as_deref()
        .map(str::trim)
        .filter(|serial| !serial.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| "Removable drive".to_owned())
}

fn keep<T>(held: &mut Option<T>, from: Option<T>) {
    if from.is_some() {
        *held = from;
    }
}

fn merge_drive(held: &mut DriveProperties, from: DriveProperties) {
    keep(&mut held.vendor, from.vendor);
    keep(&mut held.model, from.model);
    keep(&mut held.serial, from.serial);
    keep(&mut held.revision, from.revision);
    keep(&mut held.size, from.size);
    keep(&mut held.media, from.media);
    keep(&mut held.media_available, from.media_available);
    keep(&mut held.media_removable, from.media_removable);
    keep(&mut held.removable, from.removable);
    keep(&mut held.ejectable, from.ejectable);
    keep(&mut held.can_power_off, from.can_power_off);
    keep(&mut held.connection_bus, from.connection_bus);
    keep(&mut held.optical, from.optical);
    keep(&mut held.sort_key, from.sort_key);
}

fn merge_block(held: &mut BlockProperties, from: BlockProperties) {
    keep(&mut held.device, from.device);
    keep(&mut held.preferred_device, from.preferred_device);
    keep(&mut held.drive, from.drive);
    keep(&mut held.crypto_backing_device, from.crypto_backing_device);
    keep(&mut held.id_label, from.id_label);
    keep(&mut held.id_type, from.id_type);
    keep(&mut held.id_uuid, from.id_uuid);
    keep(&mut held.id_usage, from.id_usage);
    keep(&mut held.size, from.size);
    keep(&mut held.read_only, from.read_only);
    keep(&mut held.hint_auto, from.hint_auto);
    keep(&mut held.hint_ignore, from.hint_ignore);
    keep(&mut held.hint_system, from.hint_system);
    keep(&mut held.hint_name, from.hint_name);
    keep(&mut held.hint_icon_name, from.hint_icon_name);
    keep(
        &mut held.hint_symbolic_icon_name,
        from.hint_symbolic_icon_name,
    );
}

fn merge_filesystem(held: &mut FilesystemProperties, raw: &Properties, from: FilesystemProperties) {
    if raw.contains_key("MountPoints") {
        held.mount_points = from.mount_points;
    }
    keep(&mut held.size, from.size);
}

#[cfg(test)]
mod tests;
