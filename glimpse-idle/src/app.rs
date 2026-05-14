use std::sync::Arc;

use glimpse_core::{
    Config, ConfigEvent,
    dbus::login1::Login1ManagerProxy,
    services::{
        battery::{BatteryHandle, BatteryService},
        framework::Control,
        idle::{self, IdleHandle, IdleService, State},
        idle_inhibitor::InhibitorsHealth,
    },
    watch_for_config_changes,
};
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

use crate::{
    backend,
    dbus_helpers::RealLogin1Inhibit,
    inhibitor_registry::Registry,
    inhibitors_api::InhibitorsApi,
    login1_observer,
    portal_inhibit::PortalInhibit,
    runtime::{InstanceGuard, try_acquire_name},
    screen_saver::{Login1InhibitTaker, ScreenSaver},
};

struct AppTask {
    task: tokio::task::JoinHandle<()>,
}

impl AppTask {
    async fn join(self) {
        let _ = self.task.await;
    }
}

pub async fn run(config: Config, instance: InstanceGuard) -> anyhow::Result<()> {
    let cancel = CancellationToken::new();
    let mut running_services = Vec::new();
    let session = instance.connection.clone();
    let system_dbus = zbus::Connection::system().await?;

    let (battery_service, battery) = BatteryService::new(system_dbus.clone());
    running_services.push(spawn_service(cancel.clone(), |cancel| {
        battery_service.run(cancel)
    }));

    let (idle_service, idle) = IdleService::new(battery.clone());
    running_services.push(spawn_service(cancel.clone(), |cancel| {
        idle_service.run(cancel)
    }));

    start_services(&battery, &idle, config.clone());
    running_services.push(spawn_idle_subscription(idle.clone(), cancel.clone()));
    let backend_idle = idle.clone();
    running_services.push(spawn_service(cancel.clone(), move |cancel| {
        backend::run(backend_idle.clone(), cancel)
    }));

    if let Err(e) = wire_inhibitor_subsystem(
        &session,
        &system_dbus,
        cancel.clone(),
        &mut running_services,
    )
    .await
    {
        tracing::error!(error = ?e, "failed to wire inhibitor subsystem; daemon continues without it");
    }

    let (config_tx, mut config_rx) = mpsc::channel(1);
    let config_cancel = cancel.clone();
    running_services.push(spawn_service(cancel.clone(), move |_| async move {
        tokio::select! {
            _ = config_cancel.cancelled() => {}
            _ = watch_for_config_changes(config_tx) => {}
        }
    }));

    tracing::info!("glimpse-idle is running");
    let mut current_idle_config = config.idle;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("received shutdown signal");
                break;
            }
            message = config_rx.recv() => match message {
                Some(ConfigEvent::Changed(config)) => {
                    if current_idle_config == config.idle {
                        continue;
                    }
                    tracing::info!("idle config changed");
                    reconfigure_services(&idle, config.clone());
                    current_idle_config = config.idle;
                }
                None => break,
            }
        }
    }

    shutdown_services(&battery, &idle);
    cancel.cancel();
    for service in running_services {
        service.join().await;
    }
    tracing::info!("glimpse-idle stopped");

    Ok(())
}

