mod call;
mod source;

use std::collections::HashSet;

use tokio::sync::oneshot;

use glimpse_dbus::kdeconnect::DeviceProperties;
pub use glimpse_dbus::kdeconnect::{BatteryProperties as Battery, DeviceType, PairState};

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
        Command::Act { reply, .. } | Command::Share { reply, .. } | Command::Discover { reply } => {
            reply
        }
    };
    let _ = reply.send(Err(CommandError::Unavailable(reason.to_owned())));
}

fn failed(error: zbus::Error) -> CommandError {
    tracing::warn!(%error, "a kdeconnect command was refused");
    CommandError::Unavailable(error.to_string())
}

#[cfg(test)]
mod tests {
    use glimpse_dbus::Buses;
    use tokio_util::sync::CancellationToken;

    use super::*;

    const OWNER: &str = ":1.74";

    async fn kdeconnect() -> (
        Kdeconnect,
        Ctx<Kdeconnect>,
        tokio::sync::watch::Receiver<KdeconnectState>,
        tokio::sync::watch::Receiver<crate::ServiceState>,
    ) {
        let cancel = CancellationToken::new();
        let (events, _inbox) = tokio::sync::mpsc::channel(8);
        let (state, state_rx) = tokio::sync::watch::channel(KdeconnectState::default());
        let (health, health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
        let ctx = Ctx::<Kdeconnect>::new(
            events,
            &cancel,
            state,
            health,
            Buses::unavailable("no bus in tests"),
        );
        let service = Kdeconnect::start(&ctx, NoConfig, ()).await.expect("starts");
        (service, ctx, state_rx, health_rx)
    }

    fn phone(id: &str, reachable: bool, pair: PairState) -> Device {
        Device::project(
            DeviceId::new(id),
            DeviceProperties {
                name: Some(format!("Phone {id}")),
                kind: DeviceType::Phone,
                reachable,
                pair,
            },
            Some(Battery {
                charge: Some(76),
                charging: false,
                low: false,
            }),
            Actions {
                ring: true,
                ping: true,
                send_clipboard: false,
                share: true,
                browse: true,
                messages: true,
            },
        )
    }

    fn enumerated(generation: u64, devices: Vec<Device>) -> Event {
        Event::Enumerated {
            generation,
            devices,
        }
    }

    async fn event(service: &mut Kdeconnect, ctx: &Ctx<Kdeconnect>, event: Event) {
        service.handle(ctx, Input::Event(event)).await;
    }

    fn keys(service: &Kdeconnect) -> Vec<Watch> {
        service
            .subscriptions()
            .iter()
            .map(|sub| match sub.key() {
                Watch::NameOwner => Watch::NameOwner,
                Watch::Devices(generation) => Watch::Devices(*generation),
            })
            .collect()
    }

    #[tokio::test]
    async fn before_the_first_event_nothing_is_running_and_only_the_owner_is_watched() {
        let (service, _ctx, state, _health) = kdeconnect().await;

        assert_eq!(*state.borrow(), KdeconnectState::default());
        assert_eq!(keys(&service), vec![Watch::NameOwner]);
    }

    #[tokio::test]
    async fn an_owner_starts_the_device_source_and_publishes_running() {
        let (mut service, ctx, state, health) = kdeconnect().await;

        event(&mut service, &ctx, Event::NameOwner(Some(OWNER.to_owned()))).await;

        assert!(state.borrow().running);
        assert_eq!(*health.borrow(), crate::ServiceState::Running);
        assert_eq!(keys(&service), vec![Watch::NameOwner, Watch::Devices(1)]);
    }

    #[tokio::test]
    async fn no_owner_is_healthy_and_publishes_nothing_running() {
        let (mut service, ctx, state, health) = kdeconnect().await;

        event(&mut service, &ctx, Event::NameOwner(Some(OWNER.to_owned()))).await;
        event(
            &mut service,
            &ctx,
            enumerated(1, vec![phone("a", true, PairState::Paired)]),
        )
        .await;
        event(&mut service, &ctx, Event::NameOwner(None)).await;

        assert_eq!(*state.borrow(), KdeconnectState::default());
        assert_eq!(
            *health.borrow(),
            crate::ServiceState::Running,
            "no daemon is the normal state for most users, not a fault"
        );
        assert_eq!(keys(&service), vec![Watch::NameOwner]);
    }

    #[tokio::test]
    async fn a_restarted_daemon_is_a_new_generation() {
        let (mut service, ctx, _state, _health) = kdeconnect().await;

        event(&mut service, &ctx, Event::NameOwner(Some(OWNER.to_owned()))).await;
        event(
            &mut service,
            &ctx,
            Event::NameOwner(Some(":1.99".to_owned())),
        )
        .await;

        assert_eq!(keys(&service), vec![Watch::NameOwner, Watch::Devices(2)]);
    }

    #[tokio::test]
    async fn the_same_owner_twice_does_not_restart_the_source() {
        let (mut service, ctx, _state, _health) = kdeconnect().await;

        event(&mut service, &ctx, Event::NameOwner(Some(OWNER.to_owned()))).await;
        event(&mut service, &ctx, Event::NameOwner(Some(OWNER.to_owned()))).await;

        assert_eq!(keys(&service), vec![Watch::NameOwner, Watch::Devices(1)]);
    }

    #[tokio::test]
    async fn a_device_signal_replaces_inserts_and_removes_by_id() {
        let (mut service, ctx, state, _health) = kdeconnect().await;
        event(&mut service, &ctx, Event::NameOwner(Some(OWNER.to_owned()))).await;
        event(
            &mut service,
            &ctx,
            enumerated(
                1,
                vec![
                    phone("b", true, PairState::Paired),
                    phone("a", false, PairState::Paired),
                ],
            ),
        )
        .await;
        let ids = |state: &KdeconnectState| {
            state
                .devices
                .iter()
                .map(|device| device.id.as_str().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&state.borrow()), ["a", "b"]);

        event(
            &mut service,
            &ctx,
            Event::Device {
                generation: 1,
                id: DeviceId::new("a"),
                device: Some(phone("a", true, PairState::Paired)),
            },
        )
        .await;
        assert!(state.borrow().devices[0].reachable);

        event(
            &mut service,
            &ctx,
            Event::Device {
                generation: 1,
                id: DeviceId::new("c"),
                device: Some(phone("c", true, PairState::NotPaired)),
            },
        )
        .await;
        assert_eq!(ids(&state.borrow()), ["a", "b", "c"]);

        event(
            &mut service,
            &ctx,
            Event::Device {
                generation: 1,
                id: DeviceId::new("b"),
                device: None,
            },
        )
        .await;
        assert_eq!(ids(&state.borrow()), ["a", "c"]);
    }

    #[tokio::test]
    async fn a_dead_daemons_late_list_cannot_resurrect_its_devices() {
        let (mut service, ctx, state, health) = kdeconnect().await;
        event(&mut service, &ctx, Event::NameOwner(Some(OWNER.to_owned()))).await;
        event(&mut service, &ctx, Event::NameOwner(None)).await;

        event(
            &mut service,
            &ctx,
            enumerated(1, vec![phone("a", true, PairState::Paired)]),
        )
        .await;
        event(
            &mut service,
            &ctx,
            Event::Failed {
                generation: 1,
                reason: "gone".to_owned(),
            },
        )
        .await;

        assert_eq!(*state.borrow(), KdeconnectState::default());
        assert_eq!(*health.borrow(), crate::ServiceState::Running);
    }

    #[tokio::test]
    async fn a_burst_of_signals_is_one_fetch_per_device_and_one_list_at_a_time() {
        let (mut service, _ctx, _state, _health) = kdeconnect().await;
        let phone = || Stale::Device(DeviceId::new("a"));

        assert_eq!(service.plan(phone()), Some(phone()));
        for _ in 0..100 {
            assert_eq!(service.plan(phone()), None, "a fetch is already in flight");
        }
        assert!(service.fetches.refetch.contains(&DeviceId::new("a")));

        assert_eq!(service.plan(Stale::All), Some(Stale::All));
        assert_eq!(service.plan(Stale::All), None);
        assert!(
            service.fetches.relist,
            "the second list waits for the first"
        );
    }

    #[test]
    fn only_a_paired_reachable_device_carries_a_battery_and_actions() {
        let connected = phone("a", true, PairState::Paired);
        assert!(connected.battery.is_some());
        assert!(connected.actions.ring);

        for device in [
            phone("b", false, PairState::Paired),
            phone("c", true, PairState::NotPaired),
            phone("d", true, PairState::Requested),
        ] {
            assert_eq!(device.battery, None, "{}", device.id.as_str());
            assert_eq!(device.actions, Actions::default(), "{}", device.id.as_str());
        }
    }

    #[test]
    fn a_nameless_device_falls_back_to_its_id() {
        let device = Device::project(
            DeviceId::new("b98d"),
            DeviceProperties::default(),
            None,
            Actions::default(),
        );
        assert_eq!(device.name, "b98d");
    }

    #[tokio::test]
    async fn a_command_with_no_daemon_is_refused_as_unavailable() {
        let (mut service, ctx, _state, _health) = kdeconnect().await;
        let (reply, result) = oneshot::channel();

        service
            .handle(
                &ctx,
                Input::Command(Command::Act {
                    id: DeviceId::new("a"),
                    action: Action::Ring,
                    reply,
                }),
            )
            .await;

        assert!(matches!(
            result.await.expect("a reply"),
            Err(CommandError::Unavailable(_))
        ));
    }
}
