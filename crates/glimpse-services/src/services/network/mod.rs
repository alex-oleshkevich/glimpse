mod agent;
mod call;
mod failure;
mod source;

use std::collections::{BTreeMap, HashMap};

use tokio::sync::oneshot;
use zbus::zvariant::OwnedValue;

use glimpse_dbus::network_manager as nm;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, Service, ServiceError},
    subscription::Sub,
};

pub(crate) type Properties = HashMap<String, OwnedValue>;
pub(crate) type Interfaces = HashMap<String, Properties>;
pub use agent::{Answer, Request};
pub use failure::{Action, Failure};

pub type Secret = zeroize::Zeroizing<String>;

pub(crate) type Objects = BTreeMap<String, Interfaces>;
pub(crate) type Profiles = BTreeMap<String, nm::Profile>;

const WATCHED: &[&str] = &[
    "ActiveAccessPoint",
    "ActiveConnection",
    "AccessPoints",
    "Carrier",
    "Connectivity",
    "Default",
    "Devices",
    "ActiveConnections",
    "Flags",
    "Frequency",
    "Id",
    "Managed",
    "Metered",
    "NetworkingEnabled",
    "PrimaryConnection",
    "RsnFlags",
    "Speed",
    "Ssid",
    "State",
    "StateReason",
    "Strength",
    "Type",
    "Uuid",
    "Vpn",
    "VpnState",
    "WirelessEnabled",
    "WirelessHardwareEnabled",
    "WpaFlags",
];