async fn wire_inhibitor_subsystem(
    session: &zbus::Connection,
    system_dbus: &zbus::Connection,
    cancel: CancellationToken,
    running_services: &mut Vec<AppTask>,
) -> anyhow::Result<()> {
    let registry = Arc::new(Mutex::new(Registry::new()));
    let health = Arc::new(Mutex::new(InhibitorsHealth::default()));

    let login1_proxy = Login1ManagerProxy::new(system_dbus).await?;
    let login1_inhibit: Arc<dyn Login1InhibitTaker + Send + Sync> = Arc::new(RealLogin1Inhibit {
        proxy: login1_proxy.clone(),
    });

    // PropertiesChanged hook — fires the Inhibitors and Health change signals
    // on the InhibitorsApi path so the shell's proxies refresh. Synchronous from
    // the caller's perspective; emission happens on a background task.
    let session_for_change = session.clone();
    let on_change: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        let session = session_for_change.clone();
        tokio::spawn(async move {
            emit_inhibitors_changed(&session).await;
        });
    });

    // Inhibitors API (Glimpse-private read/release).
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

    // ScreenSaver server at both historical paths.
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

    // Portal backend at the xdg-desktop-portal object path.
    session
        .object_server()
        .at(
            "/org/freedesktop/portal/desktop",
            PortalInhibit {
                registry: registry.clone(),
                login1_inhibit: login1_inhibit.clone(),
                on_change: on_change.clone(),
            },
        )
        .await?;

    // Acquire the two public well-known names. Non-fatal on conflict —
    // backend health degrades and the daemon keeps running.
    use glimpse_core::services::idle_inhibitor::BackendHealth;
    match try_acquire_name(session, "org.freedesktop.ScreenSaver").await {
        Ok(true) => tracing::info!("acquired org.freedesktop.ScreenSaver"),
        Ok(false) => {
            tracing::warn!("ScreenSaver bus name already owned — running in degraded mode");
            health.lock().await.screen_saver = BackendHealth::degraded("Bus name already owned");
        }
        Err(e) => {
            tracing::warn!(error = ?e, "ScreenSaver bus name acquisition failed");
            health.lock().await.screen_saver = BackendHealth::degraded(e.to_string());
        }
    }
    match try_acquire_name(session, "me.aresa.GlimpseIdle.Portal").await {
        Ok(true) => tracing::info!("acquired me.aresa.GlimpseIdle.Portal"),
        Ok(false) => {
            tracing::warn!("Portal bus name already owned — running in degraded mode");
            health.lock().await.portal = BackendHealth::degraded("Bus name already owned");
        }
        Err(e) => {
            tracing::warn!(error = ?e, "Portal bus name acquisition failed");
            health.lock().await.portal = BackendHealth::degraded(e.to_string());
        }
    }

    // login1 observer (5s poll).
    let observer_registry = registry.clone();
    let observer_on_change = on_change.clone();
    let observer_cancel = cancel.clone();
    running_services.push(spawn_service(cancel.clone(), move |_| async move {
        login1_observer::run(
            login1_proxy,
            observer_registry,
            observer_on_change,
            observer_cancel,
        )
        .await;
    }));

    // NameOwnerChanged — auto-release records owned by callers who disconnect.
    let noc_registry = registry.clone();
    let noc_on_change = on_change.clone();
    let noc_session = session.clone();
    let noc_cancel = cancel.clone();
    running_services.push(spawn_service(cancel.clone(), move |_| async move {
        use futures_util::StreamExt;
        let dbus = match zbus::fdo::DBusProxy::new(&noc_session).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = ?e, "DBusProxy for NameOwnerChanged failed");
                return;
            }
        };
        let mut stream = match dbus.receive_name_owner_changed().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = ?e, "NameOwnerChanged subscription failed");
                return;
            }
        };
        loop {
            tokio::select! {
                _ = noc_cancel.cancelled() => return,
                Some(signal) = stream.next() => {
                    let args = match signal.args() { Ok(a) => a, Err(_) => continue };
                    let dropped = args.new_owner.as_ref().map(|n| n.as_str()).unwrap_or("");
                    if dropped.is_empty() {
                        let name = args.name.as_str().to_owned();
                        let mut reg = noc_registry.lock().await;
                        let released = reg.release_by_bus_name(&name);
                        drop(reg);
                        if !released.is_empty() { noc_on_change(); }
                    }
                }
            }
        }
    }));

    tracing::info!("inhibitor subsystem wired");
    Ok(())
}

async fn emit_inhibitors_changed(session: &zbus::Connection) {
    let path = match zbus::zvariant::ObjectPath::try_from("/me/aresa/GlimpseIdle/Inhibitors") {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = ?e, "emit_inhibitors_changed: invalid path");
            return;
        }
    };
    let iface_ref = match session
        .object_server()
        .interface::<_, InhibitorsApi>(path.clone())
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = ?e, "emit_inhibitors_changed: interface lookup failed");
            return;
        }
    };
    let ctxt = iface_ref.signal_emitter();
    let api = iface_ref.get().await;
    if let Err(e) = api.inhibitors_changed(ctxt).await {
        tracing::warn!(error = ?e, "emit_inhibitors_changed: inhibitors signal failed");
    }
    if let Err(e) = api.health_changed(ctxt).await {
        tracing::warn!(error = ?e, "emit_inhibitors_changed: health signal failed");
    }
}

