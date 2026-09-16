mod agent;
mod call;
mod failure;
mod source;

use std::collections::{BTreeMap, HashMap};

use tokio::sync::oneshot;
use zbus::zvariant::OwnedValue;

use glimpse_dbus::bluez::{
    self, AdapterProperties, Codec, DeviceIcon, DeviceProperties, DisconnectReason, Power, Profile,
    TransportProperties,
};

pub use agent::Answer;
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
pub struct DeviceId(String);

impl DeviceId {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapter {
    pub alias: String,
    pub power: Power,
    pub discovering: bool,
    pub discoverable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Busy {
    Connecting,
    Disconnecting,
    Pairing,
    Forgetting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: DeviceId,
    pub address: String,
    pub name: String,
    pub icon: DeviceIcon,
    pub paired: bool,
    pub bonded: bool,
    pub trusted: bool,
    pub blocked: bool,
    pub connected: bool,
    pub battery: Option<u8>,
    pub codec: Option<Codec>,
    pub rssi: Option<i16>,
    pub profiles: Vec<Profile>,
    pub busy: Option<Busy>,
    pub failure: Option<Failure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prompt {
    Confirm {
        device: DeviceId,
        passkey: u32,
    },
    Authorize(DeviceId),
    RequestPin(DeviceId),
    RequestPasskey(DeviceId),
    DisplayPin {
        device: DeviceId,
        pin: String,
    },
    DisplayPasskey {
        device: DeviceId,
        passkey: u32,
        entered: u16,
    },
}

impl Prompt {
    pub fn device(&self) -> &DeviceId {
        match self {
            Prompt::Confirm { device, .. }
            | Prompt::Authorize(device)
            | Prompt::RequestPin(device)
            | Prompt::RequestPasskey(device)
            | Prompt::DisplayPin { device, .. }
            | Prompt::DisplayPasskey { device, .. } => device,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirmation {
    Forget { device: DeviceId, connected: bool },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BluetoothState {
    pub adapter: Option<Adapter>,
    pub devices: Vec<Device>,
    pub scanning: bool,
    pub pairing: Option<Prompt>,
    pub confirm: Option<Confirmation>,
}

impl Device {
    pub fn known(&self) -> bool {
        self.paired || self.bonded || self.connected || self.trusted
    }
}

impl BluetoothState {
    pub fn device(&self, id: &DeviceId) -> Option<&Device> {
        self.devices.iter().find(|device| &device.id == id)
    }

    pub fn name(&self, id: &DeviceId) -> Option<&str> {
        self.device(id).map(|device| device.name.as_str())
    }

    pub fn connected(&self) -> impl Iterator<Item = &Device> {
        self.devices.iter().filter(|device| device.connected)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    scan_timeout: Option<std::time::Duration>,
    hide_unnamed: bool,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            scan_timeout: (document.bluetooth.scan_timeout > 0)
                .then(|| std::time::Duration::from_secs(document.bluetooth.scan_timeout)),
            hide_unnamed: document.bluetooth.hide_unnamed,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Watch {
    NameOwner,
    Objects(u64),
    Properties(u64),
    Disconnects(u64),
    ScanDeadline(u64),
}

type Reply = oneshot::Sender<Result<(), BluetoothError>>;

#[derive(Debug, thiserror::Error)]
pub enum BluetoothError {
    #[error("bluetooth refused the command: {0:?}")]
    Failed(Failure),
    #[error(transparent)]
    Service(#[from] CommandError),
}

impl BluetoothError {
    pub fn failure(&self) -> Option<Failure> {
        match self {
            Self::Failed(failure) => Some(*failure),
            Self::Service(_) => None,
        }
    }
}

pub enum Command {
    SetPowered {
        powered: bool,
        reply: Reply,
    },
    SetDiscoverable {
        on: bool,
        reply: Reply,
    },
    StartScan {
        reply: Reply,
    },
    StopScan {
        reply: Reply,
    },
    Connect {
        id: DeviceId,
        reply: Reply,
    },
    Disconnect {
        id: DeviceId,
        reply: Reply,
    },
    Pair {
        id: DeviceId,
        reply: Reply,
    },
    CancelPairing {
        id: DeviceId,
        reply: Reply,
    },
    SetTrusted {
        id: DeviceId,
        trusted: bool,
        reply: Reply,
    },
    Forget {
        id: DeviceId,
        confirmed: bool,
        reply: Reply,
    },
    DismissConfirmation {
        reply: Reply,
    },
    AnswerPairing {
        answer: Answer,
        reply: Reply,
    },
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
        invalidated: Vec<String>,
    },
    Settled {
        id: DeviceId,
        busy: Busy,
        failure: Option<Failure>,
    },
    Disconnected {
        path: String,
        reason: DisconnectReason,
    },
    Prompt(Prompt, Option<oneshot::Sender<Answer>>),
    PromptGone,
    AgentReleased,
    AgentRegistered(bool),
    ScanExpired(u64),
    Unavailable(String),
}

#[derive(Debug, Default)]
struct Record {
    properties: DeviceProperties,
    battery: Option<u8>,
    busy: Option<Busy>,
    failure: Option<Failure>,
}

pub struct Bluetooth {
    state: Publisher<BluetoothState>,
    config: Config,
    generation: u64,
    adopted: bool,
    scan: Option<u64>,
    scans: u64,
    deadline: Option<chrono::DateTime<chrono::Utc>>,
    agent: bool,
    prompt: Option<(Prompt, Option<oneshot::Sender<Answer>>)>,
    confirm: Option<Confirmation>,
    adapters: BTreeMap<String, AdapterProperties>,
    devices: BTreeMap<String, Record>,
    transports: BTreeMap<String, TransportProperties>,
}

#[derive(Clone)]
pub struct BluetoothHandle(crate::ServiceEndpoint<Bluetooth>);

impl BluetoothHandle {
    pub fn snapshot(&self) -> BluetoothState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<BluetoothState> {
        self.0.subscribe()
    }

    pub fn health(&self) -> tokio::sync::watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    pub async fn set_powered(&self, powered: bool) -> Result<(), BluetoothError> {
        self.call(|reply| Command::SetPowered { powered, reply })
            .await
    }

    pub async fn start_scan(&self) -> Result<(), BluetoothError> {
        self.call(|reply| Command::StartScan { reply }).await
    }

    pub async fn set_discoverable(&self, on: bool) -> Result<(), BluetoothError> {
        self.call(|reply| Command::SetDiscoverable { on, reply })
            .await
    }

    pub async fn stop_scan(&self) -> Result<(), BluetoothError> {
        self.call(|reply| Command::StopScan { reply }).await
    }

    pub async fn connect(&self, id: DeviceId) -> Result<(), BluetoothError> {
        self.call(|reply| Command::Connect { id, reply }).await
    }

    pub async fn disconnect(&self, id: DeviceId) -> Result<(), BluetoothError> {
        self.call(|reply| Command::Disconnect { id, reply }).await
    }

    pub async fn pair(&self, id: DeviceId) -> Result<(), BluetoothError> {
        self.call(|reply| Command::Pair { id, reply }).await
    }

    pub async fn cancel_pairing(&self, id: DeviceId) -> Result<(), BluetoothError> {
        self.call(|reply| Command::CancelPairing { id, reply })
            .await
    }

    pub async fn set_trusted(&self, id: DeviceId, trusted: bool) -> Result<(), BluetoothError> {
        self.call(|reply| Command::SetTrusted { id, trusted, reply })
            .await
    }

    pub async fn forget(&self, id: DeviceId, confirmed: bool) -> Result<(), BluetoothError> {
        self.call(|reply| Command::Forget {
            id,
            confirmed,
            reply,
        })
        .await
    }

    pub async fn dismiss_confirmation(&self) -> Result<(), BluetoothError> {
        self.call(|reply| Command::DismissConfirmation { reply })
            .await
    }

    pub async fn answer_pairing(&self, answer: Answer) -> Result<(), BluetoothError> {
        self.call(|reply| Command::AnswerPairing { answer, reply })
            .await
    }

    async fn call(&self, command: impl FnOnce(Reply) -> Command) -> Result<(), BluetoothError> {
        let (reply, result) = oneshot::channel();
        self.0.command(command(reply))?;
        result.await.map_err(|_| {
            CommandError::Unavailable("bluetooth stopped before completing the command".to_owned())
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
            "bluetooth refused a command"
        );
    }
    settled
}

impl Service for Bluetooth {
    const NAME: &'static str = "bluetooth";
    type Config = Config;
    type State = BluetoothState;
    type Handle = BluetoothHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Watch;

    fn from_endpoint(endpoint: crate::ServiceEndpoint<Self>) -> Self::Handle {
        BluetoothHandle(endpoint)
    }

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let mut subs = vec![
            Sub::stream(Watch::NameOwner, source::name_owner),
            Sub::stream(Watch::Objects(self.generation), source::objects),
            Sub::stream(Watch::Properties(self.generation), source::properties),
            Sub::stream(Watch::Disconnects(self.generation), source::disconnects),
        ];
        if let (Some(scan), Some(at)) = (self.scan, self.deadline) {
            subs.push(Sub::deadline(
                Watch::ScanDeadline(scan),
                at,
                Event::ScanExpired(scan),
            ));
        }
        subs
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        _: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        let service = Self {
            state: ctx.publisher(),
            config,
            generation: 0,
            adopted: false,
            scan: None,
            scans: 0,
            deadline: None,
            agent: false,
            prompt: None,
            confirm: None,
            adapters: BTreeMap::new(),
            devices: BTreeMap::new(),
            transports: BTreeMap::new(),
        };

        service.reregister(ctx);

        Ok(service)
    }

    async fn stop(self, ctx: &Ctx<Self>) {
        let Ok(connection) = ctx.system_bus() else {
            return;
        };
        if let (Some(adapter), true) = (self.adapter_path(), self.scan.is_some()) {
            let _ = call::stop_scan(connection, adapter).await;
        }
        if self.agent {
            agent::unregister(connection).await;
        }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(command) => self.run(ctx, command).await,
            Input::Config(config) => {
                self.config = config;
                self.publish();
            }
            Input::Event(Event::Enumerated(objects)) => {
                self.adopted = true;
                self.adopt(*objects);
                ctx.running();
                self.settle_scan(ctx);
                self.publish();
            }
            Input::Event(Event::NameOwner(owner)) => match owner {
                Some(_) => {
                    self.generation = self.generation.wrapping_add(1);
                    self.adopted = false;
                    self.agent = false;
                    self.reregister(ctx);
                }
                None => {
                    self.agent = false;
                    self.prompt = None;
                    self.confirm = None;
                    self.publish();
                    ctx.degraded("org.bluez left the bus");
                }
            },
            Input::Event(Event::AgentRegistered(registered)) => self.agent = registered,
            Input::Event(Event::AgentReleased) => {
                self.agent = false;
                self.prompt = None;
                if let Ok(connection) = ctx.system_bus().cloned() {
                    ctx.spawn_detached(move |_ctx| async move {
                        agent::withdraw(&connection).await;
                    });
                }
                self.publish();
            }
            Input::Event(Event::Prompt(prompt, answer)) => {
                self.prompt = Some((prompt, answer));
                self.publish();
            }
            Input::Event(Event::PromptGone) => {
                self.prompt = None;
                self.publish();
            }
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
                invalidated,
            }) if self.adopted => {
                self.change(&path, &interface, &changed);
                self.invalidate(&path, &interface, &invalidated);
                self.settle_scan(ctx);
                self.publish();
            }
            Input::Event(Event::Settled { id, busy, failure }) => {
                let mut answered = false;
                if let Some(record) = self.devices.get_mut(id.as_str())
                    && record.busy == Some(busy)
                {
                    record.busy = None;
                    record.failure = failure;
                    answered = true;
                }
                if answered && busy == Busy::Pairing && failure.is_none() {
                    self.trust_then_connect(ctx, &id);
                }
                self.publish();
            }
            Input::Event(Event::Disconnected { path, reason }) => {
                if let Some(record) = self.devices.get_mut(&path) {
                    record.failure = failure::dropped(reason);
                }
                self.publish();
            }
            Input::Event(Event::ScanExpired(generation)) => {
                if self.scan == Some(generation) {
                    self.halt(ctx);
                }
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

impl Bluetooth {
    async fn run(&mut self, ctx: &Ctx<Self>, command: Command) {
        match command {
            Command::AnswerPairing { answer, reply } => self.answer(ctx, answer, reply).await,
            Command::DismissConfirmation { reply } => {
                self.confirm = None;
                self.publish();
                let _ = reply.send(Ok(()));
            }
            Command::Forget {
                id,
                confirmed: false,
                reply,
            } => {
                let connected = self
                    .devices
                    .get(id.as_str())
                    .is_some_and(|record| record.properties.connected == Some(true));
                self.confirm = Some(Confirmation::Forget {
                    device: id,
                    connected,
                });
                self.publish();
                let _ = reply.send(Ok(()));
            }
            Command::Pair { id, reply } if !self.agent => {
                if let Some(record) = self.devices.get_mut(id.as_str()) {
                    record.failure = Some(Failure::NoAgent);
                }
                self.publish();
                let _ = reply.send(Err(BluetoothError::Failed(Failure::NoAgent)));
            }
            command => {
                if matches!(command, Command::Forget { .. } | Command::SetTrusted { .. }) {
                    self.settled();
                }
                self.dispatch(ctx, command).await;
            }
        }
    }

    async fn dispatch(&mut self, ctx: &Ctx<Self>, command: Command) {
        let connection = match ctx.system_bus() {
            Ok(connection) => connection.clone(),
            Err(reason) => return reject(command, reason),
        };
        let Some(adapter) = self.adapter_path().map(str::to_owned) else {
            return reject(command, "there is no bluetooth adapter");
        };

        match command {
            Command::SetPowered { powered, reply } => {
                detached(ctx, Action::Power, reply, async move {
                    call::set_powered(&connection, &adapter, powered).await
                });
            }
            Command::SetDiscoverable { on, reply } => {
                detached(ctx, Action::Discoverable, reply, async move {
                    call::set_discoverable(&connection, &adapter, on).await
                });
            }
            Command::StartScan { reply } => {
                self.begin_scan();
                let generation = self.scans;
                self.publish();
                ctx.spawn_detached(move |ctx| async move {
                    let outcome =
                        settle(Action::Scan, call::start_scan(&connection, &adapter).await);
                    if outcome.is_err() {
                        let _ = ctx
                            .events()
                            .send(Input::Event(Event::ScanExpired(generation)))
                            .await;
                    }
                    let _ = reply.send(outcome.map_err(BluetoothError::Failed));
                });
            }
            Command::StopScan { reply } => {
                self.scan = None;
                self.deadline = None;
                self.publish();
                detached(ctx, Action::Scan, reply, async move {
                    call::stop_scan(&connection, &adapter).await
                });
            }
            Command::SetTrusted {
                id, trusted, reply, ..
            } => {
                let path = id.as_str().to_owned();
                detached(ctx, Action::Trust, reply, async move {
                    call::set_trusted(&connection, &path, trusted).await
                });
            }
            Command::CancelPairing { id, reply } => {
                let path = id.as_str().to_owned();
                detached(ctx, Action::CancelPairing, reply, async move {
                    call::cancel_pairing(&connection, &path).await
                });
            }
            Command::Connect { id, reply } => {
                let path = id.as_str().to_owned();
                self.spawned(
                    ctx,
                    id,
                    Busy::Connecting,
                    Action::Connect,
                    reply,
                    async move { call::connect(&connection, &path).await },
                );
            }
            Command::Disconnect { id, reply } => {
                let path = id.as_str().to_owned();
                self.spawned(
                    ctx,
                    id,
                    Busy::Disconnecting,
                    Action::Disconnect,
                    reply,
                    async move { call::disconnect(&connection, &path).await },
                );
            }
            Command::Pair { id, reply } => {
                let path = id.as_str().to_owned();
                self.spawned(ctx, id, Busy::Pairing, Action::Pair, reply, async move {
                    call::pair(&connection, &path).await
                });
            }
            Command::Forget { id, reply, .. } => {
                let path = id.as_str().to_owned();
                self.spawned(
                    ctx,
                    id,
                    Busy::Forgetting,
                    Action::Forget,
                    reply,
                    async move { call::forget(&connection, &adapter, &path).await },
                );
            }
            Command::AnswerPairing { reply, .. } | Command::DismissConfirmation { reply } => {
                let _ = reply.send(Err(CommandError::Internal(
                    "that command is answered locally and never reaches the bus".to_owned(),
                )
                .into()));
            }
        }
    }

    async fn answer(&mut self, ctx: &Ctx<Self>, answer: Answer, reply: Reply) {
        let Some((prompt, sender)) = self.prompt.take() else {
            let _ = reply.send(Err(CommandError::Unavailable(
                "nothing is waiting for an answer".to_owned(),
            )
            .into()));
            return;
        };
        self.publish();

        if let Some(sender) = sender {
            let _ = sender.send(answer);
            let _ = reply.send(Ok(()));
            return;
        }
        if answer == Answer::Confirm {
            let _ = reply.send(Ok(()));
            return;
        }

        let Ok(connection) = ctx.system_bus().cloned() else {
            let _ = reply.send(Err(
                CommandError::Unavailable("no system bus".to_owned()).into()
            ));
            return;
        };
        let path = prompt.device().as_str().to_owned();
        detached(ctx, Action::CancelPairing, reply, async move {
            call::cancel_pairing(&connection, &path).await
        });
    }

    fn spawned(
        &mut self,
        ctx: &Ctx<Self>,
        id: DeviceId,
        busy: Busy,
        action: Action,
        reply: Reply,
        work: impl Future<Output = zbus::Result<()>> + Send + 'static,
    ) {
        let Some(record) = self.devices.get_mut(id.as_str()) else {
            let _ = reply.send(Err(CommandError::InvalidArgument(
                "there is no such device".to_owned(),
            )
            .into()));
            return;
        };
        if record.busy.is_some() {
            let _ = reply.send(Err(CommandError::Unavailable(
                "that device is already busy".to_owned(),
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
                .send(Input::Event(Event::Settled {
                    id,
                    busy,
                    failure: outcome.err(),
                }))
                .await;
            let _ = reply.send(outcome.map_err(BluetoothError::Failed));
        });
    }

    fn settled(&mut self) {
        if self.confirm.take().is_some() {
            self.publish();
        }
    }

    fn reregister(&self, ctx: &Ctx<Self>) {
        let Ok(connection) = ctx.system_bus().cloned() else {
            return;
        };
        let events = ctx.events();
        ctx.spawn_detached(move |ctx| async move {
            let registered = agent::register(&connection, events).await;
            if let Err(error) = &registered {
                tracing::warn!(%error, "could not register the bluetooth pairing agent");
            }
            let _ = ctx
                .events()
                .send(Input::Event(Event::AgentRegistered(registered.is_ok())))
                .await;
        });
    }

    fn trust_then_connect(&self, ctx: &Ctx<Self>, id: &DeviceId) {
        let Ok(connection) = ctx.system_bus().cloned() else {
            return;
        };
        let path = id.as_str().to_owned();
        let device = id.clone();
        let events = ctx.events();
        ctx.spawn_detached(move |_ctx| async move {
            if let Err(error) = call::set_trusted(&connection, &path, true).await {
                tracing::warn!(%error, device = %path, "paired but not trusted; it will not reconnect on its own");
            }

            let (reply, outcome) = oneshot::channel();
            let connect = Command::Connect {
                id: device.clone(),
                reply,
            };
            if events.send(Input::Command(connect)).await.is_err() {
                return;
            }
            if let Ok(Err(error)) = outcome.await {
                tracing::warn!(
                    %error,
                    device = %device.as_str(),
                    "paired but could not connect; the device row carries the reason"
                );
            }
        });
    }

    fn begin_scan(&mut self) {
        self.scans = self.scans.wrapping_add(1);
        self.scan = Some(self.scans);
        self.deadline = self.config.scan_timeout.and_then(|timeout| {
            let timeout = chrono::TimeDelta::from_std(timeout).ok()?;
            chrono::Utc::now().checked_add_signed(timeout)
        });
    }

    fn halt(&mut self, ctx: &Ctx<Self>) {
        self.scan = None;
        self.deadline = None;
        self.publish();
        let (Some(adapter), Ok(connection)) = (
            self.adapter_path().map(str::to_owned),
            ctx.system_bus().cloned(),
        ) else {
            return;
        };
        ctx.spawn_detached(move |_ctx| async move {
            let _ = call::stop_scan(&connection, &adapter).await;
        });
    }

    fn settle_scan(&mut self, ctx: &Ctx<Self>) {
        let powered = self
            .adapters
            .first_key_value()
            .is_some_and(|(_, properties)| power(properties).is_on());

        if self.scan.is_some() && !powered {
            self.halt(ctx);
        }
    }

    fn adapter_path(&self) -> Option<&str> {
        self.adapters.keys().next().map(String::as_str)
    }

    fn adopt(&mut self, objects: Objects) {
        self.adapters.clear();
        self.devices.clear();
        self.transports.clear();
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
            bluez::ADAPTER1 => {
                let held = self.adapters.entry(path.to_owned()).or_default();
                merge_adapter(held, bluez::decode_adapter(properties));
            }
            bluez::DEVICE1 => {
                let decoded = bluez::decode_device(properties);
                let connected = decoded.connected == Some(true);
                let held = self.devices.entry(path.to_owned()).or_default();
                merge_device(&mut held.properties, decoded);
                if connected {
                    held.failure = None;
                }
            }
            bluez::BATTERY1 => {
                let held = self.devices.entry(path.to_owned()).or_default();
                let decoded = bluez::decode_battery(properties);
                if decoded.percentage.is_some() {
                    held.battery = decoded.percentage;
                }
            }
            bluez::MEDIA_TRANSPORT1 => {
                let held = self.transports.entry(path.to_owned()).or_default();
                merge_transport(held, bluez::decode_transport(properties));
            }
            _ => {}
        }
    }

    fn invalidate(&mut self, path: &str, interface: &str, names: &[String]) {
        if interface != bluez::DEVICE1 {
            return;
        }
        let Some(held) = self.devices.get_mut(path) else {
            return;
        };
        for name in names {
            match name.as_str() {
                "RSSI" => held.properties.rssi = None,
                "Icon" => held.properties.icon = None,
                "Class" => held.properties.class = None,
                "UUIDs" => held.properties.uuids = None,
                _ => {}
            }
        }
    }

    fn remove(&mut self, path: &str, interfaces: &[String]) {
        for interface in interfaces {
            match interface.as_str() {
                bluez::ADAPTER1 => {
                    self.adapters.remove(path);
                }
                bluez::DEVICE1 => {
                    self.devices.remove(path);
                }
                bluez::BATTERY1 => {
                    if let Some(held) = self.devices.get_mut(path) {
                        held.battery = None;
                    }
                }
                bluez::MEDIA_TRANSPORT1 => {
                    self.transports.remove(path);
                }
                _ => {}
            }
        }
    }

    fn publish(&mut self) {
        let pairing = self.prompt.as_ref().map(|(prompt, _)| prompt.clone());
        let confirm = self.confirm.clone();

        let Some((adapter_path, properties)) = self.adapters.first_key_value() else {
            self.state.set(BluetoothState {
                pairing,
                confirm,
                ..Default::default()
            });
            return;
        };

        let adapter = Adapter {
            alias: properties.alias.clone().unwrap_or_default(),
            power: power(properties),
            discovering: properties.discovering.unwrap_or_default(),
            discoverable: properties.discoverable.unwrap_or_default(),
        };

        let mut devices: Vec<Device> = self
            .devices
            .iter()
            .filter(|(path, _)| is_device_path(adapter_path, path))
            .map(|(path, record)| self.project(path, record))
            .filter(|device| {
                !self.config.hide_unnamed
                    || device.known()
                    || !bluez::is_synthesized_name(&device.name, &device.address)
            })
            .collect();
        devices.sort_by(order);

        self.state.set(BluetoothState {
            adapter: Some(adapter),
            devices,
            scanning: self.scan.is_some(),
            pairing,
            confirm,
        });
    }

    fn project(&self, path: &str, record: &Record) -> Device {
        let properties = &record.properties;
        let address = properties.address.clone().unwrap_or_default();
        let name = properties.alias.clone().unwrap_or_else(|| address.clone());
        let connected = properties.connected.unwrap_or_default();

        Device {
            id: DeviceId(path.to_owned()),
            address,
            name,
            icon: bluez::device_icon(properties.icon.as_deref(), properties.class),
            paired: properties.paired.unwrap_or_default(),
            bonded: properties.bonded.unwrap_or_default(),
            trusted: properties.trusted.unwrap_or_default(),
            blocked: properties.blocked.unwrap_or_default(),
            connected,
            battery: record.battery,
            codec: connected.then(|| self.codec(path)).flatten(),
            rssi: properties.rssi,
            profiles: bluez::profiles(properties.uuids.as_deref().unwrap_or_default()),
            busy: record.busy,
            failure: record.failure,
        }
    }

    fn codec(&self, path: &str) -> Option<Codec> {
        let transport = self
            .transports
            .values()
            .find(|transport| transport.device.as_deref() == Some(path))?;

        Some(bluez::codec(
            transport.codec?,
            transport.configuration.as_deref(),
        ))
    }
}

fn detached(
    ctx: &Ctx<Bluetooth>,
    action: Action,
    reply: Reply,
    work: impl Future<Output = zbus::Result<()>> + Send + 'static,
) {
    ctx.spawn_detached(move |_ctx| async move {
        let _ = reply.send(settle(action, work.await).map_err(BluetoothError::Failed));
    });
}

fn reject(command: Command, reason: &str) {
    let refused: Result<(), BluetoothError> =
        Err(CommandError::Unavailable(reason.to_owned()).into());
    match command {
        Command::SetPowered { reply, .. }
        | Command::SetDiscoverable { reply, .. }
        | Command::StartScan { reply }
        | Command::StopScan { reply }
        | Command::Connect { reply, .. }
        | Command::Disconnect { reply, .. }
        | Command::Pair { reply, .. }
        | Command::CancelPairing { reply, .. }
        | Command::SetTrusted { reply, .. }
        | Command::Forget { reply, .. }
        | Command::AnswerPairing { reply, .. }
        | Command::DismissConfirmation { reply } => {
            let _ = reply.send(refused);
        }
    }
}

fn power(properties: &AdapterProperties) -> Power {
    properties
        .power_state
        .unwrap_or_else(|| Power::from_powered(properties.powered.unwrap_or_default()))
}

fn order(left: &Device, right: &Device) -> std::cmp::Ordering {
    fn rank(device: &Device) -> u8 {
        match (device.connected, device.bonded) {
            (true, _) => 0,
            (false, true) => 1,
            (false, false) => 2,
        }
    }

    rank(left)
        .cmp(&rank(right))
        .then_with(|| band(right.rssi).cmp(&band(left.rssi)))
}

fn band(rssi: Option<i16>) -> Option<i16> {
    rssi.map(|rssi| rssi / 10)
}

fn is_device_path(adapter: &str, path: &str) -> bool {
    path.strip_prefix(adapter)
        .and_then(|rest| rest.strip_prefix('/'))
        .is_some_and(|segment| segment.starts_with("dev_") && !segment.contains('/'))
}

fn keep<T>(held: &mut Option<T>, from: Option<T>) {
    if from.is_some() {
        *held = from;
    }
}

fn merge_adapter(held: &mut AdapterProperties, from: AdapterProperties) {
    keep(&mut held.address, from.address);
    keep(&mut held.alias, from.alias);
    keep(&mut held.powered, from.powered);
    keep(&mut held.power_state, from.power_state);
    keep(&mut held.discovering, from.discovering);
}

fn merge_device(held: &mut DeviceProperties, from: DeviceProperties) {
    keep(&mut held.adapter, from.adapter);
    keep(&mut held.address, from.address);
    keep(&mut held.alias, from.alias);
    keep(&mut held.icon, from.icon);
    keep(&mut held.class, from.class);
    keep(&mut held.paired, from.paired);
    keep(&mut held.bonded, from.bonded);
    keep(&mut held.trusted, from.trusted);
    keep(&mut held.blocked, from.blocked);
    keep(&mut held.connected, from.connected);
    keep(&mut held.rssi, from.rssi);
    keep(&mut held.uuids, from.uuids);
}

fn merge_transport(held: &mut TransportProperties, from: TransportProperties) {
    keep(&mut held.device, from.device);
    keep(&mut held.codec, from.codec);
    keep(&mut held.configuration, from.configuration);
}

#[cfg(test)]
mod tests {
    use glimpse_dbus::Buses;
    use tokio_util::sync::CancellationToken;
    use zbus::zvariant::Value;

    use super::*;

    const ADAPTER: &str = "/org/bluez/hci0";
    const HEADSET: &str = "/org/bluez/hci0/dev_F8_4E_17_BC_EE_D5";
    const NEARBY: &str = "/org/bluez/hci0/dev_45_16_94_89_4F_38";

    async fn bluetooth() -> (
        Bluetooth,
        Ctx<Bluetooth>,
        tokio::sync::watch::Receiver<BluetoothState>,
        tokio::sync::watch::Receiver<crate::ServiceState>,
    ) {
        let cancel = CancellationToken::new();
        let (events, _inbox) = tokio::sync::mpsc::channel(8);
        let (state, state_rx) = tokio::sync::watch::channel(BluetoothState::default());
        let (health, health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
        let ctx = Ctx::<Bluetooth>::new(
            events,
            &cancel,
            state,
            health,
            Buses::unavailable("no bus in tests"),
        );
        let service = Bluetooth::start(&ctx, Config::from(&glimpse_config::Config::default()), ())
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

    fn adapter() -> Interfaces {
        HashMap::from([(
            bluez::ADAPTER1.to_owned(),
            properties(vec![
                ("Alias", "glimpse".into()),
                ("Powered", true.into()),
                ("PowerState", "on".into()),
                ("Discoverable", false.into()),
                ("Discovering", false.into()),
            ]),
        )])
    }

    fn headset() -> Interfaces {
        HashMap::from([(
            bluez::DEVICE1.to_owned(),
            properties(vec![
                ("Address", "F8:4E:17:BC:EE:D5".into()),
                ("Alias", "WH-1000XM4".into()),
                ("Icon", "audio-headset".into()),
                ("Paired", true.into()),
                ("Bonded", true.into()),
                ("Trusted", true.into()),
                ("Connected", true.into()),
            ]),
        )])
    }

    fn nearby() -> Interfaces {
        HashMap::from([(
            bluez::DEVICE1.to_owned(),
            properties(vec![
                ("Address", "45:16:94:89:4F:38".into()),
                ("Alias", "[TV] Samsung AU7172".into()),
                ("RSSI", (-73i16).into()),
            ]),
        )])
    }

    fn beacon() -> Interfaces {
        HashMap::from([(
            bluez::DEVICE1.to_owned(),
            properties(vec![
                ("Address", "6B:2C:11:04:9A:71".into()),
                ("Alias", "6B-2C-11-04-9A-71".into()),
                ("RSSI", (-91i16).into()),
            ]),
        )])
    }

    fn session() -> Objects {
        BTreeMap::from([
            (ADAPTER.to_owned(), adapter()),
            (HEADSET.to_owned(), headset()),
            (NEARBY.to_owned(), nearby()),
        ])
    }

    async fn enumerated(service: &mut Bluetooth, ctx: &Ctx<Bluetooth>, objects: Objects) {
        service
            .handle(ctx, Input::Event(Event::Enumerated(Box::new(objects))))
            .await;
    }

    #[test]
    fn only_one_segment_below_the_adapter_is_a_device() {
        assert!(is_device_path(ADAPTER, HEADSET));
        assert!(!is_device_path(ADAPTER, &format!("{HEADSET}/fd0")));
        assert!(!is_device_path(ADAPTER, &format!("{HEADSET}/sep1")));
        assert!(!is_device_path(ADAPTER, ADAPTER));
        assert!(!is_device_path(
            ADAPTER,
            "/org/bluez/hci1/dev_F8_4E_17_BC_EE_D5"
        ));
    }

    #[tokio::test]
    async fn a_transport_feeds_a_codec_without_becoming_a_device() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        let mut objects = session();
        objects.insert(
            format!("{HEADSET}/fd0"),
            HashMap::from([(
                bluez::MEDIA_TRANSPORT1.to_owned(),
                properties(vec![
                    (
                        "Device",
                        Value::ObjectPath(HEADSET.try_into().expect("a path")),
                    ),
                    ("Codec", 255u8.into()),
                    (
                        "Configuration",
                        Value::from(vec![0x2du8, 0x01, 0x00, 0x00, 0xaa, 0x00, 0x04, 0x01]),
                    ),
                ]),
            )]),
        );
        objects.insert(format!("{HEADSET}/sep1"), HashMap::new());

        enumerated(&mut service, &ctx, objects).await;

        let published = state.borrow();
        assert_eq!(published.devices.len(), 2);
        assert!(
            published
                .devices
                .iter()
                .all(|device| !device.id.as_str().contains("/fd0"))
        );
        assert_eq!(published.devices[0].codec, Some(Codec::Ldac));
    }

    #[tokio::test]
    async fn devices_publish_connected_then_bonded_then_by_descending_signal() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        let mut objects = session();
        objects.insert(
            "/org/bluez/hci0/dev_00_00_00_00_00_01".to_owned(),
            HashMap::from([(
                bluez::DEVICE1.to_owned(),
                properties(vec![
                    ("Address", "00:00:00:00:00:01".into()),
                    ("Alias", "closer".into()),
                    ("RSSI", (-40i16).into()),
                ]),
            )]),
        );
        objects.insert(
            "/org/bluez/hci0/dev_00_00_00_00_00_02".to_owned(),
            HashMap::from([(
                bluez::DEVICE1.to_owned(),
                properties(vec![
                    ("Address", "00:00:00:00:00:02".into()),
                    ("Alias", "bonded but away".into()),
                    ("Bonded", true.into()),
                ]),
            )]),
        );

        enumerated(&mut service, &ctx, objects).await;

        let published = state.borrow();
        let names: Vec<&str> = published
            .devices
            .iter()
            .map(|device| device.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "WH-1000XM4",
                "bonded but away",
                "closer",
                "[TV] Samsung AU7172"
            ]
        );
    }

    #[tokio::test]
    async fn an_unnamed_nearby_device_is_hidden_and_a_bonded_one_is_not() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        let mut objects = session();
        objects.insert("/org/bluez/hci0/dev_6B_2C_11_04_9A_71".to_owned(), beacon());
        let mut bonded = beacon();
        bonded.get_mut(bluez::DEVICE1).expect("a device").insert(
            "Bonded".to_owned(),
            OwnedValue::try_from(Value::from(true)).expect("a flag"),
        );
        objects.insert("/org/bluez/hci0/dev_11_11_11_11_11_11".to_owned(), bonded);

        enumerated(&mut service, &ctx, objects).await;

        let published = state.borrow();
        let names: Vec<&str> = published
            .devices
            .iter()
            .map(|device| device.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec!["WH-1000XM4", "6B-2C-11-04-9A-71", "[TV] Samsung AU7172"],
            "an unnamed device that is bonded is still the user's own"
        );
    }

    #[tokio::test]
    async fn busy_is_cleared_by_the_command_completing_and_never_by_the_property() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;
        service.devices.get_mut(HEADSET).expect("the headset").busy = Some(Busy::Disconnecting);
        service.publish();

        service
            .handle(
                &ctx,
                Input::Event(Event::PropertiesChanged {
                    path: HEADSET.to_owned(),
                    interface: bluez::DEVICE1.to_owned(),
                    changed: properties(vec![("Connected", false.into())]),
                    invalidated: Vec::new(),
                }),
            )
            .await;
        assert_eq!(
            state
                .borrow()
                .device(&DeviceId(HEADSET.to_owned()))
                .and_then(|device| device.busy),
            Some(Busy::Disconnecting),
            "a backend that answers success without moving the property would strand the row"
        );

        service
            .handle(
                &ctx,
                Input::Event(Event::Settled {
                    id: DeviceId(HEADSET.to_owned()),
                    busy: Busy::Disconnecting,
                    failure: None,
                }),
            )
            .await;
        assert_eq!(
            state
                .borrow()
                .device(&DeviceId(HEADSET.to_owned()))
                .and_then(|device| device.busy),
            None
        );
    }

    #[tokio::test]
    async fn a_settled_event_for_a_command_that_was_replaced_leaves_the_new_one_running() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;
        service.devices.get_mut(HEADSET).expect("the headset").busy = Some(Busy::Connecting);
        service.publish();

        service
            .handle(
                &ctx,
                Input::Event(Event::Settled {
                    id: DeviceId(HEADSET.to_owned()),
                    busy: Busy::Disconnecting,
                    failure: None,
                }),
            )
            .await;

        assert_eq!(
            state
                .borrow()
                .device(&DeviceId(HEADSET.to_owned()))
                .and_then(|device| device.busy),
            Some(Busy::Connecting)
        );
    }

    #[tokio::test]
    async fn a_command_with_no_bus_is_refused_rather_than_queued() {
        let (mut service, ctx, _state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;

        let (reply, result) = oneshot::channel();
        service
            .handle(
                &ctx,
                Input::Command(Command::Connect {
                    id: DeviceId(HEADSET.to_owned()),
                    reply,
                }),
            )
            .await;

        assert!(matches!(
            result.await,
            Ok(Err(BluetoothError::Service(CommandError::Unavailable(_))))
        ));
    }

    #[tokio::test]
    async fn a_scan_is_declared_only_while_one_is_running_and_stops_when_the_radio_goes_off() {
        let (mut service, ctx, _state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;
        assert!(
            !service
                .subscriptions()
                .iter()
                .any(|sub| matches!(sub.key(), Watch::ScanDeadline(_))),
            "nothing may start a scan on its own"
        );

        service.begin_scan();
        assert!(
            service
                .subscriptions()
                .iter()
                .any(|sub| matches!(sub.key(), Watch::ScanDeadline(1)))
        );

        service
            .handle(
                &ctx,
                Input::Event(Event::PropertiesChanged {
                    path: ADAPTER.to_owned(),
                    interface: bluez::ADAPTER1.to_owned(),
                    changed: properties(vec![("PowerState", "off".into())]),
                    invalidated: Vec::new(),
                }),
            )
            .await;

        assert_eq!(service.scan, None);
    }

    #[tokio::test]
    async fn the_published_scanning_flag_is_ours_and_not_the_adapters() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;
        assert!(!state.borrow().scanning);

        service.begin_scan();
        service.publish();
        assert!(state.borrow().scanning);

        service
            .handle(&ctx, Input::Event(Event::ScanExpired(1)))
            .await;
        service.publish();
        assert!(
            !state.borrow().scanning,
            "the deadline stopping the scan must reach the popover"
        );
    }

    #[tokio::test]
    async fn an_expired_deadline_from_a_previous_scan_does_not_stop_the_current_one() {
        let (mut service, ctx, _state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;
        service.begin_scan();
        service.begin_scan();

        service
            .handle(&ctx, Input::Event(Event::ScanExpired(1)))
            .await;

        assert_eq!(service.scan, Some(2));
    }

    #[tokio::test]
    async fn a_zero_timeout_declares_no_deadline_at_all() {
        let (mut service, ctx, _state, _health) = bluetooth().await;
        service.config.scan_timeout = None;
        service.begin_scan();
        let _ = &ctx;

        assert_eq!(service.scan, Some(1), "the scan still runs");
        assert!(
            !service
                .subscriptions()
                .iter()
                .any(|sub| matches!(sub.key(), Watch::ScanDeadline(_))),
            "with no timeout there is nothing to stop it but the popover"
        );
    }

    #[tokio::test]
    async fn a_signal_from_before_the_daemon_restarted_is_dropped() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;

        service
            .handle(
                &ctx,
                Input::Event(Event::NameOwner(Some(":1.42".to_owned()))),
            )
            .await;
        service
            .handle(
                &ctx,
                Input::Event(Event::InterfacesRemoved {
                    path: HEADSET.to_owned(),
                    interfaces: vec![bluez::DEVICE1.to_owned()],
                }),
            )
            .await;

        assert!(
            state
                .borrow()
                .device(&DeviceId(HEADSET.to_owned()))
                .is_some(),
            "a queued signal from the previous owner must not edit what the next enumeration will \
             deliver"
        );
    }

    #[tokio::test]
    async fn an_invalidated_property_is_forgotten_rather_than_kept() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;
        assert_eq!(
            state
                .borrow()
                .device(&DeviceId(NEARBY.to_owned()))
                .and_then(|device| device.rssi),
            Some(-73)
        );

        service
            .handle(
                &ctx,
                Input::Event(Event::PropertiesChanged {
                    path: NEARBY.to_owned(),
                    interface: bluez::DEVICE1.to_owned(),
                    changed: properties(vec![]),
                    invalidated: vec!["RSSI".to_owned()],
                }),
            )
            .await;

        assert_eq!(
            state
                .borrow()
                .device(&DeviceId(NEARBY.to_owned()))
                .and_then(|device| device.rssi),
            None,
            "a stale signal strength would keep ordering the list it no longer describes"
        );
    }

    #[tokio::test]
    async fn a_local_disconnect_leaves_no_failure_and_a_timeout_does() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;

        service
            .handle(
                &ctx,
                Input::Event(Event::Disconnected {
                    path: HEADSET.to_owned(),
                    reason: DisconnectReason::Local,
                }),
            )
            .await;
        assert_eq!(
            state
                .borrow()
                .device(&DeviceId(HEADSET.to_owned()))
                .and_then(|device| device.failure),
            None,
            "the user pressed disconnect; there is nothing to report"
        );

        service
            .handle(
                &ctx,
                Input::Event(Event::Disconnected {
                    path: HEADSET.to_owned(),
                    reason: DisconnectReason::Timeout,
                }),
            )
            .await;
        assert_eq!(
            state
                .borrow()
                .device(&DeviceId(HEADSET.to_owned()))
                .and_then(|device| device.failure),
            Some(Failure::Dropped)
        );
    }

    #[tokio::test]
    async fn a_failure_is_cleared_by_the_device_connecting() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;
        service
            .handle(
                &ctx,
                Input::Event(Event::Disconnected {
                    path: HEADSET.to_owned(),
                    reason: DisconnectReason::Remote,
                }),
            )
            .await;

        service
            .handle(
                &ctx,
                Input::Event(Event::PropertiesChanged {
                    path: HEADSET.to_owned(),
                    interface: bluez::DEVICE1.to_owned(),
                    changed: properties(vec![("Connected", true.into())]),
                    invalidated: Vec::new(),
                }),
            )
            .await;

        assert_eq!(
            state
                .borrow()
                .device(&DeviceId(HEADSET.to_owned()))
                .and_then(|device| device.failure),
            None
        );
    }

    #[tokio::test]
    async fn only_a_pairing_this_service_was_waiting_on_trusts_and_connects() {
        let (mut service, ctx, _state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;

        let settled = |busy| {
            Input::Event(Event::Settled {
                id: DeviceId(HEADSET.to_owned()),
                busy,
                failure: None,
            })
        };

        service.devices.get_mut(HEADSET).expect("the headset").busy = Some(Busy::Connecting);
        service.handle(&ctx, settled(Busy::Pairing)).await;
        assert!(
            service.devices.get(HEADSET).expect("the headset").busy == Some(Busy::Connecting),
            "a pairing that answers a record the service is no longer waiting on must change \
             nothing — not the busy state it did not set, and not trust"
        );

        service.devices.get_mut(HEADSET).expect("the headset").busy = Some(Busy::Pairing);
        service.handle(&ctx, settled(Busy::Pairing)).await;
        assert_eq!(
            service.devices.get(HEADSET).expect("the headset").busy,
            None,
            "the pairing it was waiting on settles the record"
        );
    }

    #[tokio::test]
    async fn making_the_adapter_discoverable_reaches_the_bus_rather_than_the_state() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;

        assert!(
            !state
                .borrow()
                .adapter
                .as_ref()
                .expect("an adapter")
                .discoverable,
            "the adapter publishes what BlueZ says, and the fixture says it is not discoverable"
        );

        let (reply, outcome) = oneshot::channel();
        service
            .handle(
                &ctx,
                Input::Command(Command::SetDiscoverable { on: true, reply }),
            )
            .await;

        assert!(
            matches!(
                outcome.await,
                Ok(Err(BluetoothError::Service(CommandError::Unavailable(_))))
            ),
            "with no bus the command is refused rather than pretending the adapter changed"
        );
        assert!(
            !state
                .borrow()
                .adapter
                .as_ref()
                .expect("an adapter")
                .discoverable,
            "and nothing moves optimistically"
        );
    }

    #[tokio::test]
    async fn a_successful_command_clears_the_failure_the_last_one_left() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;
        service.devices.get_mut(HEADSET).expect("the headset").busy = Some(Busy::Connecting);
        service
            .devices
            .get_mut(HEADSET)
            .expect("the headset")
            .failure = Some(Failure::Unreachable);

        service
            .handle(
                &ctx,
                Input::Event(Event::Settled {
                    id: DeviceId(HEADSET.to_owned()),
                    busy: Busy::Connecting,
                    failure: None,
                }),
            )
            .await;

        assert_eq!(
            state
                .borrow()
                .device(&DeviceId(HEADSET.to_owned()))
                .and_then(|device| device.failure),
            None
        );
    }

    #[tokio::test]
    async fn a_prompt_reaches_the_state_and_an_answer_clears_it() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;
        let (answer, reply) = oneshot::channel();

        service
            .handle(
                &ctx,
                Input::Event(Event::Prompt(
                    Prompt::Confirm {
                        device: DeviceId(HEADSET.to_owned()),
                        passkey: 123_456,
                    },
                    Some(answer),
                )),
            )
            .await;
        assert!(state.borrow().pairing.is_some());

        let (sent, outcome) = oneshot::channel();
        service
            .handle(
                &ctx,
                Input::Command(Command::AnswerPairing {
                    answer: Answer::Confirm,
                    reply: sent,
                }),
            )
            .await;

        assert_eq!(state.borrow().pairing, None);
        assert!(matches!(outcome.await, Ok(Ok(()))));
        assert_eq!(reply.await, Ok(Answer::Confirm));
    }

    #[tokio::test]
    async fn an_answer_with_nothing_waiting_is_refused() {
        let (mut service, ctx, _state, _health) = bluetooth().await;
        let (sent, outcome) = oneshot::channel();

        service
            .handle(
                &ctx,
                Input::Command(Command::AnswerPairing {
                    answer: Answer::Confirm,
                    reply: sent,
                }),
            )
            .await;

        assert!(matches!(
            outcome.await,
            Ok(Err(BluetoothError::Service(CommandError::Unavailable(_))))
        ));
    }

    #[tokio::test]
    async fn a_second_prompt_replaces_the_first_and_cancels_its_call() {
        let (mut service, ctx, _state, _health) = bluetooth().await;
        let (first, canceled) = oneshot::channel();
        let (second, _still_open) = oneshot::channel();

        service
            .handle(
                &ctx,
                Input::Event(Event::Prompt(
                    Prompt::Authorize(DeviceId(HEADSET.to_owned())),
                    Some(first),
                )),
            )
            .await;
        service
            .handle(
                &ctx,
                Input::Event(Event::Prompt(
                    Prompt::RequestPin(DeviceId(NEARBY.to_owned())),
                    Some(second),
                )),
            )
            .await;

        assert!(
            canceled.await.is_err(),
            "the superseded call must not be left parked forever"
        );
    }

    #[tokio::test]
    async fn pairing_without_an_agent_is_refused_in_its_own_words() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;
        assert!(!service.agent, "there is no bus in tests to register on");

        let (sent, outcome) = oneshot::channel();
        service
            .handle(
                &ctx,
                Input::Command(Command::Pair {
                    id: DeviceId(HEADSET.to_owned()),
                    reply: sent,
                }),
            )
            .await;

        assert!(
            matches!(
                outcome.await,
                Ok(Err(BluetoothError::Failed(Failure::NoAgent)))
            ),
            "a pairing with no agent is refused as a typed failure the applet can word"
        );
        assert_eq!(
            state
                .borrow()
                .device(&DeviceId(HEADSET.to_owned()))
                .and_then(|device| device.failure),
            Some(Failure::NoAgent)
        );
    }

    async fn command(
        service: &mut Bluetooth,
        ctx: &Ctx<Bluetooth>,
        build: impl FnOnce(Reply) -> Command,
    ) -> Result<(), BluetoothError> {
        let (reply, outcome) = oneshot::channel();
        service.handle(ctx, Input::Command(build(reply))).await;
        outcome.await.expect("the command was answered")
    }

    #[tokio::test]
    async fn an_unconfirmed_forget_executes_nothing_and_asks_instead() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;

        let outcome = command(&mut service, &ctx, |reply| Command::Forget {
            id: DeviceId(HEADSET.to_owned()),
            confirmed: false,
            reply,
        })
        .await;

        assert!(outcome.is_ok());
        assert_eq!(
            state.borrow().confirm,
            Some(Confirmation::Forget {
                device: DeviceId(HEADSET.to_owned()),
                connected: true,
            })
        );
        assert_eq!(
            state
                .borrow()
                .device(&DeviceId(HEADSET.to_owned()))
                .and_then(|device| device.busy),
            None,
            "a confirmed action cannot be optimistic: nothing may move before the answer"
        );
    }

