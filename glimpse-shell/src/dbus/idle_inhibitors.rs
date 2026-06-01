use std::sync::Arc;

use anyhow::{Context, Result};
use glimpse_core::{
    dbus::login1::Login1ManagerProxy,
    services::{
        framework::{ServiceCommand, ServiceHandle},
        idle_inhibitor::{
            self, BackendHealth, Command, IdleInhibitorHandle, InhibitorsHealth, State,
        },
    },
};
use tokio::sync::{Mutex, mpsc, watch};
use tokio_util::sync::CancellationToken;
use zbus::fdo::{DBusProxy, RequestNameFlags, RequestNameReply};

use crate::dbus::{
    idle_inhibitor_api::InhibitorsApi,
    idle_inhibitor_dbus::RealLogin1Inhibit,
    idle_inhibitor_login1,
    idle_inhibitor_portal::PortalInhibit,
    idle_inhibitor_registry::Registry,
    idle_inhibitor_screen_saver::{
        Login1InhibitTaker, MANUAL_HOLD_WHO, MANUAL_HOLD_WHY, ScreenSaver, inhibit_impl,
    },
};

struct ManualHoldState {
    cookie: Option<u32>,
}

pub async fn spawn(
    session: zbus::Connection,
    system_dbus: zbus::Connection,
    cancel: CancellationToken,
) -> Result<IdleInhibitorHandle> {
    let registry = Arc::new(Mutex::new(Registry::new()));
    let health = Arc::new(Mutex::new(InhibitorsHealth::default()));
    let (state_tx, state_rx) = watch::channel(State::default());
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<ServiceCommand<Command>>(16);

    let login1_proxy = Login1ManagerProxy::new(&system_dbus).await?;
    let login1_inhibit: Arc<dyn Login1InhibitTaker + Send + Sync> = Arc::new(RealLogin1Inhibit {
        proxy: login1_proxy.clone(),
    });

    let session_for_change = session.clone();
    let registry_for_change = registry.clone();
    let health_for_change = health.clone();
    let state_tx_for_change = state_tx.clone();
    let on_change: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        let session = session_for_change.clone();
        let registry = registry_for_change.clone();
        let health = health_for_change.clone();
        let tx = state_tx_for_change.clone();
        tokio::spawn(async move {
            emit_inhibitors_changed(&session).await;
            publish_state(&registry, &health, &tx).await;
        });
    });

    register_objects(
        &session,
        registry.clone(),
        health.clone(),
        login1_inhibit.clone(),
        on_change.clone(),
    )
    .await?;
    acquire_names(&session, &health).await;
    publish_state(&registry, &health, &state_tx).await;
    spawn_login1_observer(
        login1_proxy,
        registry.clone(),
        on_change.clone(),
        cancel.clone(),
    );
    spawn_name_owner_cleanup(
        &session,
        registry.clone(),
        on_change.clone(),
        cancel.clone(),
    );

    let command_registry = registry.clone();
    let command_login1 = login1_inhibit.clone();
    let command_on_change = on_change.clone();
    let command_cancel = cancel.clone();
    let own_bus_name = session
        .unique_name()
        .map(|name| name.to_string())
        .unwrap_or_default();
    tokio::spawn(async move {
        let manual = Mutex::new(ManualHoldState { cookie: None });
        loop {
            tokio::select! {
                _ = command_cancel.cancelled() => return,
                Some(command) = cmd_rx.recv() => {
                    if let ServiceCommand::Command(command) = command {
                        handle_command(
                            command,
                            &command_registry,
                            command_login1.as_ref(),
                            command_on_change.as_ref(),
                            &manual,
                            &own_bus_name,
                        )
                        .await;
                    }
                }
            }
        }
    });

    tracing::info!("idle inhibitor subsystem wired in shell");
    Ok(ServiceHandle::new(state_rx, cmd_tx))
}

async fn register_objects(
    session: &zbus::Connection,
    registry: Arc<Mutex<Registry>>,
    health: Arc<Mutex<InhibitorsHealth>>,
    login1_inhibit: Arc<dyn Login1InhibitTaker + Send + Sync>,
    on_change: Arc<dyn Fn() + Send + Sync>,
) -> Result<()> {
    session
        .object_server()
        .at(
            "/me/aresa/GlimpseIdle/Inhibitors",
            InhibitorsApi {
                registry: registry.clone(),
                health: health.clone(),
                on_change: on_change.clone(),
            },
        )
        .await?;

    session
        .object_server()
        .at(
            "/org/freedesktop/ScreenSaver",
            ScreenSaver {
                registry: registry.clone(),
                login1_inhibit: login1_inhibit.clone(),
                on_change: on_change.clone(),
            },
        )
        .await?;
    session
        .object_server()
        .at(
            "/ScreenSaver",
            ScreenSaver {
                registry: registry.clone(),
                login1_inhibit: login1_inhibit.clone(),
                on_change: on_change.clone(),
            },
        )
        .await?;

    session
        .object_server()
        .at(
            "/org/freedesktop/portal/desktop",
            PortalInhibit {
                registry,
                login1_inhibit,
                on_change,
            },
        )
        .await?;

    Ok(())
}

