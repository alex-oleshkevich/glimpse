use std::pin::Pin;

use futures_util::{Stream, StreamExt, stream};
use glimpse_dbus::login1::{
    Login1InhibitorEntry, Login1ManagerProxy, Login1SessionProxy, session_path,
};
use glimpse_dbus::session::SessionProxy;
use glimpse_dbus::systemd1::{Systemd1ManagerProxy, Systemd1UnitProxy};
use glimpse_dbus::{DEADLINE, wake};
use glimpse_utils::clean;
use tokio::sync::oneshot;
use zbus::proxy::CacheProperties;

use crate::{
    CommandError, CompositorHandle, Ctx, Input, NoConfig, Publisher, Service, ServiceEndpoint, Sub,
    say,
};

const INHIBITOR_MAX: usize = 8;
const TEXT_MAX: usize = 128;

type Events = Pin<Box<dyn Stream<Item = Event> + Send>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionActionsState {
    pub user: Option<String>,
    pub signed_in_seconds: Option<u64>,
    pub windows: Option<usize>,
    pub inhibitors: Vec<Inhibitor>,
    pub suspend: Capability,
    pub hibernate: Capability,
    pub reboot: Capability,
    pub power_off: Capability,
}

impl Default for SessionActionsState {
    fn default() -> Self {
        Self {
            user: None,
            signed_in_seconds: None,
            windows: None,
            inhibitors: Vec::new(),
            suspend: Capability::Hidden,
            hibernate: Capability::Hidden,
            reboot: Capability::Hidden,
            power_off: Capability::Hidden,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inhibitor {
    pub what: String,
    pub who: String,
    pub why: String,
    pub mode: String,
}

impl Inhibitor {
    pub fn blocks(&self, what: &str) -> bool {
        matches!(self.mode.as_str(), "block" | "block-weak")
            && self.what.split(':').any(|held| held == what)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Hidden,
    Available,
    Blocked,
}

impl Capability {
    pub fn visible(self) -> bool {
        !matches!(self, Self::Hidden)
    }

    pub fn enabled(self) -> bool {
        matches!(self, Self::Available)
    }
}

pub fn capability(value: &str) -> Capability {
    match value {
        "yes" | "challenge" => Capability::Available,
        "inhibited" | "inhibitor-blocked" | "challenge-inhibitor-blocked" => Capability::Blocked,
        _ => Capability::Hidden,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Lock,
    Suspend,
    Hibernate,
    LogOut,
    Reboot,
    PowerOff,
}

impl Action {
    pub fn inhibits(&self) -> Option<&'static str> {
        match self {
            Self::Suspend | Self::Hibernate => Some("sleep"),
            Self::Reboot | Self::PowerOff | Self::LogOut => Some("shutdown"),
            Self::Lock => None,
        }
    }
}

pub enum Command {
    Run {
        action: Action,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
}

pub enum Event {
    Logind(LogindSnapshot),
    Windows(Option<usize>),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogindSnapshot {
    user: Option<String>,
    signed_in_seconds: Option<u64>,
    inhibitors: Vec<Inhibitor>,
    suspend: Capability,
    hibernate: Capability,
    reboot: Capability,
    power_off: Capability,
}

pub struct SessionActions {
    state: Publisher<SessionActionsState>,
    compositor: CompositorHandle,
}

pub type SessionActionsHandle = ServiceEndpoint<SessionActions>;

impl ServiceEndpoint<SessionActions> {
    pub async fn run(&self, action: Action) -> Result<(), CommandError> {
        self.call(
            "session actions stopped before completing the command",
            |reply| Command::Run { action, reply },
        )
        .await?
    }
}

pub struct Dependencies {
    pub compositor: CompositorHandle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Watch {
    Logind,
    Compositor,
}

impl Service for SessionActions {
    const NAME: &'static str = "session-actions";
    type Config = NoConfig;
    type State = SessionActionsState;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = Watch;

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        vec![
            Sub::stream(Watch::Logind, logind_events),
            Sub::watch(
                Watch::Compositor,
                self.compositor.subscribe(),
                |state| Event::Windows(state.windows.map(|windows| windows.windows.len())),
                Event::Windows(None),
            ),
        ]
    }

    async fn start(
        ctx: &Ctx<Self>,
        _config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> Self {
        Self {
            state: ctx.publisher(),
            compositor: dependencies.compositor,
        }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Logind(logind)) => {
                self.state.update(|state| apply_logind(state, logind));
                ctx.running();
            }
            Input::Event(Event::Windows(windows)) => {
                self.state.update(|state| state.windows = windows);
            }
            Input::Event(Event::Failed(reason)) => ctx.degraded(reason),
            Input::Command(Command::Run { action, reply }) => self.run(ctx, action, reply),
            Input::Config(_) => {}
        }
    }
}

impl SessionActions {
    fn run(
        &self,
        ctx: &Ctx<Self>,
        action: Action,
        reply: oneshot::Sender<Result<(), CommandError>>,
    ) {
        match action {
            Action::Reboot => self.admit(ctx, "reboot", reply),
            Action::PowerOff => self.admit(ctx, "power-off", reply),
            action => self.invoke_locally(ctx, action, reply),
        }
    }

    fn admit(
        &self,
        ctx: &Ctx<Self>,
        action: &'static str,
        reply: oneshot::Sender<Result<(), CommandError>>,
    ) {
        let Ok(bus) = ctx.session_bus().cloned() else {
            let _ = reply.send(Err(CommandError::Unavailable(
                "no session bus for session actions".to_owned(),
            )));
            return;
        };
        ctx.spawn_detached(move |_ctx| async move {
            let _ = reply.send(admit(&bus, action).await);
        });
    }

    fn invoke_locally(
        &self,
        ctx: &Ctx<Self>,
        action: Action,
        reply: oneshot::Sender<Result<(), CommandError>>,
    ) {
        let Ok(bus) = ctx.system_bus().cloned() else {
            let _ = reply.send(Err(CommandError::Unavailable(
                "no system bus for session actions".to_owned(),
            )));
            return;
        };
        let user_bus = ctx.session_bus().ok().cloned();
        ctx.spawn_detached(move |_ctx| async move {
            let result = match (&action, &user_bus) {
                (Action::LogOut, Some(user_bus))
                    if graphical_session_has_compositor(user_bus).await =>
                {
                    stop_graphical_session(user_bus).await
                }
                _ => invoke(&bus, action).await,
            };
            let _ = reply.send(result);
        });
    }
}

fn apply_logind(state: &mut SessionActionsState, logind: LogindSnapshot) {
    state.user = logind.user;
    state.signed_in_seconds = logind.signed_in_seconds;
    state.inhibitors = logind.inhibitors;
    state.suspend = logind.suspend;
    state.hibernate = logind.hibernate;
    state.reboot = logind.reboot;
    state.power_off = logind.power_off;
}

async fn logind_events(ctx: Ctx<SessionActions>) -> Events {
    let Ok(bus) = ctx.system_bus().cloned() else {
        return Box::pin(stream::once(async {
            Event::Failed("no system bus for session actions".to_owned())
        }));
    };
    let manager = match Login1ManagerProxy::new(&bus).await {
        Ok(manager) => manager,
        Err(error) => {
            let error = error.to_string();
            return Box::pin(stream::once(async move { Event::Failed(error) }));
        }
    };

    let mut wakes: Vec<Pin<Box<dyn Stream<Item = ()> + Send>>> = Vec::new();
    if let Ok(new) = manager.receive_session_new().await.map(wake) {
        wakes.push(new);
    }
    if let Ok(removed) = manager.receive_session_removed().await.map(wake) {
        wakes.push(removed);
    }
    wakes.push(wake(manager.receive_block_inhibited_changed().await));
    wakes.push(wake(manager.receive_delay_inhibited_changed().await));

    let first = snapshot_logind(&bus).await;
    Box::pin(
        stream::once(async move { first })
            .chain(stream::select_all(wakes).then({
                let bus = bus.clone();
                move |_| {
                    let bus = bus.clone();
                    async move { snapshot_logind(&bus).await }
                }
            }))
            .map(|result| match result {
                Ok(snapshot) => Event::Logind(snapshot),
                Err(error) => Event::Failed(error),
            }),
    )
}

async fn snapshot_logind(bus: &zbus::Connection) -> Result<LogindSnapshot, String> {
    let manager = Login1ManagerProxy::new(bus).await.map_err(say)?;
    let path = session_path(bus).await?;
    let current = Login1SessionProxy::builder(bus)
        .path(path)
        .map_err(say)?
        .build()
        .await
        .map_err(say)?;
    let (user, timestamp) = tokio::try_join!(current.name(), current.timestamp()).map_err(say)?;
    let inhibitors = manager
        .list_inhibitors()
        .await
        .map_err(say)
        .map(inhibitors)?;
    let (suspend, hibernate, reboot, power_off) = tokio::try_join!(
        manager.can_suspend(),
        manager.can_hibernate(),
        manager.can_reboot(),
        manager.can_power_off()
    )
    .map_err(say)?;
    Ok(LogindSnapshot {
        user: nonempty(user),
        signed_in_seconds: signed_in(timestamp),
        inhibitors,
        suspend: capability(&suspend),
        hibernate: capability(&hibernate),
        reboot: capability(&reboot),
        power_off: capability(&power_off),
    })
}

const GRAPHICAL_SESSION_TARGET: &str = "graphical-session.target";

struct BoundUnit {
    name: String,
    active_state: String,
    before: Vec<String>,
}

fn stops_graphical_session(units: &[BoundUnit]) -> bool {
    units.iter().any(|unit| {
        unit.name.ends_with(".service")
            && unit.active_state == "active"
            && unit
                .before
                .iter()
                .any(|before| before == GRAPHICAL_SESSION_TARGET)
    })
}

async fn unit_proxy(
    bus: &zbus::Connection,
    manager: &Systemd1ManagerProxy<'_>,
    name: &str,
) -> zbus::Result<Systemd1UnitProxy<'static>> {
    let path = manager.get_unit(name).await?;
    Systemd1UnitProxy::builder(bus).path(path)?.build().await
}

async fn bound_unit(
    bus: &zbus::Connection,
    manager: &Systemd1ManagerProxy<'_>,
    name: String,
) -> Option<BoundUnit> {
    let unit = unit_proxy(bus, manager, &name).await.ok()?;
    let (active_state, before) = tokio::try_join!(unit.active_state(), unit.before()).ok()?;
    Some(BoundUnit {
        name,
        active_state,
        before,
    })
}

async fn graphical_session_has_compositor(bus: &zbus::Connection) -> bool {
    let Ok(manager) = Systemd1ManagerProxy::new(bus).await else {
        return false;
    };
    let Ok(target) = unit_proxy(bus, &manager, GRAPHICAL_SESSION_TARGET).await else {
        return false;
    };
    let Ok(bound_by) = target.bound_by().await else {
        return false;
    };
    let units = futures_util::future::join_all(
        bound_by
            .into_iter()
            .filter(|name| name.ends_with(".service"))
            .map(|name| bound_unit(bus, &manager, name)),
    )
    .await
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    stops_graphical_session(&units)
}

async fn stop_graphical_session(bus: &zbus::Connection) -> Result<(), CommandError> {
    let manager = Systemd1ManagerProxy::new(bus)
        .await
        .map_err(|error| CommandError::Internal(error.to_string()))?;
    manager
        .stop_unit(GRAPHICAL_SESSION_TARGET, "replace-irreversibly")
        .await
        .map(|_| ())
        .map_err(|error| CommandError::Internal(error.to_string()))
}

async fn invoke(bus: &zbus::Connection, action: Action) -> Result<(), CommandError> {
    let manager = Login1ManagerProxy::new(bus)
        .await
        .map_err(|error| CommandError::Unavailable(error.to_string()))?;
    match action {
        Action::Lock => {
            manager
                .lock_session(&current_session_id(bus, &manager).await?)
                .await
        }
        Action::Suspend => manager.suspend(true).await,
        Action::Hibernate => manager.hibernate(true).await,
        Action::LogOut => {
            manager
                .terminate_session(&current_session_id(bus, &manager).await?)
                .await
        }
        Action::Reboot | Action::PowerOff => {
            return Err(CommandError::Unavailable(
                "reboot and power-off are admitted by agents".to_owned(),
            ));
        }
    }
    .map_err(|error| CommandError::Internal(error.to_string()))
}

async fn current_session_id(
    bus: &zbus::Connection,
    manager: &Login1ManagerProxy<'_>,
) -> Result<String, CommandError> {
    let path = match manager.get_session_by_pid(std::process::id()).await {
        Ok(path) => path,
        Err(_) => session_path(bus).await.map_err(CommandError::Unavailable)?,
    };
    let session = Login1SessionProxy::builder(bus)
        .path(path)
        .map_err(|error| CommandError::Unavailable(error.to_string()))?
        .build()
        .await
        .map_err(|error| CommandError::Unavailable(error.to_string()))?;
    session
        .id()
        .await
        .map_err(|error| CommandError::Unavailable(error.to_string()))
}

fn inhibitors(entries: Vec<Login1InhibitorEntry>) -> Vec<Inhibitor> {
    entries
        .into_iter()
        .filter(|(_, _, _, mode, _, _)| matches!(mode.as_str(), "block" | "block-weak" | "delay"))
        .take(INHIBITOR_MAX)
        .map(|(what, who, why, mode, _, _)| Inhibitor {
            what: clean(&what, TEXT_MAX),
            who: clean(&who, TEXT_MAX),
            why: clean(&why, TEXT_MAX),
            mode: clean(&mode, TEXT_MAX),
        })
        .collect()
}

async fn admit(bus: &zbus::Connection, action: &str) -> Result<(), CommandError> {
    let admitted = async {
        let proxy = SessionProxy::builder(bus)
            .cache_properties(CacheProperties::No)
            .build()
            .await?;
        proxy.request_action(action).await
    };
    match tokio::time::timeout(DEADLINE, admitted).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(admission_error(error)),
        Err(_) => Err(CommandError::Unavailable(
            "agents did not admit the session action".to_owned(),
        )),
    }
}

fn admission_error(error: zbus::Error) -> CommandError {
    let zbus::Error::MethodError(name, detail, _) = error else {
        return CommandError::Unavailable(error.to_string());
    };
    let message = detail.unwrap_or_else(|| name.to_string());
    match name.as_str() {
        "org.freedesktop.DBus.Error.InvalidArgs" => CommandError::InvalidArgument(message),
        "org.freedesktop.DBus.Error.LimitsExceeded" => CommandError::LimitExceeded(message),
        "org.freedesktop.DBus.Error.AccessDenied"
        | "org.freedesktop.DBus.Error.ServiceUnknown"
        | "org.freedesktop.DBus.Error.NameHasNoOwner"
        | "org.freedesktop.DBus.Error.Failed" => CommandError::Unavailable(message),
        _ => CommandError::Internal(message),
    }
}

fn signed_in(timestamp: u64) -> Option<u64> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_micros() as u64;
    now.checked_sub(timestamp)
        .map(|duration| duration / 1_000_000)
}

fn nonempty(value: String) -> Option<String> {
    let value = clean(&value, TEXT_MAX);
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests;