pub fn relevant(changed: &Properties, invalidated: &[String]) -> bool {
    changed.keys().any(|key| WATCHED.contains(&key.as_str()))
        || invalidated
            .iter()
            .any(|key| WATCHED.contains(&key.as_str()))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NetworkId(String);

impl NetworkId {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Busy {
    Connecting,
    Disconnecting,
    Forgetting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Radio {
    pub enabled: bool,
    pub hardware_enabled: bool,
}

impl Radio {
    pub fn blocked(self) -> bool {
        !self.hardware_enabled
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access {
    pub id: NetworkId,
    pub ssid: Option<String>,
    pub bssid: Option<String>,
    pub strength: u8,
    pub band: nm::Band,
    pub security: nm::Security,
    pub active: bool,
    pub saved: Option<NetworkId>,
    pub busy: Option<Busy>,
    pub failure: Option<Failure>,
}

impl Access {
    pub fn known(&self) -> bool {
        self.saved.is_some()
    }

    pub fn band_of(&self) -> nm::Strength {
        nm::Strength::band(self.strength)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub id: NetworkId,
    pub name: Option<String>,
    pub kind: String,
    pub uuid: Option<String>,
    pub autoconnect: bool,
    pub in_range: bool,
    pub active: bool,
    pub busy: Option<Busy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wired {
    pub id: NetworkId,
    pub name: String,
    pub carrier: bool,
    pub speed: Option<u32>,
    pub active: bool,
    pub busy: Option<Busy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vpn {
    pub id: NetworkId,
    pub name: String,
    pub kind: String,
    pub state: nm::VpnState,
    pub active: bool,
    pub failure: Option<Failure>,
    pub busy: Option<Busy>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkState {
    pub networking: bool,
    pub wifi: Option<Radio>,
    pub connectivity: nm::Connectivity,
    pub metered: nm::Metered,
    pub primary: Option<String>,
    pub networks: Vec<Access>,
    pub known: Vec<Saved>,
    pub wired: Vec<Wired>,
    pub vpn: Vec<Vpn>,
    pub scanning: bool,
    pub secret: Option<Request>,
}

impl NetworkState {
    pub fn connected(&self) -> Option<&Access> {
        self.networks.iter().find(|network| network.active)
    }

    pub fn reaches_the_internet(&self) -> bool {
        self.connectivity.reaches_the_internet()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    scan_timeout: Option<std::time::Duration>,
    hide_unnamed: bool,
    show_vpn: bool,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            scan_timeout: (document.network.scan_timeout > 0)
                .then(|| std::time::Duration::from_secs(document.network.scan_timeout)),
            hide_unnamed: document.network.hide_unnamed,
            show_vpn: document.network.show_vpn,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Watch {
    NameOwner,
    Objects(u64),
    Properties(u64),
    ActiveStates(u64),
    DeviceStates(u64),
    ProfileUpdates(u64),
    ScanDeadline(u64),
}

type Reply = oneshot::Sender<Result<(), NetworkError>>;

#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("NetworkManager refused the command: {0:?}")]
    Failed(Failure),
    #[error("the network is unavailable: {0}")]
    Unavailable(String),
    #[error(transparent)]
    Service(#[from] CommandError),
}

impl NetworkError {
    pub fn failure(&self) -> Option<Failure> {
        match self {
            Self::Failed(failure) => Some(*failure),
            _ => None,
        }
    }
}

fn settle(action: Action, outcome: zbus::Result<()>) -> Result<(), NetworkError> {
    let Err(error) = outcome else {
        return Ok(());
    };
    match failure::classify(action, &error) {
        Ok(()) => Ok(()),
        Err(failure) => {
            tracing::warn!(?action, ?failure, %error, "networkmanager refused a command");
            Err(NetworkError::Failed(failure))
        }
    }
}

pub enum Command {
    SetNetworkingEnabled {
        enabled: bool,
        reply: Reply,
    },
    SetWifiEnabled {
        enabled: bool,
        reply: Reply,
    },
    StartScan {
        reply: Reply,
    },
    StopScan {
        reply: Reply,
    },
    ConnectAccessPoint {
        id: NetworkId,
        secret: Option<Secret>,
        reply: Reply,
    },
    ConnectHidden {
        ssid: String,
        security: nm::Security,
        secret: Option<Secret>,
        reply: Reply,
    },
    ConnectProfile {
        id: NetworkId,
        reply: Reply,
    },
    Disconnect {
        id: NetworkId,
        reply: Reply,
    },
    Forget {
        id: NetworkId,
        reply: Reply,
    },
    SetAutoconnect {
        id: NetworkId,
        autoconnect: bool,
        reply: Reply,
    },
    AnswerSecret {
        answer: Answer,
        reply: Reply,
    },
    ConnectVpn {
        id: NetworkId,
        reply: Reply,
    },
    DisconnectVpn {
        id: NetworkId,
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
    Profiles(Box<Profiles>),
    ProfilesChanged,
    Settled {
        key: String,
    },
    ScanExpired(u64),
    Secrets(Request, oneshot::Sender<Answer>),
    SecretsCancelled,
    AgentRegistered(bool),
    ActiveStateChanged {
        path: String,
        state: nm::ActiveState,
        reason: u32,
    },
    DeviceStateChanged {
        path: String,
        state: nm::DeviceState,
        reason: u32,
    },
    Unavailable(String),
}

#[derive(Debug, Default)]
struct DeviceRecord {
    properties: nm::DeviceProperties,
    wireless: nm::WirelessProperties,
    wired: nm::WiredProperties,
}

pub struct Network {
    state: Publisher<NetworkState>,
    config: Config,
    generation: u64,
    adopted: bool,
    manager: nm::ManagerProperties,
    devices: BTreeMap<String, DeviceRecord>,
    access_points: BTreeMap<String, nm::AccessPointProperties>,
    actives: BTreeMap<String, nm::ActiveProperties>,
    profiles: BTreeMap<String, nm::Profile>,
    reasons: BTreeMap<String, u32>,
    busy: BTreeMap<String, Busy>,
    settings: std::collections::BTreeSet<String>,
    failures: BTreeMap<String, Failure>,
    vpn_states: BTreeMap<String, nm::VpnState>,
    scan: Option<u64>,
    scans: u64,
    deadline: Option<chrono::DateTime<chrono::Utc>>,
    agent: bool,
    secret: Option<(Request, oneshot::Sender<Answer>)>,
}

#[derive(Clone)]
pub struct NetworkHandle(crate::ServiceEndpoint<Network>);

impl NetworkHandle {
    pub fn snapshot(&self) -> NetworkState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<NetworkState> {
        self.0.subscribe()
    }

    pub fn health(&self) -> tokio::sync::watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    pub async fn set_networking_enabled(&self, enabled: bool) -> Result<(), NetworkError> {
        self.call(|reply| Command::SetNetworkingEnabled { enabled, reply })
            .await
    }

    pub async fn set_wifi_enabled(&self, enabled: bool) -> Result<(), NetworkError> {
        self.call(|reply| Command::SetWifiEnabled { enabled, reply })
            .await
    }

    pub async fn start_scan(&self) -> Result<(), NetworkError> {
        self.call(|reply| Command::StartScan { reply }).await
    }

    pub async fn stop_scan(&self) -> Result<(), NetworkError> {
        self.call(|reply| Command::StopScan { reply }).await
    }

    pub async fn connect_access_point(
        &self,
        id: NetworkId,
        secret: Option<Secret>,
    ) -> Result<(), NetworkError> {
        self.call(|reply| Command::ConnectAccessPoint { id, secret, reply })
            .await
    }

    pub async fn connect_hidden(
        &self,
        ssid: String,
        security: nm::Security,
        secret: Option<Secret>,
    ) -> Result<(), NetworkError> {
        self.call(|reply| Command::ConnectHidden {
            ssid,
            security,
            secret,
            reply,
        })
        .await
    }

    pub async fn connect_profile(&self, id: NetworkId) -> Result<(), NetworkError> {
        self.call(|reply| Command::ConnectProfile { id, reply })
            .await
    }

    pub async fn disconnect(&self, id: NetworkId) -> Result<(), NetworkError> {
        self.call(|reply| Command::Disconnect { id, reply }).await
    }

    pub async fn forget(&self, id: NetworkId) -> Result<(), NetworkError> {
        self.call(|reply| Command::Forget { id, reply }).await
    }

    pub async fn set_autoconnect(
        &self,
        id: NetworkId,
        autoconnect: bool,
    ) -> Result<(), NetworkError> {
        self.call(|reply| Command::SetAutoconnect {
            id,
            autoconnect,
            reply,
        })
        .await
    }

    pub async fn answer_secret(&self, answer: Answer) -> Result<(), NetworkError> {
        self.call(|reply| Command::AnswerSecret { answer, reply })
            .await
    }

    pub async fn connect_vpn(&self, id: NetworkId) -> Result<(), NetworkError> {
        self.call(|reply| Command::ConnectVpn { id, reply }).await
    }

    pub async fn disconnect_vpn(&self, id: NetworkId) -> Result<(), NetworkError> {
        self.call(|reply| Command::DisconnectVpn { id, reply })
            .await
    }

    async fn call(&self, command: impl FnOnce(Reply) -> Command) -> Result<(), NetworkError> {
        let (reply, result) = oneshot::channel();
        self.0.command(command(reply))?;
        result.await.map_err(|_| {
            CommandError::Unavailable("network stopped before completing the command".to_owned())
        })?
    }
}

fn wanted(device: &nm::DeviceProperties) -> bool {
    device.managed.unwrap_or(false) && device.kind.user_facing()
}

/// Which of two beacons for one SSID the list keeps. The connected one wins even when it is the
/// weaker: the access point in use is often not the strongest in range, and a list that dropped it
/// would report no connection at all while NetworkManager still has one.
fn keeps(held: &Access, point: &Access) -> bool {
    (held.active, held.strength) >= (point.active, point.strength)
}

fn strongest(points: impl Iterator<Item = Access>) -> Vec<Access> {
    let mut best: BTreeMap<String, Access> = BTreeMap::new();
    let mut unnamed: Vec<Access> = Vec::new();

    for point in points {
        match point.ssid.clone() {
            Some(ssid) => match best.get(&ssid) {
                Some(held) if keeps(held, &point) => {}
                _ => {
                    best.insert(ssid, point);
                }
            },
            None => unnamed.push(point),
        }
    }

    let mut all: Vec<Access> = best.into_values().chain(unnamed).collect();
    all.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then(b.strength.cmp(&a.strength))
            .then(a.ssid.cmp(&b.ssid))
    });
    all
}

impl Service for Network {
    const NAME: &'static str = "network";
    type Config = Config;
    type State = NetworkState;
    type Handle = NetworkHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Watch;

    fn from_endpoint(endpoint: crate::ServiceEndpoint<Self>) -> Self::Handle {
        NetworkHandle(endpoint)
    }

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let mut subs = vec![
            Sub::stream(Watch::NameOwner, source::name_owner),
            Sub::stream(Watch::Objects(self.generation), source::objects),
            Sub::stream(Watch::Properties(self.generation), source::properties),
            Sub::stream(Watch::ActiveStates(self.generation), source::active_states),
            Sub::stream(Watch::DeviceStates(self.generation), source::device_states),
            Sub::stream(
                Watch::ProfileUpdates(self.generation),
                source::profile_updates,
            ),
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
        let _ = ctx;
        Ok(Self {
            state: ctx.publisher(),
            config,
            generation: 0,
            adopted: false,
            manager: nm::ManagerProperties::default(),
            devices: BTreeMap::new(),
            access_points: BTreeMap::new(),
            actives: BTreeMap::new(),
            profiles: BTreeMap::new(),
            reasons: BTreeMap::new(),
            busy: BTreeMap::new(),
            settings: std::collections::BTreeSet::new(),
            failures: BTreeMap::new(),
            vpn_states: BTreeMap::new(),
            scan: None,
            scans: 0,
            deadline: None,
            agent: false,
            secret: None,
        })
    }

    async fn stop(self, ctx: &Ctx<Self>) {
        if let (true, Ok(connection)) = (self.agent, ctx.system_bus()) {
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
                let paths = self.adopt(*objects);
                ctx.running();
                self.publish();
                self.load_profiles(ctx, paths);
                if !self.agent {
                    self.reregister(ctx);
                }
            }
            Input::Event(Event::Profiles(profiles)) => {
                self.profiles = *profiles;
                self.publish();
            }
            Input::Event(Event::ProfilesChanged) => {
                self.load_profiles(ctx, self.profile_paths());
            }
            Input::Event(Event::Settled { key }) => {
                self.busy.remove(&key);
                self.publish();
            }
            Input::Event(Event::AgentRegistered(registered)) => self.agent = registered,
            Input::Event(Event::ScanExpired(scan)) => {
                if self.scan == Some(scan) {
                    self.halt(ctx);
                }
            }
            Input::Event(Event::Secrets(request, answer)) => {
                if self.secret.is_some() {
                    tracing::warn!(
                        network = %request.name,
                        "a second secret request arrived while one was open; refusing it"
                    );
                    let _ = answer.send(Answer::Refused);
                    return;
                }
                self.secret = Some((request, answer));
                self.publish();
            }
            Input::Event(Event::SecretsCancelled) => {
                if let Some((_, answer)) = self.secret.take() {
                    let _ = answer.send(Answer::Refused);
                }
                self.publish();
            }
            Input::Event(Event::NameOwner(owner)) => match owner {
                Some(_) => {
                    self.generation = self.generation.wrapping_add(1);
                    self.adopted = false;
                    self.agent = false;
                    self.scan = None;
                    self.deadline = None;
                    self.forget_everything();
                    self.publish();
                    self.reregister(ctx);
                }
                None => {
                    self.agent = false;
                    self.scan = None;
                    self.deadline = None;
                    if let Some((_, answer)) = self.secret.take() {
                        let _ = answer.send(Answer::Refused);
                    }
                    self.forget_everything();
                    self.publish();
                    ctx.degraded("NetworkManager left the bus");
                }
            },
            Input::Event(Event::InterfacesAdded { path, interfaces }) => {
                let settings = interfaces.contains_key(nm::SETTINGS_CONNECTION1);
                self.absorb(&path, &interfaces);
                self.publish();
                if settings {
                    self.load_profiles(ctx, self.profile_paths());
                }
            }
            Input::Event(Event::InterfacesRemoved { path, interfaces }) => {
                let settings = interfaces.iter().any(|one| one == nm::SETTINGS_CONNECTION1);
                self.remove(&path, &interfaces);
                if settings {
                    self.profiles.remove(&path);
                    self.settings.remove(&path);
                }
                self.publish();
            }
            Input::Event(Event::PropertiesChanged {
                path,
                interface,
                changed,
                invalidated,
            }) => {
                if !relevant(&changed, &invalidated) {
                    return;
                }
                self.merge(&path, &interface, &changed);
                if self.scan.is_some() && !self.wireless_usable() {
                    self.halt(ctx);
                }
                self.publish();
            }
            Input::Event(Event::ActiveStateChanged {
                path,
                state,
                reason,
            }) => {
                let name = self.actives.get(&path).and_then(|active| active.id.clone());
                match state {
                    nm::ActiveState::Deactivated => {
                        if let (Some(name), Err(failure)) = (name, failure::from_active(reason)) {
                            self.failures.insert(name, failure);
                        }
                        self.reasons.remove(&path);
                        self.actives.remove(&path);
                    }
                    _ => {
                        self.reasons.insert(path.clone(), reason);
                        if let (Some(name), nm::ActiveState::Activated) = (&name, state) {
                            self.failures.remove(name);
                        }
                        if let Some(active) = self.actives.get_mut(&path) {
                            active.state = state;
                        }
                    }
                }
                self.publish();
            }
            Input::Event(Event::DeviceStateChanged {
                path,
                state,
                reason,
            }) => {
                let wireless = self
                    .devices
                    .get(&path)
                    .is_some_and(|record| record.properties.kind == nm::DeviceKind::Wifi);
                if let Some(device) = self.devices.get_mut(&path) {
                    device.properties.state = state;
                    device.properties.reason = reason;
                }
                if state == nm::DeviceState::Failed
                    && let Err(failure) = failure::from_device(reason, wireless)
                    && let Some(ssid) = self.attempted_ssid(&path)
                {
                    self.failures.insert(ssid, failure);
                }
                if state == nm::DeviceState::Activated
                    && let Some(ssid) = self.attempted_ssid(&path)
                {
                    self.failures.remove(&ssid);
                }
                self.publish();
            }
            Input::Event(Event::Unavailable(reason)) => {
                ctx.degraded(reason);
            }
        }
    }
}

impl Network {
    fn forget_everything(&mut self) {
        self.manager = nm::ManagerProperties::default();
        self.devices.clear();
        self.access_points.clear();
        self.actives.clear();
        self.profiles.clear();
        self.reasons.clear();
        self.busy.clear();
        self.settings.clear();
        self.failures.clear();
        self.vpn_states.clear();
    }

    fn reregister(&self, ctx: &Ctx<Self>) {
        let Ok(connection) = ctx.system_bus().cloned() else {
            return;
        };
        ctx.spawn_detached(move |ctx| async move {
            let registered = match agent::register(&connection, ctx.events()).await {
                Ok(()) => true,
                Err(error) => {
                    tracing::warn!(%error, "could not register the network secret agent");
                    false
                }
            };
            let _ = ctx
                .events()
                .send(Input::Event(Event::AgentRegistered(registered)))
                .await;
        });
    }

    fn adopt(&mut self, objects: Objects) -> Vec<String> {
        self.forget_everything();
        for (path, interfaces) in objects {
            self.absorb(&path, &interfaces);
        }
        self.profile_paths()
    }

    fn profile_paths(&self) -> Vec<String> {
        self.settings.iter().cloned().collect()
    }

    fn load_profiles(&self, ctx: &Ctx<Self>, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        let Ok(connection) = ctx.system_bus().cloned() else {
            return;
        };
        ctx.spawn_detached(move |ctx| async move {
            let mut profiles = BTreeMap::new();
            for path in paths {
                let built =
                    match nm::SettingsConnectionProxy::builder(&connection).path(path.clone()) {
                        Ok(builder) => builder.build().await,
                        Err(error) => Err(error),
                    };
                let Ok(proxy) = built else { continue };
                if let Ok(settings) = proxy.get_settings().await {
                    profiles.insert(path, nm::decode_profile(&settings));
                }
            }
            let _ = ctx
                .events()
                .send(Input::Event(Event::Profiles(Box::new(profiles))))
                .await;
        });
    }

    fn absorb(&mut self, path: &str, interfaces: &Interfaces) {
        for (name, properties) in interfaces {
            match name.as_str() {
                nm::MANAGER1 if path == nm::MANAGER => {
                    self.manager = nm::decode_manager(properties);
                }
                nm::DEVICE1 => {
                    let decoded = nm::decode_device(properties);
                    self.devices.entry(path.to_owned()).or_default().properties = decoded;
                }
                nm::WIRELESS1 => {
                    let decoded = nm::decode_wireless(properties);
                    self.devices.entry(path.to_owned()).or_default().wireless = decoded;
                }
                nm::WIRED1 => {
                    let decoded = nm::decode_wired(properties);
                    self.devices.entry(path.to_owned()).or_default().wired = decoded;
                }
                nm::ACCESS_POINT1 => {
                    self.access_points
                        .insert(path.to_owned(), nm::decode_access_point(properties));
                }
                nm::ACTIVE1 => {
                    self.actives
                        .insert(path.to_owned(), nm::decode_active(properties));
                }
                nm::SETTINGS_CONNECTION1 => {
                    self.settings.insert(path.to_owned());
                }
                _ => {}
            }
        }
    }

    fn remove(&mut self, path: &str, interfaces: &[String]) {
        for name in interfaces {
            match name.as_str() {
                nm::DEVICE1 => {
                    self.devices.remove(path);
                }
                nm::ACCESS_POINT1 => {
                    self.access_points.remove(path);
                }
                nm::ACTIVE1 => {
                    self.actives.remove(path);
                    self.reasons.remove(path);
                }
                _ => {}
            }
        }
    }

    fn merge(&mut self, path: &str, interface: &str, changed: &Properties) {
        match interface {
            nm::MANAGER1 if path == nm::MANAGER => {
                let merged = nm::decode_manager(changed);
                let held = &mut self.manager;
                if changed.contains_key("NetworkingEnabled") {
                    held.networking_enabled = merged.networking_enabled;
                }
                if changed.contains_key("WirelessEnabled") {
                    held.wireless_enabled = merged.wireless_enabled;
                }
                if changed.contains_key("WirelessHardwareEnabled") {
                    held.wireless_hardware_enabled = merged.wireless_hardware_enabled;
                }
                if changed.contains_key("Connectivity") {
                    held.connectivity = merged.connectivity;
                }
                if changed.contains_key("Metered") {
                    held.metered = merged.metered;
                }
                if changed.contains_key("PrimaryConnection") {
                    held.primary = merged.primary;
                }
                if changed.contains_key("Devices") {
                    held.devices = merged.devices;
                }
                if changed.contains_key("ActiveConnections") {
                    held.active = merged.active;
                }
            }
            nm::DEVICE1 => {
                if let Some(device) = self.devices.get_mut(path) {
                    let merged = nm::decode_device(changed);
                    if changed.contains_key("State") {
                        device.properties.state = merged.state;
                    }
                    if changed.contains_key("StateReason") {
                        device.properties.reason = merged.reason;
                    }
                    if changed.contains_key("Managed") {
                        device.properties.managed = merged.managed;
                    }
                    if changed.contains_key("Metered") {
                        device.properties.metered = merged.metered;
                    }
                    if changed.contains_key("ActiveConnection") {
                        device.properties.active = merged.active;
                    }
                }
            }
            nm::WIRELESS1 => {
                if let Some(device) = self.devices.get_mut(path) {
                    let merged = nm::decode_wireless(changed);
                    if changed.contains_key("AccessPoints") {
                        device.wireless.access_points = merged.access_points;
                    }
                    if changed.contains_key("ActiveAccessPoint") {
                        device.wireless.active_access_point = merged.active_access_point;
                    }
                }
            }
            nm::WIRED1 => {
                if let Some(device) = self.devices.get_mut(path) {
                    let merged = nm::decode_wired(changed);
                    if changed.contains_key("Carrier") {
                        device.wired.carrier = merged.carrier;
                    }
                    if changed.contains_key("Speed") {
                        device.wired.speed = merged.speed;
                    }
                }
            }
            nm::ACCESS_POINT1 => {
                if let Some(point) = self.access_points.get_mut(path) {
                    let merged = nm::decode_access_point(changed);
                    if changed.contains_key("Strength") {
                        point.strength = merged.strength;
                    }
                    if changed.contains_key("Ssid") {
                        point.ssid = merged.ssid;
                    }
                }
            }
            nm::VPN1 => {
                if let Some(state) = changed.get("VpnState").and_then(|v| u32::try_from(v).ok()) {
                    self.vpn_states
                        .insert(path.to_owned(), nm::VpnState::from_code(state));
                }
            }
            nm::ACTIVE1 => {
                if let Some(active) = self.actives.get_mut(path) {
                    let merged = nm::decode_active(changed);
                    if changed.contains_key("State") {
                        active.state = merged.state;
                    }
                    if changed.contains_key("Default") {
                        active.default = merged.default;
                    }
                }
            }
            _ => {}
        }
    }

    fn wireless_usable(&self) -> bool {
        self.wireless_device().is_some()
            && self.manager.wireless_enabled.unwrap_or(false)
            && self.manager.wireless_hardware_enabled.unwrap_or(true)
    }

    fn wireless_device(&self) -> Option<(&String, &DeviceRecord)> {
        self.devices.iter().find(|(_, record)| {
            wanted(&record.properties) && record.properties.kind == nm::DeviceKind::Wifi
        })
    }

    fn build(&self) -> NetworkState {
        let wifi = self.wireless_device().map(|_| Radio {
            enabled: self.manager.wireless_enabled.unwrap_or(false),
            hardware_enabled: self.manager.wireless_hardware_enabled.unwrap_or(true),
        });

        let active_point = self
            .wireless_device()
            .and_then(|(_, record)| record.wireless.active_access_point.clone());

        let visible = self
            .wireless_device()
            .map(|(_, record)| record.wireless.access_points.clone())
            .unwrap_or_default();

        let networks = strongest(visible.iter().filter_map(|path| {
            let point = self.access_points.get(path)?;
            if self.config.hide_unnamed && point.ssid.is_none() {
                return None;
            }
            Some(Access {
                id: NetworkId::new(path.clone()),
                ssid: point.ssid.clone(),
                bssid: point.bssid.clone(),
                strength: point.strength,
                band: point.band(),
                security: point.security(),
                active: active_point.as_deref() == Some(path.as_str()),
                saved: self.profile_for(point.ssid.as_deref()),
                busy: self.busy.get(path).copied(),
                failure: point
                    .ssid
                    .as_deref()
                    .and_then(|ssid| self.failures.get(ssid).copied()),
            })
        }));

        let wired = self
            .devices
            .iter()
            .filter(|(_, record)| {
                wanted(&record.properties) && record.properties.kind == nm::DeviceKind::Ethernet
            })
            .map(|(path, record)| Wired {
                id: NetworkId::new(path.clone()),
                name: record
                    .properties
                    .interface
                    .clone()
                    .unwrap_or_else(|| "ethernet".to_owned()),
                carrier: record.wired.carrier.unwrap_or(false),
                speed: record.wired.speed,
                active: record.properties.state == nm::DeviceState::Activated,
                busy: self.busy.get(path).copied(),
            })
            .collect();

        let vpn = self
            .profiles
            .iter()
            .filter(|(_, profile)| {
                self.config.show_vpn
                    && matches!(profile.kind.as_deref(), Some("vpn") | Some("wireguard"))
            })
            .map(|(path, profile)| {
                let active = self
                    .actives
                    .values()
                    .find(|active| active.connection.as_deref() == Some(path.as_str()));
                let state = self
                    .vpn_states
                    .get(path)
                    .copied()
                    .unwrap_or(nm::VpnState::Unknown);
                Vpn {
                    id: NetworkId::new(path.clone()),
                    name: profile.id.clone().unwrap_or_else(|| "VPN".to_owned()),
                    kind: profile.kind.clone().unwrap_or_default(),
                    state,
                    active: state == nm::VpnState::Activated
                        || active.is_some_and(|active| active.state == nm::ActiveState::Activated),
                    failure: failure::from_vpn(state).err(),
                    busy: self.busy.get(path).copied(),
                }
            })
            .collect();

        let seen: Vec<Option<String>> = networks.iter().map(|one| one.ssid.clone()).collect();
        let known = self
            .profiles
            .iter()
            .filter(|(_, profile)| profile.kind.as_deref() == Some("802-11-wireless"))
            .map(|(path, profile)| Saved {
                id: NetworkId::new(path.clone()),
                name: profile.id.clone(),
                kind: profile.kind.clone().unwrap_or_default(),
                uuid: profile.uuid.clone(),
                autoconnect: profile.autoconnect,
                in_range: seen.iter().any(|ssid| ssid == &profile.ssid),
                active: self
                    .actives
                    .values()
                    .any(|active| active.connection.as_deref() == Some(path.as_str())),
                busy: self.busy.get(path).copied(),
            })
            .collect();

        NetworkState {
            networking: self.manager.networking_enabled.unwrap_or(false),
            wifi,
            connectivity: self.manager.connectivity,
            metered: self.manager.metered,
            primary: self.manager.primary.clone(),
            networks,
            known,
            wired,
            vpn,
            scanning: self.scan.is_some(),
            secret: self.secret.as_ref().map(|(request, _)| request.clone()),
        }
    }

    fn attempted_ssid(&self, device: &str) -> Option<String> {
        let record = self.devices.get(device)?;
        record
            .wireless
            .active_access_point
            .as_ref()
            .and_then(|path| self.access_points.get(path))
            .and_then(|point| point.ssid.clone())
    }

    fn profile_of(&self, id: &NetworkId) -> Option<String> {
        let key = id.as_str();
        if self.profiles.contains_key(key) {
            return Some(key.to_owned());
        }
        let point = self.access_points.get(key)?;
        self.profile_for(point.ssid.as_deref())
            .map(|one| one.as_str().to_owned())
    }

    fn profile_for(&self, ssid: Option<&str>) -> Option<NetworkId> {
        let ssid = ssid?;
        self.profiles
            .iter()
            .filter(|(_, profile)| profile.ssid.as_deref() == Some(ssid))
            .max_by_key(|(path, profile)| (profile.timestamp, std::cmp::Reverse((*path).clone())))
            .map(|(path, _)| NetworkId::new(path.clone()))
    }

    fn publish(&self) {
        if !self.adopted {
            return;
        }
        self.state.set(self.build());
    }

    fn bus(ctx: &Ctx<Self>) -> Result<zbus::Connection, NetworkError> {
        ctx.system_bus()
            .cloned()
            .map_err(|reason| NetworkError::Unavailable(reason.to_owned()))
    }

    fn mark(&mut self, key: &str, busy: Busy) {
        self.busy.insert(key.to_owned(), busy);
    }

    fn dispatch<F, Fut>(&mut self, ctx: &Ctx<Self>, key: Option<String>, reply: Reply, work: F)
    where
        F: FnOnce(zbus::Connection) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), NetworkError>> + Send + 'static,
    {
        let connection = match Self::bus(ctx) {
            Ok(connection) => connection,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        ctx.spawn_detached(move |ctx| async move {
            let outcome = work(connection).await;
            if let Some(key) = key {
                let _ = ctx
                    .events()
                    .send(Input::Event(Event::Settled { key }))
                    .await;
            }
            let _ = reply.send(outcome);
        });
    }

    async fn run(&mut self, ctx: &Ctx<Self>, command: Command) {
        match command {
            Command::SetNetworkingEnabled { enabled, reply } => {
                self.dispatch(ctx, None, reply, move |connection| async move {
                    settle(
                        Action::Radio,
                        call::set_networking(&connection, enabled).await,
                    )
                });
            }
            Command::SetWifiEnabled { enabled, reply } => {
                self.dispatch(ctx, None, reply, move |connection| async move {
                    settle(Action::Radio, call::set_wifi(&connection, enabled).await)
                });
            }
            Command::ConnectAccessPoint { id, secret, reply } => {
                let Some(device) = self.wireless_device().map(|(path, _)| path.clone()) else {
                    let _ = reply.send(Err(NetworkError::Failed(Failure::NoDevice)));
                    return;
                };
                let Some(point) = self.access_points.get(id.as_str()).cloned() else {
                    let _ = reply.send(Err(NetworkError::Failed(Failure::NotFound)));
                    return;
                };
                let saved = self
                    .profile_for(point.ssid.as_deref())
                    .map(|one| one.as_str().to_owned());
                if saved.is_none() && !call::joinable(point.security()) {
                    let _ = reply.send(Err(NetworkError::Failed(Failure::ConfigFailed)));
                    return;
                }
                let specific = id.as_str().to_owned();
                if let Some(ssid) = point.ssid.clone() {
                    self.failures.remove(&ssid);
                }
                self.mark(id.as_str(), Busy::Connecting);
                self.publish();
                let raw = point.ssid.clone().unwrap_or_default().into_bytes();
                let security = point.security();
                let key = id.as_str().to_owned();
                self.dispatch(ctx, Some(key), reply, move |connection| async move {
                    let outcome = match saved {
                        Some(saved) => {
                            call::activate(&connection, &saved, &device, &specific).await
                        }
                        None => {
                            call::add_and_activate(
                                &connection,
                                &raw,
                                security,
                                false,
                                secret.as_ref().map(|one| one.as_str()),
                                &device,
                                &specific,
                            )
                            .await
                        }
                    };
                    settle(Action::Connect, outcome)
                });
            }
            Command::ConnectHidden {
                ssid,
                security,
                secret,
                reply,
            } => {
                let Some(device) = self.wireless_device().map(|(path, _)| path.clone()) else {
                    let _ = reply.send(Err(NetworkError::Failed(Failure::NoDevice)));
                    return;
                };
                let raw = ssid.into_bytes();
                self.dispatch(ctx, None, reply, move |connection| async move {
                    settle(
                        Action::Hidden,
                        call::add_and_activate(
                            &connection,
                            &raw,
                            security,
                            true,
                            secret.as_ref().map(|one| one.as_str()),
                            &device,
                            "/",
                        )
                        .await,
                    )
                });
            }
            Command::ConnectProfile { id, reply } => {
                let device = self
                    .wireless_device()
                    .map(|(path, _)| path.clone())
                    .unwrap_or_else(|| "/".to_owned());
                let saved = id.as_str().to_owned();
                self.mark(id.as_str(), Busy::Connecting);
                self.publish();
                let key = id.as_str().to_owned();
                self.dispatch(ctx, Some(key), reply, move |connection| async move {
                    settle(
                        Action::Connect,
                        call::activate(&connection, &saved, &device, "/").await,
                    )
                });
            }
            Command::Disconnect { id, reply } => {
                let Some(active) = self.active_for(&id) else {
                    let _ = reply.send(Ok(()));
                    return;
                };
                self.mark(id.as_str(), Busy::Disconnecting);
                self.publish();
                let key = id.as_str().to_owned();
                self.dispatch(ctx, Some(key), reply, move |connection| async move {
                    settle(
                        Action::Disconnect,
                        call::deactivate(&connection, &active).await,
                    )
                });
            }
            Command::Forget { id, reply } => {
                let Some(saved) = self.profile_of(&id) else {
                    let _ = reply.send(Err(NetworkError::Failed(Failure::NotFound)));
                    return;
                };
                self.mark(&saved, Busy::Forgetting);
                self.publish();
                let key = saved.clone();
                self.dispatch(ctx, Some(key), reply, move |connection| async move {
                    settle(Action::Forget, call::forget(&connection, &saved).await)
                });
            }
            Command::SetAutoconnect {
                id,
                autoconnect,
                reply,
            } => {
                let Some(saved) = self.profile_of(&id) else {
                    let _ = reply.send(Err(NetworkError::Failed(Failure::NotFound)));
                    return;
                };
                self.dispatch(ctx, None, reply, move |connection| async move {
                    settle(
                        Action::Autoconnect,
                        call::set_autoconnect(&connection, &saved, autoconnect).await,
                    )
                });
            }
            Command::StartScan { reply } => {
                let Some(device) = self
                    .wireless_usable()
                    .then(|| self.wireless_device().map(|(path, _)| path.clone()))
                    .flatten()
                else {
                    let _ = reply.send(Err(NetworkError::Failed(Failure::NoDevice)));
                    return;
                };
                self.scans = self.scans.wrapping_add(1);
                self.scan = Some(self.scans);
                self.deadline = self.config.scan_timeout.and_then(|timeout| {
                    chrono::Utc::now().checked_add_signed(
                        chrono::TimeDelta::from_std(timeout).unwrap_or_default(),
                    )
                });
                self.publish();
                self.dispatch(ctx, None, reply, move |connection| async move {
                    settle(Action::Scan, call::scan(&connection, &device).await)
                });
            }
            Command::StopScan { reply } => {
                self.scan = None;
                self.deadline = None;
                self.publish();
                let _ = reply.send(Ok(()));
            }
            Command::AnswerSecret { answer, reply } => {
                match self.secret.take() {
                    Some((_, sender)) => {
                        let _ = sender.send(answer);
                        let _ = reply.send(Ok(()));
                    }
                    None => {
                        let _ = reply.send(Err(NetworkError::Failed(Failure::Unknown)));
                    }
                }
                self.publish();
            }
            Command::ConnectVpn { id, reply } => {
                let saved = id.as_str().to_owned();
                self.mark(id.as_str(), Busy::Connecting);
                self.publish();
                let key = id.as_str().to_owned();
                self.dispatch(ctx, Some(key), reply, move |connection| async move {
                    settle(
                        Action::Connect,
                        call::activate_vpn(&connection, &saved).await,
                    )
                });
            }
            Command::DisconnectVpn { id, reply } => {
                let Some(active) = self.active_for(&id) else {
                    let _ = reply.send(Ok(()));
                    return;
                };
                self.mark(id.as_str(), Busy::Disconnecting);
                self.publish();
                let key = id.as_str().to_owned();
                self.dispatch(ctx, Some(key), reply, move |connection| async move {
                    settle(
                        Action::Disconnect,
                        call::deactivate(&connection, &active).await,
                    )
                });
            }
        }
    }

    fn halt(&mut self, _ctx: &Ctx<Self>) {
        self.scan = None;
        self.deadline = None;
        self.publish();
    }

    fn active_for(&self, id: &NetworkId) -> Option<String> {
        self.actives
            .iter()
            .find(|(_, active)| {
                active.connection.as_deref() == Some(id.as_str())
                    || active.specific_object.as_deref() == Some(id.as_str())
            })
            .map(|(path, _)| path.clone())
    }
}

#[cfg(test)]
mod tests;