async fn acquire_names(session: &zbus::Connection, health: &Mutex<InhibitorsHealth>) {
    match try_acquire_name(session, "me.aresa.GlimpseIdle").await {
        Ok(true) => tracing::info!("acquired me.aresa.GlimpseIdle"),
        Ok(false) => tracing::warn!(
            "me.aresa.GlimpseIdle already owned; private inhibitor D-Bus API unavailable"
        ),
        Err(error) => tracing::warn!(?error, "failed to acquire me.aresa.GlimpseIdle"),
    }

    match try_acquire_name(session, "org.freedesktop.ScreenSaver").await {
        Ok(true) => tracing::info!("acquired org.freedesktop.ScreenSaver"),
        Ok(false) => {
            tracing::warn!("ScreenSaver bus name already owned; running in degraded mode");
            health.lock().await.screen_saver = BackendHealth::degraded("Bus name already owned");
        }
        Err(error) => {
            tracing::warn!(?error, "ScreenSaver bus name acquisition failed");
            health.lock().await.screen_saver = BackendHealth::degraded(error.to_string());
        }
    }

    match try_acquire_name(session, "me.aresa.GlimpseIdle.Portal").await {
        Ok(true) => tracing::info!("acquired me.aresa.GlimpseIdle.Portal"),
        Ok(false) => {
            tracing::warn!("Portal bus name already owned; running in degraded mode");
            health.lock().await.portal = BackendHealth::degraded("Bus name already owned");
        }
        Err(error) => {
            tracing::warn!(?error, "Portal bus name acquisition failed");
            health.lock().await.portal = BackendHealth::degraded(error.to_string());
        }
    }
}

async fn try_acquire_name(conn: &zbus::Connection, name: &str) -> Result<bool> {
    let proxy = DBusProxy::new(conn)
        .await
        .context("create session D-Bus proxy")?;
    let well_known = zbus::names::WellKnownName::try_from(name)
        .with_context(|| format!("validate D-Bus name {name}"))?;
    let reply = proxy
        .request_name(well_known, RequestNameFlags::DoNotQueue.into())
        .await
        .with_context(|| format!("request D-Bus name {name}"))?;
    Ok(matches!(
        reply,
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner
    ))
}

fn spawn_login1_observer(
    login1_proxy: Login1ManagerProxy<'static>,
    registry: Arc<Mutex<Registry>>,
    on_change: Arc<dyn Fn() + Send + Sync>,
    cancel: CancellationToken,
) {
    tokio::spawn(async move {
        idle_inhibitor_login1::run(login1_proxy, registry, on_change, cancel).await;
    });
}

fn spawn_name_owner_cleanup(
    session: &zbus::Connection,
    registry: Arc<Mutex<Registry>>,
    on_change: Arc<dyn Fn() + Send + Sync>,
    cancel: CancellationToken,
) {
    let session = session.clone();
    tokio::spawn(async move {
        use futures_util::StreamExt;

        let dbus = match DBusProxy::new(&session).await {
            Ok(proxy) => proxy,
            Err(error) => {
                tracing::warn!(?error, "DBusProxy for NameOwnerChanged failed");
                return;
            }
        };
        let mut stream = match dbus.receive_name_owner_changed().await {
            Ok(stream) => stream,
            Err(error) => {
                tracing::warn!(?error, "NameOwnerChanged subscription failed");
                return;
            }
        };
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                Some(signal) = stream.next() => {
                    let args = match signal.args() {
                        Ok(args) => args,
                        Err(_) => continue,
                    };
                    let dropped = args.new_owner.as_ref().map(|name| name.as_str()).unwrap_or("");
                    if dropped.is_empty() {
                        let name = args.name.as_str().to_owned();
                        let mut reg = registry.lock().await;
                        let released = reg.release_by_bus_name(&name);
                        drop(reg);
                        if !released.is_empty() {
                            on_change();
                        }
                    }
                }
            }
        }
    });
}

