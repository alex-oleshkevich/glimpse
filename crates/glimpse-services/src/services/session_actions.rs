use std::pin::Pin;

use futures_util::{Stream, StreamExt, stream};
use glimpse_dbus::login1::{
    Login1InhibitorEntry, Login1ManagerProxy, Login1SessionProxy, session_path,
};
use glimpse_utils::clean;
use tokio::sync::oneshot;

use crate::{
    CommandError, CompositorHandle, Ctx, Input, NoConfig, Publisher, Service, ServiceEndpoint,
    ServiceError, Sub, say,
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
        self.what.split(':').any(|held| held == what)
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
    pub fn confirm(&self) -> bool {
        !matches!(self, Self::Lock)
    }

    pub fn inhibits(&self) -> Option<&'static str> {
        match self {
            Self::Suspend | Self::Hibernate => Some("sleep"),
            Self::Reboot | Self::PowerOff => Some("shutdown"),
            Self::Lock | Self::LogOut => None,
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

#[derive(Clone)]
pub struct SessionActionsHandle(ServiceEndpoint<SessionActions>);

impl SessionActionsHandle {
    pub fn snapshot(&self) -> SessionActionsState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<SessionActionsState> {
        self.0.subscribe()
    }

    pub async fn run(&self, action: Action) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::Run { action, reply })?;
        result.await.map_err(|_| {
            CommandError::Unavailable(
                "session actions stopped before completing the command".to_owned(),
            )
        })?
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
    type Handle = SessionActionsHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = Watch;

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
        SessionActionsHandle(endpoint)
    }

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
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            state: ctx.publisher(),
            compositor: dependencies.compositor,
        })
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
        let Ok(bus) = ctx.system_bus().cloned() else {
            let _ = reply.send(Err(CommandError::Unavailable(
                "no system bus for session actions".to_owned(),
            )));
            return;
        };
        ctx.spawn_detached(move |_ctx| async move {
            let result = invoke(&bus, action).await;
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
    if let Ok(new) = manager.receive_session_new().await {
        wakes.push(wake(new));
    }
    if let Ok(removed) = manager.receive_session_removed().await {
        wakes.push(wake(removed));
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

fn wake<S>(stream: S) -> Pin<Box<dyn Stream<Item = ()> + Send>>
where
    S: Stream + Send + 'static,
{
    Box::pin(stream.map(|_| ()))
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
        Action::Reboot => manager.reboot(true).await,
        Action::PowerOff => manager.power_off(true).await,
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
        .filter(|(_, _, _, mode, _, _)| mode == "block" || mode == "delay")
        .take(INHIBITOR_MAX)
        .map(|(what, who, why, mode, _, _)| Inhibitor {
            what: clean(&what, TEXT_MAX),
            who: clean(&who, TEXT_MAX),
            why: clean(&why, TEXT_MAX),
            mode: clean(&mode, TEXT_MAX),
        })
        .collect()
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
mod tests {
    use super::*;

    #[test]
    fn capability_keeps_inhibitor_blocks_visible_and_disabled() {
        let mapped = capability("inhibitor-blocked");
        assert!(mapped.visible());
        assert!(!mapped.enabled());
        assert_eq!(mapped, Capability::Blocked);
    }

    #[test]
    fn a_polkit_challenge_stays_available() {
        assert_eq!(capability("challenge"), Capability::Available);
        assert!(capability("challenge").enabled());
    }

    #[test]
    fn unsupported_capability_is_absent() {
        assert!(!capability("na").visible());
        assert_eq!(capability("no"), Capability::Hidden);
    }

    #[test]
    fn lock_does_not_confirm_power_actions_do() {
        assert!(!Action::Lock.confirm());
        assert!(Action::Reboot.confirm());
        assert!(Action::LogOut.confirm());
    }

    #[test]
    fn inhibitors_are_limited_and_text_is_bounded() {
        let entries = (0..10)
            .map(|_| {
                (
                    "sleep".to_owned(),
                    "x".repeat(200),
                    "reason".to_owned(),
                    "block".to_owned(),
                    0,
                    0,
                )
            })
            .collect();
        let listed = inhibitors(entries);
        assert_eq!(listed.len(), INHIBITOR_MAX);
        assert!(listed[0].who.ends_with('…'));
    }

    #[test]
    fn delay_inhibitors_are_kept_alongside_blocks() {
        let listed = inhibitors(vec![
            (
                "sleep".to_owned(),
                "player".to_owned(),
                "video".to_owned(),
                "delay".to_owned(),
                1000,
                1,
            ),
            (
                "shutdown".to_owned(),
                "updater".to_owned(),
                "install".to_owned(),
                "block".to_owned(),
                1000,
                2,
            ),
            (
                "idle".to_owned(),
                "other".to_owned(),
                "ignore".to_owned(),
                "none".to_owned(),
                1000,
                3,
            ),
        ]);
        assert_eq!(listed.len(), 2);
        assert!(listed[0].blocks("sleep"));
        assert!(listed[1].blocks("shutdown"));
    }

    #[test]
    fn inhibitors_strip_hostile_text() {
        let listed = inhibitors(vec![(
            "sleep".to_owned(),
            "editor\u{202e}gpj.exe".to_owned(),
            "save\nwork".to_owned(),
            "block".to_owned(),
            1000,
            42,
        )]);

        assert_eq!(listed[0].who, "editor gpj.exe");
        assert_eq!(listed[0].why, "save work");
    }

    #[test]
    fn a_window_count_does_not_clobber_logind_fields() {
        let mut state = SessionActionsState {
            user: Some("alex".into()),
            hibernate: Capability::Available,
            ..Default::default()
        };
        apply_logind(
            &mut state,
            LogindSnapshot {
                user: Some("alex".into()),
                signed_in_seconds: Some(12),
                inhibitors: Vec::new(),
                suspend: Capability::Available,
                hibernate: Capability::Hidden,
                reboot: Capability::Available,
                power_off: Capability::Available,
            },
        );
        state.windows = Some(4);
        assert_eq!(state.user.as_deref(), Some("alex"));
        assert_eq!(state.windows, Some(4));
        assert_eq!(state.hibernate, Capability::Hidden);
    }
}
