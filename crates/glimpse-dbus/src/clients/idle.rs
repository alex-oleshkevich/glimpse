use std::sync::Arc;

use futures_util::StreamExt;
use glimpse_utils::clean;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, watch};
use zbus::proxy::CacheProperties;
use zbus::zvariant::{OwnedValue, Type, Value};

pub const GLIMPSE_IDLE_BUS_NAME: &str = "me.aresa.Glimpse.Idle";
pub const GLIMPSE_IDLE_OBJECT_PATH: &str = "/me/aresa/Glimpse/Idle";

const MOST_INHIBITORS: usize = 64;
const REASON: usize = 240;
const IDENTIFIER: usize = 120;
const HANDLE: usize = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    ScreenSaver,
    Portal,
    Login1,
}

#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue,
)]
#[serde(rename_all = "snake_case")]
pub enum Login1Mode {
    #[default]
    Block,
    Delay,
    BlockWeak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue)]
#[serde(rename_all = "snake_case")]
pub enum HealthKind {
    Ready,
    Degraded,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue)]
pub struct InhibitionTargets {
    pub idle: bool,
    pub suspend: bool,
    pub shutdown: bool,
    pub lid_switch: bool,
    pub power_key: bool,
    pub suspend_key: bool,
    pub hibernate_key: bool,
}

impl InhibitionTargets {
    pub const NONE: Self = Self {
        idle: false,
        suspend: false,
        shutdown: false,
        lid_switch: false,
        power_key: false,
        suspend_key: false,
        hibernate_key: false,
    };

    pub fn idle_only() -> Self {
        Self {
            idle: true,
            ..Self::NONE
        }
    }

    pub fn manual_hold() -> Self {
        Self {
            idle: true,
            suspend: true,
            ..Self::NONE
        }
    }

    pub fn from_login1_what(what: &str) -> Self {
        let mut targets = Self::NONE;
        for token in what.split(':') {
            match token {
                "idle" => targets.idle = true,
                "sleep" => targets.suspend = true,
                "shutdown" => targets.shutdown = true,
                "handle-lid-switch" => targets.lid_switch = true,
                "handle-power-key" => targets.power_key = true,
                "handle-suspend-key" => targets.suspend_key = true,
                "handle-hibernate-key" => targets.hibernate_key = true,
                _ => {}
            }
        }
        targets
    }

    pub fn from_portal_flags(flags: u32) -> Self {
        Self {
            idle: flags & 0x8 != 0,
            suspend: flags & 0x4 != 0,
            shutdown: flags & 0x1 != 0,
            ..Self::NONE
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue)]
pub struct IdleInhibitorSource {
    pub kind: SourceKind,
    pub cookie: u32,
    pub app_id: String,
    pub request_handle: String,
    pub pid: u32,
    pub uid: u32,
    pub mode: Login1Mode,
}

impl IdleInhibitorSource {
    fn base(kind: SourceKind) -> Self {
        Self {
            kind,
            cookie: 0,
            app_id: String::new(),
            request_handle: String::new(),
            pid: 0,
            uid: 0,
            mode: Login1Mode::default(),
        }
    }

    pub fn screen_saver(cookie: u32) -> Self {
        Self {
            cookie,
            ..Self::base(SourceKind::ScreenSaver)
        }
    }

    pub fn portal(request_handle: impl Into<String>, app_id: impl Into<String>) -> Self {
        Self {
            request_handle: request_handle.into(),
            app_id: app_id.into(),
            ..Self::base(SourceKind::Portal)
        }
    }

    pub fn login1(pid: u32, uid: u32, mode: Login1Mode) -> Self {
        Self {
            pid,
            uid,
            mode,
            ..Self::base(SourceKind::Login1)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue)]
pub struct IdleInhibitorRecord {
    pub id: u64,
    pub who: String,
    pub why: String,
    pub bus_name: String,
    pub process_name: String,
    pub source: IdleInhibitorSource,
    pub targets: InhibitionTargets,
    pub can_release: bool,
    pub added_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue)]
pub struct BackendHealth {
    pub kind: HealthKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue)]
pub struct InhibitorsHealth {
    pub screen_saver: BackendHealth,
    pub portal: BackendHealth,
    pub login1: BackendHealth,
}

impl InhibitorsHealth {
    fn unknown() -> Self {
        let health = BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        };
        Self {
            screen_saver: health.clone(),
            portal: health.clone(),
            login1: health,
        }
    }
}