async fn handle_command(
    command: Command,
    registry: &Mutex<Registry>,
    login1: &(dyn Login1InhibitTaker + Send + Sync),
    on_change: &(dyn Fn() + Send + Sync),
    manual: &Mutex<ManualHoldState>,
    own_bus_name: &str,
) {
    match command {
        Command::SetManualHold(true) => {
            let mut manual = manual.lock().await;
            if manual.cookie.is_some() {
                return;
            }
            match inhibit_impl(
                registry,
                login1,
                MANUAL_HOLD_WHO.into(),
                MANUAL_HOLD_WHY.into(),
                own_bus_name.to_owned(),
                on_change,
            )
            .await
            {
                Ok(cookie) => manual.cookie = Some(cookie),
                Err(error) => tracing::warn!(?error, "manual hold Inhibit failed"),
            }
        }
        Command::SetManualHold(false) => {
            let cookie = manual.lock().await.cookie.take();
            if let Some(cookie) = cookie {
                let mut reg = registry.lock().await;
                if let Some(id) = reg.lookup_by_cookie(cookie) {
                    reg.release_record(id);
                    drop(reg);
                    on_change();
                }
            }
        }
        Command::Release { id } => {
            let mut reg = registry.lock().await;
            let record = reg.get(id).map(|record| record.record.clone());
            let Some(record) = record else {
                return;
            };
            if !record.can_release {
                tracing::debug!(id, "ignoring release for externally-owned idle inhibitor");
                return;
            }
            let released = reg.release_record(id).is_some();
            drop(reg);
            if released {
                on_change();
            }
        }
    }
}

async fn publish_state(
    registry: &Mutex<Registry>,
    health: &Mutex<InhibitorsHealth>,
    state_tx: &watch::Sender<State>,
) {
    let reg = registry.lock().await;
    let health = health.lock().await;
    if state_tx
        .send(idle_inhibitor::State {
            health: health.clone(),
            inhibitors: reg.snapshot(),
        })
        .is_err()
    {
        tracing::debug!("inhibitor state channel has no receivers");
    }
}

async fn emit_inhibitors_changed(session: &zbus::Connection) {
    let path = match zbus::zvariant::ObjectPath::try_from("/me/aresa/GlimpseIdle/Inhibitors") {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(?error, "emit_inhibitors_changed: invalid path");
            return;
        }
    };
    let iface_ref = match session
        .object_server()
        .interface::<_, InhibitorsApi>(path.clone())
        .await
    {
        Ok(interface) => interface,
        Err(error) => {
            tracing::warn!(?error, "emit_inhibitors_changed: interface lookup failed");
            return;
        }
    };
    let signal_emitter = iface_ref.signal_emitter();
    let api = iface_ref.get().await;
    if let Err(error) = api.inhibitors_changed(signal_emitter).await {
        tracing::warn!(?error, "emit_inhibitors_changed: inhibitors signal failed");
    }
    if let Err(error) = api.health_changed(signal_emitter).await {
        tracing::warn!(?error, "emit_inhibitors_changed: health signal failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use glimpse_core::services::idle_inhibitor::InhibitionTargets;
    use std::os::fd::OwnedFd;

    struct FakeLogin1;

    #[async_trait]
    impl Login1InhibitTaker for FakeLogin1 {
        async fn take(&self, _what: &str, _who: &str, _why: &str) -> Result<OwnedFd, zbus::Error> {
            let file = std::fs::File::open("/dev/null").expect("open /dev/null");
            Ok(OwnedFd::from(file))
        }
    }

    #[tokio::test]
    async fn manual_hold_command_adds_and_removes_record() {
        let registry = Mutex::new(Registry::new());
        let manual = Mutex::new(ManualHoldState { cookie: None });
        let changed = std::sync::atomic::AtomicUsize::new(0);
        let on_change = || {
            changed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        };

        handle_command(
            Command::SetManualHold(true),
            &registry,
            &FakeLogin1,
            &on_change,
            &manual,
            ":1.42",
        )
        .await;
        {
            let records = registry.lock().await.snapshot();
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].who, MANUAL_HOLD_WHO);
            assert_eq!(records[0].targets, InhibitionTargets::manual_hold());
        }

        handle_command(
            Command::SetManualHold(false),
            &registry,
            &FakeLogin1,
            &on_change,
            &manual,
            ":1.42",
        )
        .await;
        assert!(registry.lock().await.snapshot().is_empty());
    }
}
