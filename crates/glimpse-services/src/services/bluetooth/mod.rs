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
    service::{CommandError, Input, Service},
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    Timed,
    Held,
}

const ACTIVE: &str = "bluetooth-active-symbolic";
const IDLE: &str = "bluetooth-symbolic";
const SCANNING: &str = "bluetooth-acquiring-symbolic";
const OFF: &str = "bluetooth-disabled-symbolic";
const BLOCKED: &str = "bluetooth-hardware-disabled-symbolic";

pub fn icon_for(power: Power, discovering: bool, any_connected: bool) -> &'static str {
    match power {
        Power::Blocked => BLOCKED,
        Power::Off | Power::Disabling => OFF,
        Power::Enabling => IDLE,
        Power::On if discovering => SCANNING,
        Power::On if any_connected => ACTIVE,
        Power::On => IDLE,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BluetoothState {
    pub adapter: Option<Adapter>,
    pub devices: Vec<Device>,
    pub scan: Option<Hold>,
    pub pairing: Option<Prompt>,
    pub confirm: Option<Confirmation>,
}

impl BluetoothState {
    pub fn scanning(&self) -> bool {
        self.scan.is_some()
    }

    pub fn held(&self) -> bool {
        self.scan == Some(Hold::Held)
    }

    pub fn icon_name(&self) -> Option<&'static str> {
        let adapter = self.adapter.as_ref()?;
        Some(icon_for(
            adapter.power,
            self.held(),
            self.connected().next().is_some(),
        ))
    }
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
        hold: Hold,
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
    AuthorizeService {
        device: DeviceId,
        reply: oneshot::Sender<bool>,
    },
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

pub struct Dependencies {
    pub agent: bool,
}

pub struct Bluetooth {
    state: Publisher<BluetoothState>,
    config: Config,
    generation: u64,
    adopted: bool,
    scan: Option<(u64, Hold)>,
    scans: u64,
    deadline: Option<chrono::DateTime<chrono::Utc>>,
    agent_enabled: bool,
    #[cfg(test)]
    register_attempts: std::cell::Cell<u32>,
    agent: bool,
    prompt: Option<(Prompt, Option<oneshot::Sender<Answer>>)>,
    confirm: Option<Confirmation>,
    adapters: BTreeMap<String, AdapterProperties>,
    devices: BTreeMap<String, Record>,
    transports: BTreeMap<String, TransportProperties>,
}

pub type BluetoothHandle = crate::ServiceEndpoint<Bluetooth>;

impl crate::ServiceEndpoint<Bluetooth> {
    pub async fn set_powered(&self, powered: bool) -> Result<(), BluetoothError> {
        self.call("bluetooth stopped before completing the command", |reply| {
            Command::SetPowered { powered, reply }
        })
        .await?
    }

    pub async fn start_scan(&self, hold: Hold) -> Result<(), BluetoothError> {
        self.call("bluetooth stopped before completing the command", |reply| {
            Command::StartScan { hold, reply }
        })
        .await?
    }

    pub async fn set_discoverable(&self, on: bool) -> Result<(), BluetoothError> {
        self.call("bluetooth stopped before completing the command", |reply| {
            Command::SetDiscoverable { on, reply }
        })
        .await?
    }

    pub async fn stop_scan(&self) -> Result<(), BluetoothError> {
        self.call("bluetooth stopped before completing the command", |reply| {
            Command::StopScan { reply }
        })
        .await?
    }

    pub async fn connect(&self, id: DeviceId) -> Result<(), BluetoothError> {
        self.call("bluetooth stopped before completing the command", |reply| {
            Command::Connect { id, reply }
        })
        .await?
    }

    pub async fn disconnect(&self, id: DeviceId) -> Result<(), BluetoothError> {
        self.call("bluetooth stopped before completing the command", |reply| {
            Command::Disconnect { id, reply }
        })
        .await?
    }

    pub async fn pair(&self, id: DeviceId) -> Result<(), BluetoothError> {
        self.call("bluetooth stopped before completing the command", |reply| {
            Command::Pair { id, reply }
        })
        .await?
    }

    pub async fn cancel_pairing(&self, id: DeviceId) -> Result<(), BluetoothError> {
        self.call("bluetooth stopped before completing the command", |reply| {
            Command::CancelPairing { id, reply }
        })
        .await?
    }

    pub async fn set_trusted(&self, id: DeviceId, trusted: bool) -> Result<(), BluetoothError> {
        self.call("bluetooth stopped before completing the command", |reply| {
            Command::SetTrusted { id, trusted, reply }
        })
        .await?
    }

    pub async fn forget(&self, id: DeviceId, confirmed: bool) -> Result<(), BluetoothError> {
        self.call("bluetooth stopped before completing the command", |reply| {
            Command::Forget {
                id,
                confirmed,
                reply,
            }
        })
        .await?
    }

    pub async fn dismiss_confirmation(&self) -> Result<(), BluetoothError> {
        self.call("bluetooth stopped before completing the command", |reply| {
            Command::DismissConfirmation { reply }
        })
        .await?
    }

    pub async fn answer_pairing(&self, answer: Answer) -> Result<(), BluetoothError> {
        self.call("bluetooth stopped before completing the command", |reply| {
            Command::AnswerPairing { answer, reply }
        })
        .await?
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
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = Watch;

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
        if let (Some((scan, _)), Some(at)) = (self.scan, self.deadline) {
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
        dependencies: Self::Dependencies,
    ) -> Self {
        let service = Self {
            state: ctx.publisher(),
            config,
            generation: 0,
            adopted: false,
            scan: None,
            scans: 0,
            deadline: None,
            agent_enabled: dependencies.agent,
            #[cfg(test)]
            register_attempts: std::cell::Cell::new(0),
            agent: false,
            prompt: None,
            confirm: None,
            adapters: BTreeMap::new(),
            devices: BTreeMap::new(),
            transports: BTreeMap::new(),
        };

        service.reregister(ctx);

        service
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
                    self.scan = None;
                    self.deadline = None;
                    self.publish();
                    self.reregister(ctx);
                }
                None => {
                    self.agent = false;
                    self.prompt = None;
                    self.confirm = None;
                    self.scan = None;
                    self.deadline = None;
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
                if let Some((held, Some(_))) = self.prompt.replace((prompt, answer)) {
                    tracing::warn!(
                        prompt = ?held,
                        "a pairing question was replaced before it was answered"
                    );
                }
                self.publish();
            }
            Input::Event(Event::PromptGone) => {
                self.prompt = None;
                self.publish();
            }
            Input::Event(Event::AuthorizeService { device, reply }) => {
                let bonded = self
                    .devices
                    .get(device.as_str())
                    .is_some_and(|record| record.properties.bonded == Some(true));
                let _ = reply.send(bonded);
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
                self.bonded(ctx, &path);
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
                if self.scan.is_some_and(|(running, _)| running == generation) {
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
            Command::StartScan { hold, reply } => {
                self.begin_scan(hold);
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
        if !self.agent_enabled {
            return;
        }
        #[cfg(test)]
        self.register_attempts.set(self.register_attempts.get() + 1);

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

    fn bonded(&mut self, ctx: &Ctx<Self>, path: &str) {
        let shown = self
            .prompt
            .as_ref()
            .is_some_and(|(prompt, answer)| answer.is_none() && prompt.device().as_str() == path);
        if !shown {
            return;
        }
        let Some(record) = self.devices.get(path) else {
            return;
        };
        if record.properties.bonded != Some(true) {
            return;
        }
        let trusted = record.properties.trusted == Some(true);

        self.prompt = None;
        if !trusted {
            self.trust_then_connect(ctx, &DeviceId(path.to_owned()));
        }
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

    fn begin_scan(&mut self, hold: Hold) {
        self.scans = self.scans.wrapping_add(1);
        self.scan = Some((self.scans, hold));
        self.deadline = match hold {
            Hold::Held => None,
            Hold::Timed => self.config.scan_timeout.and_then(|timeout| {
                let timeout = chrono::TimeDelta::from_std(timeout).ok()?;
                chrono::Utc::now().checked_add_signed(timeout)
            }),
        };
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
            scan: self.scan.map(|(_, hold)| hold),
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
        | Command::StartScan { reply, .. }
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
    keep(&mut held.discoverable, from.discoverable);
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
mod tests;
