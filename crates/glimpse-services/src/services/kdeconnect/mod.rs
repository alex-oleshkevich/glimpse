mod call;
mod source;

use std::collections::HashSet;

use tokio::sync::oneshot;

use glimpse_dbus::kdeconnect::DeviceProperties;
pub use glimpse_dbus::kdeconnect::{BatteryProperties as Battery, DeviceType, PairState};
use glimpse_dbus::upower::{ChargeState, DeviceKind, WarningLevel};

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, NoConfig, Service, ServiceError},
    subscription::Sub,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Actions {
    pub ring: bool,
    pub ping: bool,
    pub send_clipboard: bool,
    pub share: bool,
    pub browse: bool,
    pub messages: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: DeviceId,
    pub name: String,
    pub kind: DeviceType,
    pub reachable: bool,
    pub pair: PairState,
    pub battery: Option<Battery>,
    pub actions: Actions,
}

impl Device {
    fn project(
        id: DeviceId,
        properties: DeviceProperties,
        battery: Option<Battery>,
        actions: Actions,
    ) -> Self {
        let paired_and_reachable = properties.reachable && properties.pair == PairState::Paired;
        Self {
            name: properties.name.unwrap_or_else(|| id.as_str().to_owned()),
            id,
            kind: properties.kind,
            reachable: properties.reachable,
            pair: properties.pair,
            battery: battery.filter(|_| paired_and_reachable),
            actions: match paired_and_reachable {
                true => actions,
                false => Actions::default(),
            },
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KdeconnectState {
    pub running: bool,
    pub devices: Vec<Device>,
}

impl KdeconnectState {
    pub fn device(&self, id: &DeviceId) -> Option<&Device> {
        self.devices.iter().find(|device| &device.id == id)
    }
}

pub fn batteries(state: &KdeconnectState) -> Vec<crate::BatteryPeripheral> {
    state
        .devices
        .iter()
        .filter_map(|device| {
            let battery = device.battery?;
            let percentage = battery.charge?;
            Some(crate::BatteryPeripheral {
                path: format!("kdeconnect:{}", device.id.as_str()),
                kind: match device.kind {
                    DeviceType::Phone => DeviceKind::Phone,
                    DeviceType::Tablet => DeviceKind::Tablet,
                    DeviceType::Desktop | DeviceType::Laptop => DeviceKind::Computer,
                    DeviceType::Tv | DeviceType::Unknown => DeviceKind::Unknown,
                },
                name: device.name.clone(),
                icon_name: None,
                charge: crate::Charge {
                    percentage,
                    state: match battery.charging {
                        true => ChargeState::Charging,
                        false => ChargeState::Discharging,
                    },
                    icon_name: String::new(),
                    time_to_empty: None,
                    time_to_full: None,
                    energy_rate_mw: None,
                    warning: match battery.low {
                        true => WarningLevel::Low,
                        false => WarningLevel::None,
                    },
                },
            })
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Watch {
    NameOwner,
    Devices(u64),
}

type Reply = oneshot::Sender<Result<(), CommandError>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Ring,
    Ping,
    SendClipboard,
    Browse,
    OpenMessages,
    Pair,
    Unpair,
}

pub enum Command {
    Act {
        id: DeviceId,
        action: Action,
        reply: Reply,
    },
    Share {
        id: DeviceId,
        urls: Vec<String>,
        reply: Reply,
    },
    ShareText {
        id: DeviceId,
        text: String,
        reply: Reply,
    },
    Discover {
        reply: Reply,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stale {
    All,
    Device(DeviceId),
}

pub enum Event {
    NameOwner(Option<String>),
    Stale {
        generation: u64,
        stale: Stale,
    },
    Enumerated {
        generation: u64,
        devices: Vec<Device>,
    },
    Device {
        generation: u64,
        id: DeviceId,
        device: Option<Device>,
    },
    Failed {
        generation: u64,
        reason: String,
    },
    Unavailable(String),
}

impl Event {
    fn generation(&self) -> Option<u64> {
        match self {
            Event::Stale { generation, .. }
            | Event::Enumerated { generation, .. }
            | Event::Device { generation, .. }
            | Event::Failed { generation, .. } => Some(*generation),
            Event::NameOwner(_) | Event::Unavailable(_) => None,
        }
    }
}

#[derive(Default)]
struct Fetches {
    listing: bool,
    relist: bool,
    fetching: HashSet<DeviceId>,
    refetch: HashSet<DeviceId>,
}

pub struct Kdeconnect {
    state: Publisher<KdeconnectState>,
    owner: Option<String>,
    generation: u64,
    devices: Vec<Device>,
    fetches: Fetches,
}

#[derive(Clone)]
pub struct KdeconnectHandle(crate::ServiceEndpoint<Kdeconnect>);

impl KdeconnectHandle {
    pub fn snapshot(&self) -> KdeconnectState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<KdeconnectState> {
        self.0.subscribe()
    }

    pub fn health(&self) -> tokio::sync::watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    pub async fn act(&self, id: DeviceId, action: Action) -> Result<(), CommandError> {
        self.call(|reply| Command::Act { id, action, reply }).await
    }

    pub async fn share(&self, id: DeviceId, urls: Vec<String>) -> Result<(), CommandError> {
        self.call(|reply| Command::Share { id, urls, reply }).await
    }

    pub async fn share_text(&self, id: DeviceId, text: String) -> Result<(), CommandError> {
        self.call(|reply| Command::ShareText { id, text, reply })
            .await
    }

    pub async fn discover(&self) -> Result<(), CommandError> {
        self.call(|reply| Command::Discover { reply }).await
    }

    async fn call(&self, command: impl FnOnce(Reply) -> Command) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(command(reply))?;
        result.await.map_err(|_| {
            CommandError::Unavailable("kdeconnect stopped before completing the command".to_owned())
        })?
    }
}

impl Service for Kdeconnect {
    const NAME: &'static str = "kdeconnect";
    type Config = NoConfig;
    type State = KdeconnectState;
    type Handle = KdeconnectHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Watch;

    fn from_endpoint(endpoint: crate::ServiceEndpoint<Self>) -> Self::Handle {
        KdeconnectHandle(endpoint)
    }

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let mut subs = vec![Sub::stream(Watch::NameOwner, source::name_owner)];
        if let Some(owner) = self.owner.clone() {
            let generation = self.generation;
            subs.push(Sub::stream(Watch::Devices(generation), move |ctx| {
                source::devices(ctx, owner, generation)
            }));
        }
        subs
    }

    async fn start(
        ctx: &Ctx<Self>,
        _: Self::Config,
        _: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            state: ctx.publisher(),
            owner: None,
            generation: 0,
            devices: Vec::new(),
            fetches: Fetches::default(),
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(command) => self.run(ctx, command),
            Input::Config(_) => {}
            Input::Event(Event::NameOwner(owner)) => {
                if owner == self.owner {
                    return;
                }
                self.generation = self.generation.wrapping_add(1);
                self.devices.clear();
                self.fetches = Fetches::default();
                self.owner = owner;
                ctx.running();
                self.publish();
            }
            Input::Event(event) if event.generation().is_some_and(|at| at != self.generation) => {}
            Input::Event(Event::Stale { stale, .. }) => self.refresh(ctx, stale),
            Input::Event(Event::Enumerated { devices, .. }) => {
                self.fetches.listing = false;
                self.devices = devices;
                self.devices.sort_by(|a, b| a.id.cmp(&b.id));
                ctx.running();
                self.publish();
                if std::mem::take(&mut self.fetches.relist) {
                    self.refresh(ctx, Stale::All);
                }
            }
            Input::Event(Event::Device { id, device, .. }) => {
                self.fetches.fetching.remove(&id);
                let at = self.devices.iter().position(|known| known.id == id);
                match (at, device) {
                    (Some(at), Some(device)) => self.devices[at] = device,
                    (Some(at), None) => {
                        self.devices.remove(at);
                    }
                    (None, Some(device)) => {
                        self.devices.push(device);
                        self.devices.sort_by(|a, b| a.id.cmp(&b.id));
                    }
                    (None, None) => {}
                }
                self.publish();
                if self.fetches.refetch.remove(&id) {
                    self.refresh(ctx, Stale::Device(id));
                }
            }
            Input::Event(Event::Failed { reason, .. }) => {
                self.fetches.listing = false;
                ctx.degraded(reason);
            }
            Input::Event(Event::Unavailable(reason)) => ctx.degraded(reason),
        }
    }
}

impl Kdeconnect {
    fn plan(&mut self, stale: Stale) -> Option<Stale> {
        match stale {
            Stale::All if self.fetches.listing => {
                self.fetches.relist = true;
                None
            }
            Stale::All => {
                self.fetches.listing = true;
                Some(Stale::All)
            }
            Stale::Device(id) if self.fetches.fetching.contains(&id) => {
                self.fetches.refetch.insert(id);
                None
            }
            Stale::Device(id) => {
                self.fetches.fetching.insert(id.clone());
                Some(Stale::Device(id))
            }
        }
    }

    fn refresh(&mut self, ctx: &Ctx<Self>, stale: Stale) {
        let (Some(owner), Ok(connection)) = (self.owner.clone(), ctx.session_bus()) else {
            return;
        };
        let connection = connection.clone();
        let generation = self.generation;
        match self.plan(stale) {
            None => {}
            Some(Stale::All) => ctx.spawn_detached(move |ctx| async move {
                let event = match source::enumerate(&connection, &owner).await {
                    Ok(devices) => Event::Enumerated {
                        generation,
                        devices,
                    },
                    Err(reason) => Event::Failed { generation, reason },
                };
                let _ = ctx.events().send(Input::Event(event)).await;
            }),
            Some(Stale::Device(id)) => ctx.spawn_detached(move |ctx| async move {
                let device = source::fetch(&connection, &owner, &id).await;
                let _ = ctx
                    .events()
                    .send(Input::Event(Event::Device {
                        generation,
                        id,
                        device,
                    }))
                    .await;
            }),
        }
    }

    fn publish(&mut self) {
        self.state.set(KdeconnectState {
            running: self.owner.is_some(),
            devices: self.devices.clone(),
        });
    }

    fn run(&mut self, ctx: &Ctx<Self>, command: Command) {
        let Some(owner) = self.owner.clone() else {
            return refuse(command, "kdeconnectd is not running");
        };
        let connection = match ctx.session_bus() {
            Ok(connection) => connection.clone(),
            Err(reason) => return refuse(command, reason),
        };
        match command {
            Command::Act { id, action, reply } => {
                if self.devices.iter().all(|device| device.id != id) {
                    let _ = reply.send(Err(CommandError::InvalidArgument(
                        "there is no such device".to_owned(),
                    )));
                    return;
                }
                ctx.spawn_detached(move |_| async move {
                    let outcome = call::act(&connection, &owner, &id, action).await;
                    let _ = reply.send(outcome.map_err(failed));
                });
            }
            Command::Share { id, urls, reply } => {
                ctx.spawn_detached(move |_| async move {
                    let outcome = call::share(&connection, &owner, &id, &urls).await;
                    let _ = reply.send(outcome.map_err(failed));
                });
            }
            Command::ShareText { id, text, reply } => {
                ctx.spawn_detached(move |_| async move {
                    let outcome = call::share_text(&connection, &owner, &id, &text).await;
                    let _ = reply.send(outcome.map_err(failed));
                });
            }
            Command::Discover { reply } => {
                ctx.spawn_detached(move |_| async move {
                    let outcome = call::discover(&connection, &owner).await;
                    let _ = reply.send(outcome.map_err(failed));
                });
            }
        }
    }
}

fn refuse(command: Command, reason: &str) {
    let reply = match command {
        Command::Act { reply, .. }
        | Command::Share { reply, .. }
        | Command::ShareText { reply, .. }
        | Command::Discover { reply } => reply,
    };
    let _ = reply.send(Err(CommandError::Unavailable(reason.to_owned())));
}

fn failed(error: zbus::Error) -> CommandError {
    tracing::warn!(%error, "a kdeconnect command was refused");
    CommandError::Unavailable(error.to_string())
}

#[cfg(test)]
mod tests;
