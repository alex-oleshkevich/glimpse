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
                self.spawned_drive(ctx, id, Busy::Ejecting, Action::Eject, reply, async move {
                    call::eject(&connection, &path).await
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
        if record.busy.is_some() {
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
        if record.busy.is_some() {
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

    fn any_mounted(&self) -> bool {
        self.blocks.values().any(|record| {
            record
                .filesystem
                .as_ref()
                .and_then(|fs| fs.mount_points.as_ref())
                .is_some_and(|points| !points.is_empty())
        })
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
        | Command::PowerOff { reply, .. } => {
            let _ = reply.send(refused);
        }
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
mod tests {
    use glimpse_dbus::Buses;
    use tokio_util::sync::CancellationToken;
    use zbus::zvariant::Value;

    use super::*;

    const DRIVE: &str = "/org/freedesktop/UDisks2/drives/Cruzer";
    const DATA: &str = "/org/freedesktop/UDisks2/block_devices/sdb1";
    const RESERVE: &str = "/org/freedesktop/UDisks2/block_devices/sdb2";
    const FIXED_DRIVE: &str = "/org/freedesktop/UDisks2/drives/nvme1";
    const FIXED_BLOCK: &str = "/org/freedesktop/UDisks2/block_devices/nvme0n1p3";

    async fn removable() -> (
        Removable,
        Ctx<Removable>,
        tokio::sync::watch::Receiver<RemovableState>,
        tokio::sync::watch::Receiver<crate::ServiceState>,
    ) {
        let cancel = CancellationToken::new();
        let (events, _inbox) = tokio::sync::mpsc::channel(8);
        let (state, state_rx) = tokio::sync::watch::channel(RemovableState::default());
        let (health, health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
        let ctx = Ctx::<Removable>::new(
            events,
            &cancel,
            state,
            health,
            Buses::unavailable("no bus in tests"),
        );
        let service = Removable::start(&ctx, Config::from(&glimpse_config::Config::default()), ())
            .await
            .expect("starts");
        (service, ctx, state_rx, health_rx)
    }

    fn properties(pairs: Vec<(&str, Value<'static>)>) -> Properties {
        pairs
            .into_iter()
            .map(|(key, value)| {
                (
                    key.to_owned(),
                    OwnedValue::try_from(value).expect("a plain value"),
                )
            })
            .collect()
    }

    fn owned_path(path: &'static str) -> Value<'static> {
        Value::ObjectPath(path.try_into().expect("a path"))
    }

    fn owned_mount_points(paths: &[&str]) -> Value<'static> {
        let entries: Vec<Vec<u8>> = paths
            .iter()
            .map(|path| {
                let mut raw = path.as_bytes().to_vec();
                raw.push(0);
                raw
            })
            .collect();
        Value::from(entries)
    }

    fn removable_drive_interfaces() -> Interfaces {
        HashMap::from([(
            udisks2::DRIVE1.to_owned(),
            properties(vec![
                ("Vendor", "SanDisk".into()),
                ("Model", "Cruzer".into()),
                ("Removable", true.into()),
                ("Ejectable", true.into()),
                ("CanPowerOff", true.into()),
                ("MediaAvailable", true.into()),
                ("Size", 8_000_000_000u64.into()),
            ]),
        )])
    }

    fn fixed_drive_interfaces() -> Interfaces {
        HashMap::from([(
            udisks2::DRIVE1.to_owned(),
            properties(vec![
                ("Vendor", "Samsung".into()),
                ("Model", "SSD 980 PRO".into()),
                ("Removable", false.into()),
                ("MediaRemovable", false.into()),
                ("ConnectionBus", "".into()),
            ]),
        )])
    }

    fn mounted_volume() -> Interfaces {
        HashMap::from([
            (
                udisks2::BLOCK1.to_owned(),
                properties(vec![
                    ("Drive", owned_path(DRIVE)),
                    ("IdLabel", "DATA".into()),
                    ("IdType", "vfat".into()),
                    ("Size", 4_000_000_000u64.into()),
                ]),
            ),
            (
                udisks2::FILESYSTEM1.to_owned(),
                properties(vec![(
                    "MountPoints",
                    owned_mount_points(&["/run/media/alex/DATA"]),
                )]),
            ),
        ])
    }

    fn unmounted_volume() -> Interfaces {
        HashMap::from([
            (
                udisks2::BLOCK1.to_owned(),
                properties(vec![
                    ("Drive", owned_path(DRIVE)),
                    ("IdLabel", "RESERVE".into()),
                    ("IdType", "exfat".into()),
                    ("Size", 4_000_000_000u64.into()),
                ]),
            ),
            (udisks2::FILESYSTEM1.to_owned(), properties(vec![])),
        ])
    }

    fn session() -> Objects {
        BTreeMap::from([
            (DRIVE.to_owned(), removable_drive_interfaces()),
            (DATA.to_owned(), mounted_volume()),
            (RESERVE.to_owned(), unmounted_volume()),
        ])
    }

    async fn enumerated(service: &mut Removable, ctx: &Ctx<Removable>, objects: Objects) {
        service
            .handle(ctx, Input::Event(Event::Enumerated(Box::new(objects))))
            .await;
    }

    #[tokio::test]
    async fn a_removable_drive_with_two_filesystem_blocks_publishes_one_drive_with_two_volumes() {
        let (mut service, ctx, state, _health) = removable().await;

        enumerated(&mut service, &ctx, session()).await;

        let published = state.borrow();
        assert_eq!(published.drives.len(), 1);
        assert_eq!(published.drives[0].id, DriveId(DRIVE.to_owned()));
        assert_eq!(published.drives[0].volumes.len(), 2);
    }

    #[tokio::test]
    async fn a_hint_system_block_does_not_appear() {
        let (mut service, ctx, state, _health) = removable().await;
        let mut objects = session();
        let block = objects.get_mut(DATA).expect("the block");
        block
            .get_mut(udisks2::BLOCK1)
            .expect("the block interface")
            .insert(
                "HintSystem".to_owned(),
                OwnedValue::try_from(Value::from(true)).expect("a flag"),
            );

        enumerated(&mut service, &ctx, objects).await;

        let published = state.borrow();
        assert_eq!(
            published.drives[0].volumes.len(),
            1,
            "a HintSystem block must not appear"
        );
    }

    #[tokio::test]
    async fn a_hint_ignore_block_does_not_appear() {
        let (mut service, ctx, state, _health) = removable().await;
        let mut objects = session();
        let block = objects.get_mut(DATA).expect("the block");
        block
            .get_mut(udisks2::BLOCK1)
            .expect("the block interface")
            .insert(
                "HintIgnore".to_owned(),
                OwnedValue::try_from(Value::from(true)).expect("a flag"),
            );

        enumerated(&mut service, &ctx, objects).await;

        let published = state.borrow();
        assert_eq!(
            published.drives[0].volumes.len(),
            1,
            "a HintIgnore block must not appear"
        );
    }

    #[tokio::test]
    async fn a_block_with_no_filesystem_interface_does_not_appear() {
        let (mut service, ctx, state, _health) = removable().await;
        let mut objects = session();
        objects.insert(
            "/org/freedesktop/UDisks2/block_devices/sdb3".to_owned(),
            HashMap::from([(
                udisks2::BLOCK1.to_owned(),
                properties(vec![
                    ("Drive", owned_path(DRIVE)),
                    ("IdLabel", "BARE".into()),
                ]),
            )]),
        );

        enumerated(&mut service, &ctx, objects).await;

        let published = state.borrow();
        assert_eq!(
            published.drives[0].volumes.len(),
            2,
            "a block with no Filesystem interface must not become a volume"
        );
    }

    #[tokio::test]
    async fn a_block_whose_drive_is_not_removable_does_not_appear() {
        let (mut service, ctx, state, _health) = removable().await;
        let objects = BTreeMap::from([
            (FIXED_DRIVE.to_owned(), fixed_drive_interfaces()),
            (
                FIXED_BLOCK.to_owned(),
                HashMap::from([
                    (
                        udisks2::BLOCK1.to_owned(),
                        properties(vec![
                            ("Drive", owned_path(FIXED_DRIVE)),
                            ("IdLabel", "".into()),
                            ("IdType", "exfat".into()),
                        ]),
                    ),
                    (udisks2::FILESYSTEM1.to_owned(), properties(vec![])),
                ]),
            ),
        ]);

        enumerated(&mut service, &ctx, objects).await;

        let published = state.borrow();
        assert!(
            published.drives.is_empty(),
            "a non-removable drive must not appear, nor its blocks"
        );
    }

    #[tokio::test]
    async fn automount_hint_without_removable_drive_is_hidden() {
        let (mut service, ctx, state, _health) = removable().await;
        let objects = BTreeMap::from([
            (FIXED_DRIVE.to_owned(), fixed_drive_interfaces()),
            (
                FIXED_BLOCK.to_owned(),
                HashMap::from([
                    (
                        udisks2::BLOCK1.to_owned(),
                        properties(vec![
                            ("Drive", owned_path(FIXED_DRIVE)),
                            ("HintAuto", true.into()),
                            ("IdType", "exfat".into()),
                        ]),
                    ),
                    (udisks2::FILESYSTEM1.to_owned(), properties(vec![])),
                ]),
            ),
        ]);

        enumerated(&mut service, &ctx, objects).await;

        let published = state.borrow();
        assert!(
            published.drives.is_empty(),
            "HintAuto alone must never be read as removability"
        );
    }

    #[tokio::test]
    async fn an_unmounted_volume_has_no_mount_and_a_mounted_one_carries_its_path() {
        let (mut service, ctx, state, _health) = removable().await;

        enumerated(&mut service, &ctx, session()).await;

        let published = state.borrow();
        let mounted = published
            .volume(&VolumeId(DATA.to_owned()))
            .expect("the mounted volume");
        assert_eq!(
            mounted.mount.as_ref().map(|mount| mount.at.as_path()),
            Some(std::path::Path::new("/run/media/alex/DATA"))
        );

        let unmounted = published
            .volume(&VolumeId(RESERVE.to_owned()))
            .expect("the unmounted volume");
        assert_eq!(unmounted.mount, None);
    }

    #[tokio::test]
    async fn busy_is_cleared_by_the_command_completing_and_never_by_the_property() {
        let (mut service, ctx, state, _health) = removable().await;
        enumerated(&mut service, &ctx, session()).await;
        service.blocks.get_mut(DATA).expect("the volume").busy = Some(Busy::Unmounting);
        service.publish();

        service
            .handle(
                &ctx,
                Input::Event(Event::PropertiesChanged {
                    path: DATA.to_owned(),
                    interface: udisks2::FILESYSTEM1.to_owned(),
                    changed: properties(vec![("MountPoints", owned_mount_points(&[]))]),
                }),
            )
            .await;
        assert_eq!(
            state
                .borrow()
                .volume(&VolumeId(DATA.to_owned()))
                .and_then(|volume| volume.busy),
            Some(Busy::Unmounting),
            "a backend that answers success without moving the property would strand the row"
        );

        service
            .handle(
                &ctx,
                Input::Event(Event::VolumeSettled {
                    id: VolumeId(DATA.to_owned()),
                    busy: Busy::Unmounting,
                    failure: None,
                }),
            )
            .await;
        assert_eq!(
            state
                .borrow()
                .volume(&VolumeId(DATA.to_owned()))
                .and_then(|volume| volume.busy),
            None
        );
    }

    #[tokio::test]
    async fn a_command_with_no_bus_is_refused_rather_than_queued() {
        let (mut service, ctx, _state, _health) = removable().await;
        enumerated(&mut service, &ctx, session()).await;

        let (reply, result) = oneshot::channel();
        service
            .handle(
                &ctx,
                Input::Command(Command::Mount {
                    id: VolumeId(DATA.to_owned()),
                    reply,
                }),
            )
            .await;

        assert!(matches!(
            result.await,
            Ok(Err(RemovableError::Service(CommandError::Unavailable(_))))
        ));
    }

    #[tokio::test]
    async fn a_lost_bus_degrades_and_keeps_the_last_state() {
        let (mut service, ctx, state, health) = removable().await;
        enumerated(&mut service, &ctx, session()).await;

        service
            .handle(&ctx, Input::Event(Event::NameOwner(None)))
            .await;

        assert_eq!(state.borrow().drives.len(), 1);
        assert!(matches!(
            &*health.borrow(),
            crate::ServiceState::Degraded { .. }
        ));
    }

    #[tokio::test]
    async fn regaining_the_name_restarts_the_object_sources_but_not_the_name_watch() {
        let (mut service, ctx, _state, _health) = removable().await;

        service
            .handle(
                &ctx,
                Input::Event(Event::NameOwner(Some(":1.7".to_owned()))),
            )
            .await;

        let declared = service.subscriptions();
        let keys: Vec<&Watch> = declared.iter().map(Sub::key).collect();
        assert_eq!(
            keys,
            vec![&Watch::NameOwner, &Watch::Objects(1), &Watch::Properties(1)]
        );
    }

    #[tokio::test]
    async fn a_partial_change_keeps_what_it_does_not_carry() {
        let (mut service, ctx, state, _health) = removable().await;
        enumerated(&mut service, &ctx, session()).await;

        service
            .handle(
                &ctx,
                Input::Event(Event::PropertiesChanged {
                    path: DATA.to_owned(),
                    interface: udisks2::BLOCK1.to_owned(),
                    changed: properties(vec![("Size", 5_000_000_000u64.into())]),
                }),
            )
            .await;

        let published = state.borrow();
        let volume = published
            .volume(&VolumeId(DATA.to_owned()))
            .expect("the volume");
        assert_eq!(
            volume.label, "DATA",
            "the label must survive an unrelated update"
        );
        assert_eq!(volume.size, 5_000_000_000);
    }

    #[tokio::test]
    async fn no_capacity_subscription_exists_with_nothing_mounted_and_one_appears_with_a_mount() {
        let (mut service, ctx, _state, _health) = removable().await;

        let objects = BTreeMap::from([
            (DRIVE.to_owned(), removable_drive_interfaces()),
            (RESERVE.to_owned(), unmounted_volume()),
        ]);
        enumerated(&mut service, &ctx, objects).await;
        assert!(
            !service
                .subscriptions()
                .iter()
                .any(|sub| matches!(sub.key(), Watch::Capacity)),
            "nothing is mounted, so no timer should run"
        );

        enumerated(&mut service, &ctx, session()).await;
        assert!(
            service
                .subscriptions()
                .iter()
                .any(|sub| matches!(sub.key(), Watch::Capacity)),
            "a mounted volume must declare the capacity source"
        );
    }

    #[tokio::test]
    async fn a_zero_capacity_interval_never_declares_the_subscription() {
        let (mut service, ctx, _state, _health) = removable().await;
        service.config.capacity_interval = None;

        enumerated(&mut service, &ctx, session()).await;

        assert!(
            !service
                .subscriptions()
                .iter()
                .any(|sub| matches!(sub.key(), Watch::Capacity)),
            "capacity_interval = 0 must turn the sampler off even with a mount present"
        );
    }

    #[tokio::test]
    async fn capacity_sampling_runs_off_the_handler_and_reaches_the_mounted_volume() {
        let cancel = CancellationToken::new();
        let (events, mut inbox) = tokio::sync::mpsc::channel(8);
        let (state, state_rx) = tokio::sync::watch::channel(RemovableState::default());
        let (health, _health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
        let ctx = Ctx::<Removable>::new(
            events,
            &cancel,
            state,
            health,
            Buses::unavailable("no bus in tests"),
        );
        let mut service =
            Removable::start(&ctx, Config::from(&glimpse_config::Config::default()), ())
                .await
                .expect("starts");

        let objects = BTreeMap::from([
            (DRIVE.to_owned(), removable_drive_interfaces()),
            (
                DATA.to_owned(),
                HashMap::from([
                    (
                        udisks2::BLOCK1.to_owned(),
                        properties(vec![
                            ("Drive", owned_path(DRIVE)),
                            ("IdLabel", "DATA".into()),
                        ]),
                    ),
                    (
                        udisks2::FILESYSTEM1.to_owned(),
                        properties(vec![(
                            "MountPoints",
                            owned_mount_points(&[std::env::temp_dir()
                                .to_str()
                                .expect("utf8 path")]),
                        )]),
                    ),
                ]),
            ),
        ]);
        enumerated(&mut service, &ctx, objects).await;

        service
            .handle(&ctx, Input::Event(Event::CapacityTick))
            .await;
        let delivered = tokio::time::timeout(std::time::Duration::from_secs(5), inbox.recv())
            .await
            .expect("the sample arrives")
            .expect("the channel stays open");
        service.handle(&ctx, delivered).await;

        let published = state_rx.borrow();
        let volume = published
            .volume(&VolumeId(DATA.to_owned()))
            .expect("the volume");
        let capacity = volume
            .mount
            .as_ref()
            .expect("a mount")
            .capacity
            .expect("statvfs succeeds against a real temp directory");
        assert!(capacity.total > 0);
    }

    #[tokio::test]
    async fn the_state_before_any_event_has_arrived_is_empty_and_the_service_is_not_degraded() {
        let (_service, _ctx, state, health) = removable().await;

        assert_eq!(*state.borrow(), RemovableState::default());
        assert!(!matches!(
            &*health.borrow(),
            crate::ServiceState::Degraded { .. }
        ));
    }

    #[test]
    fn a_drive_with_no_vendor_or_model_falls_back_to_its_serial_then_a_generic_name() {
        assert_eq!(
            drive_name(&DriveProperties {
                serial: Some("SN123".to_owned()),
                ..Default::default()
            }),
            "SN123"
        );
        assert_eq!(drive_name(&DriveProperties::default()), "Removable drive");
        assert_eq!(
            drive_name(&DriveProperties {
                vendor: Some("SanDisk".to_owned()),
                model: Some("Cruzer".to_owned()),
                ..Default::default()
            }),
            "SanDisk Cruzer"
        );
    }
}