pub fn start_services(battery: &BatteryHandle, idle: &IdleHandle, config: Config) {
    battery.try_send_control(
        "battery",
        Control::Start(config.clone()),
        "failed to send service control",
    );
    idle.try_send_control(
        "idle",
        Control::Start(config),
        "failed to send service control",
    );
}

pub fn reconfigure_services(idle: &IdleHandle, config: Config) {
    idle.try_send_command(
        "idle",
        idle::Command::ApplyConfig(config.idle),
        "failed to send idle config update",
    );
}

fn shutdown_services(battery: &BatteryHandle, idle: &IdleHandle) {
    battery.try_send_control(
        "battery",
        Control::Shutdown,
        "failed to send service control",
    );
    idle.try_send_control("idle", Control::Shutdown, "failed to send service control");
}

fn spawn_idle_subscription(idle: IdleHandle, cancel: CancellationToken) -> AppTask {
    spawn_service(cancel.clone(), move |_| async move {
        let mut state_rx = idle.subscribe();
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                changed = state_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    log_idle_state(&state_rx.borrow().clone());
                }
            }
        }
    })
}

fn log_idle_state(state: &State) {
    let timeouts = state
        .listeners
        .iter()
        .map(|listener| listener.timeout.to_string())
        .collect::<Vec<_>>()
        .join(",");
    tracing::info!(
        enabled = state.enabled,
        health = ?state.health,
        power_source = ?state.power_source,
        generation = state.generation,
        listeners = state.listeners.len(),
        timeouts,
        "idle state changed"
    );
}

fn spawn_service<F, Fut>(cancel: CancellationToken, run: F) -> AppTask
where
    F: FnOnce(CancellationToken) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let task = tokio::spawn(async move { run(cancel).await });
    AppTask { task }
}

#[cfg(test)]
mod tests {
    use super::{reconfigure_services, start_services};
    use glimpse_core::{
        Config, IdleConfig,
        services::{
            battery,
            framework::{Control, ServiceCommand, ServiceHandle},
            idle,
        },
    };
    use tokio::sync::{mpsc, watch};

    fn handle<State: Clone, Command: Send>(
        state: State,
    ) -> (
        ServiceHandle<State, Command>,
        mpsc::Receiver<ServiceCommand<Command>>,
    ) {
        let (_state_tx, state_rx) = watch::channel(state);
        let (command_tx, command_rx) = mpsc::channel(4);
        (ServiceHandle::new(state_rx, command_tx), command_rx)
    }

    #[tokio::test]
    async fn start_services_sends_start_control_to_battery_and_idle() {
        let (battery, mut battery_rx) =
            handle::<battery::State, battery::Command>(battery::State::default());
        let (idle, mut idle_rx) = handle::<idle::State, idle::Command>(idle::State::default());

        start_services(&battery, &idle, Config::default());

        assert!(matches!(
            battery_rx.recv().await,
            Some(ServiceCommand::Control(Control::Start(_)))
        ));
        assert!(matches!(
            idle_rx.recv().await,
            Some(ServiceCommand::Control(Control::Start(_)))
        ));
    }

    #[tokio::test]
    async fn reconfigure_services_sends_idle_config_command() {
        let (idle, mut idle_rx) = handle::<idle::State, idle::Command>(idle::State::default());
        let config = Config {
            idle: IdleConfig {
                enabled: false,
                ..IdleConfig::default()
            },
            ..Config::default()
        };

        reconfigure_services(&idle, config);

        assert!(matches!(
            idle_rx.recv().await,
            Some(ServiceCommand::Command(idle::Command::ApplyConfig(config))) if !config.enabled
        ));
    }
}