    #[tokio::test]
    async fn an_unconfirmed_forget_of_a_disconnected_device_says_so() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;
        service
            .handle(
                &ctx,
                Input::Event(Event::PropertiesChanged {
                    path: HEADSET.to_owned(),
                    interface: bluez::DEVICE1.to_owned(),
                    changed: properties(vec![("Connected", false.into())]),
                    invalidated: Vec::new(),
                }),
            )
            .await;

        let _ = command(&mut service, &ctx, |reply| Command::Forget {
            id: DeviceId(HEADSET.to_owned()),
            confirmed: false,
            reply,
        })
        .await;

        assert_eq!(
            state.borrow().confirm,
            Some(Confirmation::Forget {
                device: DeviceId(HEADSET.to_owned()),
                connected: false,
            })
        );
    }

    #[tokio::test]
    async fn connect_disconnect_and_the_power_toggle_never_confirm() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;

        for build in [
            &(|reply| Command::Connect {
                id: DeviceId(HEADSET.to_owned()),
                reply,
            }) as &dyn Fn(Reply) -> Command,
            &|reply| Command::Disconnect {
                id: DeviceId(HEADSET.to_owned()),
                reply,
            },
            &|reply| Command::SetPowered {
                powered: false,
                reply,
            },
        ] {
            let (reply, _outcome) = oneshot::channel();
            service.handle(&ctx, Input::Command(build(reply))).await;
            assert_eq!(state.borrow().confirm, None);
        }
    }

    #[tokio::test]
    async fn dismissing_a_confirmation_clears_it_and_calls_nothing() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;
        let _ = command(&mut service, &ctx, |reply| Command::Forget {
            id: DeviceId(HEADSET.to_owned()),
            confirmed: false,
            reply,
        })
        .await;

        let outcome = command(&mut service, &ctx, |reply| Command::DismissConfirmation {
            reply,
        })
        .await;

        assert!(outcome.is_ok());
        assert_eq!(state.borrow().confirm, None);
        assert_eq!(state.borrow().devices.len(), 2, "nothing was forgotten");
    }

    #[tokio::test]
    async fn a_confirmed_forget_clears_the_confirmation_and_reaches_the_backend() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;
        let _ = command(&mut service, &ctx, |reply| Command::Forget {
            id: DeviceId(HEADSET.to_owned()),
            confirmed: false,
            reply,
        })
        .await;

        let outcome = command(&mut service, &ctx, |reply| Command::Forget {
            id: DeviceId(HEADSET.to_owned()),
            confirmed: true,
            reply,
        })
        .await;

        assert_eq!(state.borrow().confirm, None);
        assert!(
            matches!(
                outcome,
                Err(BluetoothError::Service(CommandError::Unavailable(_)))
            ),
            "it went to the backend, which is unreachable in tests"
        );
    }

    #[tokio::test]
    async fn a_connected_device_with_no_battery_interface_publishes_none() {
        let (mut service, ctx, state, _health) = bluetooth().await;

        enumerated(&mut service, &ctx, session()).await;

        let headset = state.borrow();
        let headset = headset
            .device(&DeviceId(HEADSET.to_owned()))
            .expect("the headset");
        assert!(headset.connected);
        assert_eq!(
            headset.battery, None,
            "a connected WH-1000XM4 carried no Battery1 at all; this is the ordinary case"
        );
    }

    #[tokio::test]
    async fn a_blocking_command_publishes_busy_and_does_not_hold_the_handler() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;
        let (reply, mut outcome) = oneshot::channel();

        service.spawned(
            &ctx,
            DeviceId(HEADSET.to_owned()),
            Busy::Connecting,
            Action::Connect,
            reply,
            std::future::pending(),
        );

        assert_eq!(
            state
                .borrow()
                .device(&DeviceId(HEADSET.to_owned()))
                .and_then(|device| device.busy),
            Some(Busy::Connecting),
            "the row shows a pending state before the call returns"
        );
        assert!(
            outcome.try_recv().is_err(),
            "the reply is still typed and still outstanding"
        );
    }

    #[tokio::test]
    async fn a_prompt_survives_an_adapter_the_service_has_not_seen_yet() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        let (answer, _reply) = oneshot::channel();

        service
            .handle(
                &ctx,
                Input::Event(Event::Prompt(
                    Prompt::Authorize(DeviceId(HEADSET.to_owned())),
                    Some(answer),
                )),
            )
            .await;

        assert!(
            state.borrow().pairing.is_some(),
            "a prompt is not the adapter list's to withhold; BlueZ asked and the answer is owed"
        );
    }

    #[tokio::test]
    async fn an_absent_adapter_publishes_nothing_and_stays_running() {
        let (mut service, ctx, state, health) = bluetooth().await;

        enumerated(&mut service, &ctx, BTreeMap::new()).await;

        assert_eq!(*state.borrow(), BluetoothState::default());
        assert!(matches!(&*health.borrow(), crate::ServiceState::Running));
    }

    #[tokio::test]
    async fn a_lost_bus_degrades_and_keeps_the_last_state() {
        let (mut service, ctx, state, health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;

        service
            .handle(
                &ctx,
                Input::Event(Event::Unavailable("no system bus".to_owned())),
            )
            .await;

        assert_eq!(state.borrow().devices.len(), 2);
        assert!(matches!(
            &*health.borrow(),
            crate::ServiceState::Degraded { .. }
        ));
    }

    #[tokio::test]
    async fn one_properties_subscription_serves_every_device() {
        let (mut service, ctx, _state, _health) = bluetooth().await;
        let mut objects = session();
        for index in 0..17u8 {
            objects.insert(
                format!("/org/bluez/hci0/dev_00_00_00_00_00_{index:02X}"),
                nearby(),
            );
        }

        enumerated(&mut service, &ctx, objects).await;

        let declared = service.subscriptions();
        let keys: Vec<&Watch> = declared.iter().map(Sub::key).collect();
        assert_eq!(
            keys,
            vec![
                &Watch::NameOwner,
                &Watch::Objects(0),
                &Watch::Properties(0),
                &Watch::Disconnects(0)
            ]
        );
    }

    #[tokio::test]
    async fn regaining_the_name_restarts_the_object_sources_but_not_the_name_watch() {
        let (mut service, ctx, _state, _health) = bluetooth().await;

        service
            .handle(
                &ctx,
                Input::Event(Event::NameOwner(Some(":1.42".to_owned()))),
            )
            .await;

        let declared = service.subscriptions();
        let keys: Vec<&Watch> = declared.iter().map(Sub::key).collect();
        assert_eq!(
            keys,
            vec![
                &Watch::NameOwner,
                &Watch::Objects(1),
                &Watch::Properties(1),
                &Watch::Disconnects(1)
            ]
        );
    }

    #[tokio::test]
    async fn a_partial_change_keeps_what_it_does_not_carry() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;

        service
            .handle(
                &ctx,
                Input::Event(Event::PropertiesChanged {
                    path: HEADSET.to_owned(),
                    interface: bluez::DEVICE1.to_owned(),
                    changed: properties(vec![("Connected", false.into())]),
                    invalidated: Vec::new(),
                }),
            )
            .await;

        let published = state.borrow();
        let headset = published
            .device(&DeviceId(HEADSET.to_owned()))
            .expect("the headset");
        assert_eq!(headset.name, "WH-1000XM4");
        assert!(!headset.connected);
        assert!(headset.bonded);
    }

    #[tokio::test]
    async fn a_battery_interface_reaches_a_device_that_already_exists() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;

        service
            .handle(
                &ctx,
                Input::Event(Event::InterfacesAdded {
                    path: HEADSET.to_owned(),
                    interfaces: HashMap::from([(
                        bluez::BATTERY1.to_owned(),
                        properties(vec![("Percentage", 80u8.into())]),
                    )]),
                }),
            )
            .await;
        assert_eq!(
            state
                .borrow()
                .device(&DeviceId(HEADSET.to_owned()))
                .and_then(|device| device.battery),
            Some(80)
        );

        service
            .handle(
                &ctx,
                Input::Event(Event::InterfacesRemoved {
                    path: HEADSET.to_owned(),
                    interfaces: vec![bluez::BATTERY1.to_owned()],
                }),
            )
            .await;
        assert_eq!(
            state
                .borrow()
                .device(&DeviceId(HEADSET.to_owned()))
                .and_then(|device| device.battery),
            None
        );
    }

    #[tokio::test]
    async fn a_removed_device_leaves_the_list() {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;

        service
            .handle(
                &ctx,
                Input::Event(Event::InterfacesRemoved {
                    path: NEARBY.to_owned(),
                    interfaces: vec![bluez::DEVICE1.to_owned()],
                }),
            )
            .await;

        assert_eq!(state.borrow().devices.len(), 1);
    }

    #[tokio::test]
    async fn an_adapter_without_power_state_still_reports_its_power() {
        let (mut service, ctx, state, _health) = bluetooth().await;

        enumerated(
            &mut service,
            &ctx,
            BTreeMap::from([(
                ADAPTER.to_owned(),
                HashMap::from([(
                    bluez::ADAPTER1.to_owned(),
                    properties(vec![("Powered", false.into())]),
                )]),
            )]),
        )
        .await;

        assert_eq!(
            state.borrow().adapter.as_ref().map(|adapter| adapter.power),
            Some(Power::Off)
        );
    }
}