#[zbus::proxy(
    interface = "me.aresa.Glimpse.Idle1",
    default_service = "me.aresa.Glimpse.Idle",
    default_path = "/me/aresa/Glimpse/Idle"
)]
pub trait Idle1 {
    #[zbus(property)]
    fn inhibitors(&self) -> zbus::Result<Vec<IdleInhibitorRecord>>;

    #[zbus(property)]
    fn health(&self) -> zbus::Result<InhibitorsHealth>;

    fn hold(&self, seconds: u32) -> zbus::Result<u64>;
    fn release(&self, id: u64) -> zbus::Result<()>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct IdleProviderState {
    pub inhibitors: Vec<IdleInhibitorRecord>,
    pub health: InhibitorsHealth,
    pub available: bool,
    pub reason: Option<String>,
    pub owner: bool,
}

impl IdleProviderState {
    fn unavailable(reason: impl Into<String>, health: Option<InhibitorsHealth>) -> Self {
        Self {
            inhibitors: Vec::new(),
            health: health.unwrap_or_else(InhibitorsHealth::unknown),
            available: false,
            reason: Some(clean(&reason.into(), REASON)),
            owner: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IdleProviderError {
    #[error("idle provider unavailable: {0}")]
    Unavailable(String),
    #[error("idle provider call timed out")]
    TimedOut,
    #[error("idle provider call failed: {0}")]
    Call(String),
}

#[derive(Clone)]
pub struct IdleProviderHandle {
    state: watch::Receiver<IdleProviderState>,
    proxy: Arc<RwLock<Option<Idle1Proxy<'static>>>>,
}

pub struct IdleProvider {
    handle: IdleProviderHandle,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl IdleProvider {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        let (_, state) = watch::channel(IdleProviderState::unavailable(reason, None));
        Self {
            handle: IdleProviderHandle {
                state,
                proxy: Default::default(),
            },
            task: None,
        }
    }

    pub fn start(connection: zbus::Connection) -> Self {
        let (updates, state) = watch::channel(IdleProviderState::unavailable(
            "provider has no bus owner",
            None,
        ));
        let proxy = Arc::new(RwLock::new(None));
        let task = tokio::spawn(follow_provider(connection, updates, proxy.clone()));
        Self {
            handle: IdleProviderHandle { state, proxy },
            task: Some(task),
        }
    }

    pub fn handle(&self) -> IdleProviderHandle {
        self.handle.clone()
    }

    pub async fn shutdown(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
            tracing::debug!("idle provider follower stopped");
        }
    }
}

impl Drop for IdleProvider {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

impl IdleProviderHandle {
    pub fn snapshot(&self) -> IdleProviderState {
        self.state.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<IdleProviderState> {
        self.state.clone()
    }

    async fn proxy(&self) -> Result<Idle1Proxy<'static>, IdleProviderError> {
        self.proxy.read().await.clone().ok_or_else(|| {
            IdleProviderError::Unavailable(
                self.state
                    .borrow()
                    .reason
                    .clone()
                    .unwrap_or_else(|| "provider has no bus owner".to_owned()),
            )
        })
    }

    pub async fn hold(&self, seconds: u32) -> Result<u64, IdleProviderError> {
        tracing::debug!(seconds, "requesting idle hold");
        call(self.proxy().await?.hold(seconds)).await
    }

    pub async fn release(&self, id: u64) -> Result<(), IdleProviderError> {
        tracing::debug!(id, "releasing idle hold");
        call(self.proxy().await?.release(id)).await
    }
}

async fn call<T>(request: impl Future<Output = zbus::Result<T>>) -> Result<T, IdleProviderError> {
    match tokio::time::timeout(super::DEADLINE, request).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(zbus::Error::MethodError(name, reason, _))) => {
            let reason = clean(&reason.unwrap_or_default(), REASON);
            match name.as_str() {
                "org.freedesktop.DBus.Error.ServiceUnknown"
                | "org.freedesktop.DBus.Error.NameHasNoOwner" => {
                    Err(IdleProviderError::Unavailable(reason))
                }
                "org.freedesktop.DBus.Error.NoReply" => Err(IdleProviderError::TimedOut),
                _ => Err(IdleProviderError::Call(reason)),
            }
        }
        Ok(Err(error)) => Err(IdleProviderError::Call(clean(&error.to_string(), REASON))),
        Err(_) => Err(IdleProviderError::TimedOut),
    }
}

async fn cached_or_live_inhibitors(
    proxy: &Idle1Proxy<'static>,
) -> zbus::Result<Vec<IdleInhibitorRecord>> {
    match proxy.cached_inhibitors() {
        Ok(Some(inhibitors)) => Ok(inhibitors),
        Ok(None) => proxy.inhibitors().await,
        Err(error) => Err(error),
    }
}

async fn cached_or_live_health(proxy: &Idle1Proxy<'static>) -> zbus::Result<InhibitorsHealth> {
    match proxy.cached_health() {
        Ok(Some(health)) => Ok(health),
        Ok(None) => proxy.health().await,
        Err(error) => Err(error),
    }
}

async fn publish_state(
    updates: &watch::Sender<IdleProviderState>,
    current: &RwLock<Option<Idle1Proxy<'static>>>,
    proxy: &Idle1Proxy<'static>,
    serving: &mut bool,
    inhibitors: Vec<IdleInhibitorRecord>,
    health: InhibitorsHealth,
) {
    if !*serving {
        *current.write().await = Some(proxy.clone());
        *serving = true;
        tracing::info!("idle provider recovered without changing owner");
    }
    let state = IdleProviderState {
        inhibitors: decode_inhibitors(inhibitors),
        health: decode_health(health),
        available: true,
        reason: None,
        owner: true,
    };
    tracing::debug!(
        inhibitors = state.inhibitors.len(),
        "idle provider state changed"
    );
    updates.send_replace(state);
}

async fn publish_failure(
    updates: &watch::Sender<IdleProviderState>,
    current: &RwLock<Option<Idle1Proxy<'static>>>,
    serving: &mut bool,
    error: zbus::Error,
    context: &str,
) {
    let reason = clean(&error.to_string(), REASON);
    tracing::warn!(reason, "{context}");
    if *serving {
        *current.write().await = None;
        *serving = false;
    }
    replace_unavailable(updates, &reason);
}

async fn follow_provider(
    connection: zbus::Connection,
    updates: watch::Sender<IdleProviderState>,
    current: Arc<RwLock<Option<Idle1Proxy<'static>>>>,
) {
    let dbus = match zbus::fdo::DBusProxy::new(&connection).await {
        Ok(dbus) => dbus,
        Err(error) => {
            replace_unavailable(&updates, error.to_string());
            tracing::warn!(%error, "cannot observe the idle provider owner");
            return;
        }
    };
    let mut owners = match dbus.receive_name_owner_changed().await {
        Ok(owners) => owners,
        Err(error) => {
            replace_unavailable(&updates, error.to_string());
            tracing::warn!(%error, "cannot observe idle provider owner changes");
            return;
        }
    };

    let mut last_inhibitors: Option<Vec<IdleInhibitorRecord>> = None;
    let mut last_health: Option<InhibitorsHealth> = None;

    loop {
        let proxy = match Idle1Proxy::builder(&connection)
            .cache_properties(CacheProperties::Yes)
            .build()
            .await
        {
            Ok(proxy) => proxy,
            Err(error) => {
                *current.write().await = None;
                replace_unavailable(&updates, error.to_string());
                tracing::debug!(
                    reason = clean(&error.to_string(), REASON),
                    "idle provider has no owner; waiting"
                );
                if !wait_for_owner(&mut owners).await {
                    return;
                }
                continue;
            }
        };
        let mut inhibitors_changed = proxy.receive_inhibitors_changed().await;
        let mut health_changed = proxy.receive_health_changed().await;

        let mut serving = match (
            cached_or_live_inhibitors(&proxy).await,
            cached_or_live_health(&proxy).await,
        ) {
            (Ok(inhibitors), Ok(health)) => {
                *current.write().await = Some(proxy.clone());
                last_inhibitors = Some(inhibitors.clone());
                last_health = Some(health.clone());
                let state = IdleProviderState {
                    inhibitors: decode_inhibitors(inhibitors),
                    health: decode_health(health),
                    available: true,
                    reason: None,
                    owner: true,
                };
                tracing::info!(
                    inhibitors = state.inhibitors.len(),
                    "idle provider connected"
                );
                updates.send_replace(state);
                true
            }
            (Err(error), _) | (_, Err(error)) => {
                *current.write().await = None;
                let reason = clean(&error.to_string(), REASON);
                replace_unavailable(&updates, &reason);
                tracing::warn!(reason, "idle provider snapshot is unavailable");
                false
            }
        };

        let reason = loop {
            tokio::select! {
                changed = inhibitors_changed.next() => {
                    let Some(changed) = changed else {
                        break "idle inhibitors stream ended".to_owned();
                    };
                    match changed.get().await {
                        Ok(inhibitors) => {
                            last_inhibitors = Some(inhibitors.clone());
                            match cached_or_live_health(&proxy).await {
                                Ok(health) => {
                                    last_health = Some(health.clone());
                                    publish_state(&updates, &current, &proxy, &mut serving, inhibitors, health).await;
                                }
                                Err(error) => match last_health.clone() {
                                    Some(health) => {
                                        tracing::debug!(
                                            reason = clean(&error.to_string(), REASON),
                                            "idle provider health read failed; reusing last observed health"
                                        );
                                        publish_state(&updates, &current, &proxy, &mut serving, inhibitors, health).await;
                                    }
                                    None => {
                                        publish_failure(&updates, &current, &mut serving, error, "idle provider health read failed").await;
                                    }
                                },
                            }
                        }
                        Err(error) => {
                            publish_failure(&updates, &current, &mut serving, error, "idle inhibitors change failed").await;
                        }
                    }
                }
                changed = health_changed.next() => {
                    let Some(changed) = changed else {
                        break "idle health stream ended".to_owned();
                    };
                    match changed.get().await {
                        Ok(health) => {
                            last_health = Some(health.clone());
                            match cached_or_live_inhibitors(&proxy).await {
                                Ok(inhibitors) => {
                                    last_inhibitors = Some(inhibitors.clone());
                                    publish_state(&updates, &current, &proxy, &mut serving, inhibitors, health).await;
                                }
                                Err(error) => match last_inhibitors.clone() {
                                    Some(inhibitors) => {
                                        tracing::debug!(
                                            reason = clean(&error.to_string(), REASON),
                                            "idle provider inhibitors read failed; reusing last observed inhibitors"
                                        );
                                        publish_state(&updates, &current, &proxy, &mut serving, inhibitors, health).await;
                                    }
                                    None => {
                                        publish_failure(&updates, &current, &mut serving, error, "idle provider inhibitors read failed").await;
                                    }
                                },
                            }
                        }
                        Err(error) => {
                            publish_failure(&updates, &current, &mut serving, error, "idle health change failed").await;
                        }
                    }
                }
                owner = owners.next() => {
                    let Some(owner) = owner else {
                        break "idle provider owner stream ended".to_owned();
                    };
                    let Ok(args) = owner.args() else {
                        continue;
                    };
                    if args.name().as_str() != GLIMPSE_IDLE_BUS_NAME {
                        continue;
                    }
                    if args.old_owner().is_some() {
                        break match args.new_owner().is_some() {
                            true => "provider owner changed",
                            false => "provider has no bus owner",
                        }.to_owned();
                    }
                    if !serving && args.new_owner().is_some() {
                        break "provider took the name".to_owned();
                    }
                }
            }
        };
        if serving {
            *current.write().await = None;
            replace_unavailable(&updates, &reason);
            tracing::warn!(reason, "idle provider disconnected");
        } else {
            tracing::debug!(reason, "idle provider rebuilding its proxy");
        }
    }
}

fn replace_unavailable(updates: &watch::Sender<IdleProviderState>, reason: impl Into<String>) {
    let health = updates.borrow().health.clone();
    updates.send_replace(IdleProviderState::unavailable(reason, Some(health)));
}

async fn wait_for_owner(owners: &mut zbus::fdo::NameOwnerChangedStream) -> bool {
    while let Some(owner) = owners.next().await {
        let Ok(args) = owner.args() else {
            continue;
        };
        if args.name().as_str() == GLIMPSE_IDLE_BUS_NAME && args.new_owner().is_some() {
            return true;
        }
    }
    false
}

fn decode_inhibitors(records: Vec<IdleInhibitorRecord>) -> Vec<IdleInhibitorRecord> {
    records
        .into_iter()
        .take(MOST_INHIBITORS)
        .map(clean_record)
        .collect()
}

fn clean_record(record: IdleInhibitorRecord) -> IdleInhibitorRecord {
    IdleInhibitorRecord {
        who: clean(&record.who, IDENTIFIER),
        why: clean(&record.why, REASON),
        bus_name: clean(&record.bus_name, HANDLE),
        process_name: clean(&record.process_name, IDENTIFIER),
        source: clean_source(record.source),
        ..record
    }
}

fn clean_source(source: IdleInhibitorSource) -> IdleInhibitorSource {
    IdleInhibitorSource {
        app_id: clean(&source.app_id, IDENTIFIER),
        request_handle: clean(&source.request_handle, HANDLE),
        ..source
    }
}

fn decode_health(health: InhibitorsHealth) -> InhibitorsHealth {
    InhibitorsHealth {
        screen_saver: clean_backend_health(health.screen_saver),
        portal: clean_backend_health(health.portal),
        login1: clean_backend_health(health.login1),
    }
}

fn clean_backend_health(health: BackendHealth) -> BackendHealth {
    BackendHealth {
        message: clean(&health.message, REASON),
        ..health
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_signatures_match_the_versioned_contract() {
        assert_eq!(SourceKind::SIGNATURE, "u");
        assert_eq!(InhibitionTargets::SIGNATURE, "(bbbbbbb)");
        assert_eq!(IdleInhibitorSource::SIGNATURE, "(uussuuu)");
        assert_eq!(
            IdleInhibitorRecord::SIGNATURE,
            "(tssss(uussuuu)(bbbbbbb)bt)"
        );
        assert_eq!(InhibitorsHealth::SIGNATURE, "((us)(us)(us))");
    }

    #[test]
    fn from_login1_what_parses_known_tokens_and_ignores_unknown_ones() {
        let targets = InhibitionTargets::from_login1_what("idle:sleep:handle-lid-switch:bogus");
        assert_eq!(
            targets,
            InhibitionTargets {
                idle: true,
                suspend: true,
                lid_switch: true,
                ..InhibitionTargets::NONE
            }
        );
    }

    #[test]
    fn from_login1_what_maps_each_token_to_exactly_its_own_field() {
        let cases = [
            (
                "idle",
                InhibitionTargets {
                    idle: true,
                    ..InhibitionTargets::NONE
                },
            ),
            (
                "sleep",
                InhibitionTargets {
                    suspend: true,
                    ..InhibitionTargets::NONE
                },
            ),
            (
                "shutdown",
                InhibitionTargets {
                    shutdown: true,
                    ..InhibitionTargets::NONE
                },
            ),
            (
                "handle-lid-switch",
                InhibitionTargets {
                    lid_switch: true,
                    ..InhibitionTargets::NONE
                },
            ),
            (
                "handle-power-key",
                InhibitionTargets {
                    power_key: true,
                    ..InhibitionTargets::NONE
                },
            ),
            (
                "handle-suspend-key",
                InhibitionTargets {
                    suspend_key: true,
                    ..InhibitionTargets::NONE
                },
            ),
            (
                "handle-hibernate-key",
                InhibitionTargets {
                    hibernate_key: true,
                    ..InhibitionTargets::NONE
                },
            ),
        ];
        for (token, expected) in cases {
            assert_eq!(
                InhibitionTargets::from_login1_what(token),
                expected,
                "token {token}"
            );
        }
    }

    #[test]
    fn from_portal_flags_decodes_each_bit_independently() {
        assert_eq!(
            InhibitionTargets::from_portal_flags(0x8),
            InhibitionTargets {
                idle: true,
                ..InhibitionTargets::NONE
            }
        );
        assert_eq!(
            InhibitionTargets::from_portal_flags(0x4),
            InhibitionTargets {
                suspend: true,
                ..InhibitionTargets::NONE
            }
        );
        assert_eq!(
            InhibitionTargets::from_portal_flags(0x1),
            InhibitionTargets {
                shutdown: true,
                ..InhibitionTargets::NONE
            }
        );
    }

    #[test]
    fn from_portal_flags_ignores_user_switch_in_any_combination() {
        assert_eq!(
            InhibitionTargets::from_portal_flags(12),
            InhibitionTargets {
                idle: true,
                suspend: true,
                ..InhibitionTargets::NONE
            },
            "flags=12 (Idle|Suspend)"
        );
        assert_eq!(
            InhibitionTargets::from_portal_flags(5),
            InhibitionTargets {
                shutdown: true,
                suspend: true,
                ..InhibitionTargets::NONE
            },
            "flags=5 (Logout|Suspend)"
        );
        assert_eq!(
            InhibitionTargets::from_portal_flags(2),
            InhibitionTargets::NONE,
            "flags=2 (UserSwitch alone) sets no target bit"
        );
        assert_eq!(
            InhibitionTargets::from_portal_flags(15),
            InhibitionTargets {
                idle: true,
                suspend: true,
                shutdown: true,
                ..InhibitionTargets::NONE
            },
            "flags=15 (all four) still drops UserSwitch's contribution"
        );
    }

    #[test]
    fn manual_hold_sets_only_idle_and_suspend() {
        assert_eq!(
            InhibitionTargets::manual_hold(),
            InhibitionTargets {
                idle: true,
                suspend: true,
                ..InhibitionTargets::NONE
            }
        );
    }

    #[test]
    fn source_constructors_populate_only_their_own_kinds_fields() {
        let screen_saver = IdleInhibitorSource::screen_saver(7);
        assert_eq!(screen_saver.kind, SourceKind::ScreenSaver);
        assert_eq!(screen_saver.cookie, 7);
        assert_eq!(screen_saver.app_id, "");
        assert_eq!(screen_saver.request_handle, "");
        assert_eq!(screen_saver.pid, 0);
        assert_eq!(screen_saver.uid, 0);

        let portal = IdleInhibitorSource::portal("/handle/1", "org.example.App");
        assert_eq!(portal.kind, SourceKind::Portal);
        assert_eq!(portal.request_handle, "/handle/1");
        assert_eq!(portal.app_id, "org.example.App");
        assert_eq!(portal.cookie, 0);
        assert_eq!(portal.pid, 0);
        assert_eq!(portal.uid, 0);

        let login1 = IdleInhibitorSource::login1(123, 1000, Login1Mode::Delay);
        assert_eq!(login1.kind, SourceKind::Login1);
        assert_eq!(login1.pid, 123);
        assert_eq!(login1.uid, 1000);
        assert_eq!(login1.mode, Login1Mode::Delay);
        assert_eq!(login1.cookie, 0);
        assert_eq!(login1.app_id, "");
        assert_eq!(login1.request_handle, "");
    }

    fn every_field_populated_record() -> IdleInhibitorRecord {
        IdleInhibitorRecord {
            id: 42,
            who: "Zoom".to_owned(),
            why: "screen sharing".to_owned(),
            bus_name: ":1.123".to_owned(),
            process_name: "zoom".to_owned(),
            source: IdleInhibitorSource {
                kind: SourceKind::Login1,
                cookie: 11,
                app_id: "org.example.App".to_owned(),
                request_handle: "/org/freedesktop/portal/desktop/request/1".to_owned(),
                pid: 555,
                uid: 1000,
                mode: Login1Mode::BlockWeak,
            },
            targets: InhibitionTargets {
                idle: true,
                suspend: false,
                shutdown: true,
                lid_switch: false,
                power_key: true,
                suspend_key: false,
                hibernate_key: true,
            },
            can_release: true,
            added_at_unix: 1_789_382_400,
        }
    }

    #[test]
    fn a_record_with_every_field_populated_round_trips_through_json() {
        let record = every_field_populated_record();

        let encoded = serde_json::to_string(&record).unwrap();
        let decoded: IdleInhibitorRecord = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, record);
    }

    #[test]
    fn a_record_round_trips_positionally_over_the_wire() {
        use zbus::zvariant::Endian;
        use zbus::zvariant::serialized::Context;

        type SourceWire = (u32, u32, String, String, u32, u32, u32);
        type TargetsWire = (bool, bool, bool, bool, bool, bool, bool);
        type RecordWire = (
            u64,
            String,
            String,
            String,
            String,
            SourceWire,
            TargetsWire,
            bool,
            u64,
        );

        let record = every_field_populated_record();

        let ctxt = Context::new_dbus(Endian::Little, 0);
        let encoded = zbus::zvariant::to_bytes(ctxt, &record).unwrap();
        let (wire, _): (RecordWire, _) = encoded.deserialize().unwrap();

        assert_eq!(
            wire,
            (
                record.id,
                record.who.clone(),
                record.why.clone(),
                record.bus_name.clone(),
                record.process_name.clone(),
                (
                    record.source.kind as u32,
                    record.source.cookie,
                    record.source.app_id.clone(),
                    record.source.request_handle.clone(),
                    record.source.pid,
                    record.source.uid,
                    record.source.mode as u32,
                ),
                (
                    record.targets.idle,
                    record.targets.suspend,
                    record.targets.shutdown,
                    record.targets.lid_switch,
                    record.targets.power_key,
                    record.targets.suspend_key,
                    record.targets.hibernate_key,
                ),
                record.can_release,
                record.added_at_unix,
            ),
            "a positional decode must land each value in the slot its field name implies, \
             which a JSON round trip (matched by key, not position) cannot verify"
        );
    }
}
