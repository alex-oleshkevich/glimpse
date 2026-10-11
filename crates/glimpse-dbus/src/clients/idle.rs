use std::collections::{BTreeMap, HashMap};
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

impl IdleInhibitorRecord {
    pub fn owned_by(&self, owner: &str) -> bool {
        (!self.bus_name.is_empty() && self.bus_name == owner)
            || (!self.source.app_id.is_empty() && self.source.app_id == owner)
    }
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
    pub wayland: BackendHealth,
}

impl Default for InhibitorsHealth {
    fn default() -> Self {
        Self::unknown()
    }
}

impl InhibitorsHealth {
    pub(crate) fn unknown() -> Self {
        let health = BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        };
        Self {
            screen_saver: health.clone(),
            portal: health.clone(),
            login1: health.clone(),
            wayland: health,
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
    fn hold_expiries(&self) -> zbus::Result<HashMap<u64, u64>>;

    #[zbus(property)]
    fn health(&self) -> zbus::Result<InhibitorsHealth>;

    fn hold(&self, seconds: u32) -> zbus::Result<u64>;
    fn release(&self, id: u64) -> zbus::Result<()>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct IdleProviderState {
    pub inhibitors: Vec<IdleInhibitorRecord>,
    pub hold_expiries: BTreeMap<u64, u64>,
    pub health: InhibitorsHealth,
    pub available: bool,
    pub reason: Option<String>,
    pub owner: bool,
}

impl IdleProviderState {
    fn unavailable(reason: impl Into<String>, health: Option<InhibitorsHealth>) -> Self {
        Self {
            inhibitors: Vec::new(),
            hold_expiries: BTreeMap::new(),
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
    #[error("idle inhibitor already gone")]
    UnknownObject,
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
        Ok(Err(zbus::Error::FDO(error)))
            if matches!(*error, zbus::fdo::Error::UnknownObject(_)) =>
        {
            Err(IdleProviderError::UnknownObject)
        }
        Ok(Err(zbus::Error::MethodError(name, reason, _))) => {
            let reason = clean(&reason.unwrap_or_default(), REASON);
            match name.as_str() {
                "org.freedesktop.DBus.Error.ServiceUnknown"
                | "org.freedesktop.DBus.Error.NameHasNoOwner" => {
                    Err(IdleProviderError::Unavailable(reason))
                }
                "org.freedesktop.DBus.Error.NoReply" => Err(IdleProviderError::TimedOut),
                "org.freedesktop.DBus.Error.UnknownObject" => Err(IdleProviderError::UnknownObject),
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

async fn cached_or_live_hold_expiries(
    proxy: &Idle1Proxy<'static>,
) -> zbus::Result<HashMap<u64, u64>> {
    let result = match proxy.cached_hold_expiries() {
        Ok(Some(expiries)) => Ok(expiries),
        Ok(None) => proxy.hold_expiries().await,
        Err(error) => Err(error),
    };
    match result {
        Err(zbus::Error::FDO(error)) if matches!(*error, zbus::fdo::Error::UnknownProperty(_)) => {
            Ok(HashMap::new())
        }
        Err(zbus::Error::MethodError(name, _, _))
            if name.as_str() == "org.freedesktop.DBus.Error.UnknownProperty" =>
        {
            Ok(HashMap::new())
        }
        result => result,
    }
}

async fn hold_expiries_or_empty(proxy: &Idle1Proxy<'static>) -> BTreeMap<u64, u64> {
    match cached_or_live_hold_expiries(proxy).await {
        Ok(expiries) => expiries.into_iter().collect(),
        Err(error) => {
            tracing::warn!(%error, "idle hold expiries read failed");
            BTreeMap::new()
        }
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
        hold_expiries: hold_expiries_or_empty(proxy).await,
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

    let name = match zbus::names::BusName::try_from(GLIMPSE_IDLE_BUS_NAME) {
        Ok(name) => name,
        Err(error) => {
            tracing::warn!(%error, "invalid provider bus name");
            return;
        }
    };
    let mut retry_delay = std::time::Duration::from_millis(250);

    loop {
        let proxy = match Idle1Proxy::builder(&connection)
            .cache_properties(CacheProperties::Yes)
            .build()
            .await
        {
            Ok(proxy) => {
                retry_delay = std::time::Duration::from_millis(250);
                proxy
            }
            Err(error) => {
                *current.write().await = None;
                replace_unavailable(&updates, error.to_string());
                tracing::debug!(
                    reason = clean(&error.to_string(), REASON),
                    "idle provider property cache is unavailable"
                );
                if !super::retry_after_failure(&dbus, &mut owners, &name, &mut retry_delay).await {
                    return;
                }
                continue;
            }
        };
        let mut inhibitors_changed = proxy.receive_inhibitors_changed().await;
        let mut health_changed = proxy.receive_health_changed().await;
        let mut hold_expiries_changed = proxy.receive_hold_expiries_changed().await;

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
                    hold_expiries: hold_expiries_or_empty(&proxy).await,
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
                changed = hold_expiries_changed.next() => {
                    let Some(_) = changed else {
                        break "idle hold expiries stream ended".to_owned();
                    };
                    if let (Some(inhibitors), Some(health)) = (last_inhibitors.clone(), last_health.clone()) {
                        publish_state(&updates, &current, &proxy, &mut serving, inhibitors, health).await;
                    }
                }
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
    let reason = reason.into();
    updates.send_if_modified(|previous| {
        let next = IdleProviderState::unavailable(reason, Some(previous.health.clone()));
        if *previous == next {
            return false;
        }
        *previous = next;
        true
    });
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
        wayland: clean_backend_health(health.wayland),
    }
}

fn clean_backend_health(health: BackendHealth) -> BackendHealth {
    BackendHealth {
        message: clean(&health.message, REASON),
        ..health
    }
}

#[cfg(test)]
mod tests;
