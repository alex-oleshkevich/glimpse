use std::sync::Arc;

use anyhow::{Context as _, Result};
use futures_util::StreamExt;
use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_dbus::Exported;
use glimpse_dbus::actions::{self, ActionsHandle};
use glimpse_dbus::idle::{GLIMPSE_IDLE_BUS_NAME, GLIMPSE_IDLE_OBJECT_PATH};
use glimpse_dbus::login1::Login1ManagerProxy;
use glimpse_dbus::upower::UPowerProxy;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use zbus::Connection;
use zbus::proxy::PropertyStream;

use crate::idle::{Actor, Event, Handle, ShellRunner};
use crate::inhibitors::{
    Health, Idle1Server, SharedRegistry, login1_observer, portal, screen_saver,
};
use crate::wayland_notify;

pub struct IdleServices {
    handle: Handle,
    cancel: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
    idle1: Exported<Idle1Server>,
    actions: Option<ActionsHandle>,
}

impl IdleServices {
    pub async fn start(document: &Config, initial: Result<(), String>) -> Result<Self> {
        let buses = Buses::connect().await;
        let session = buses
            .session_bus()
            .cloned()
            .map_err(|reason| anyhow::anyhow!(reason.to_owned()))
            .context("the idle provider needs the session bus")?;

        let (shared_registry, any_idle_target_rx, generation_rx) = SharedRegistry::new();
        let login1 = connect_login1(&buses).await;

        let (health, health_changes) = Health::new();
        let actions = match actions::serve(
            &session,
            GLIMPSE_IDLE_OBJECT_PATH,
            crate::cli::build::PKG_VERSION,
            initial,
        )
        .await
        {
            Ok(actions) => Some(actions),
            Err(error) => {
                tracing::warn!(%error, "cannot export idle actions");
                None
            }
        };

        let idle1 = Exported::start(
            session.clone(),
            GLIMPSE_IDLE_BUS_NAME,
            GLIMPSE_IDLE_OBJECT_PATH,
            Idle1Server::new(shared_registry.clone(), login1.clone(), health.clone()),
            std::future::pending::<()>(),
        )
        .await
        .context("another glimpse-idle already owns its D-Bus name")?;

        screen_saver::start(session.clone(), shared_registry.clone(), health.clone()).await;

        portal::start(
            session.clone(),
            shared_registry.clone(),
            login1.clone(),
            health.clone(),
        )
        .await;

        let upower = connect_upower(&buses).await;
        let (on_battery, changes) = match &upower {
            Some(proxy) => {
                let changes = proxy.receive_on_battery_changed().await;
                let on_battery = proxy.on_battery().await.unwrap_or_else(|error| {
                    tracing::warn!(
                        %error,
                        "cannot read the UPower on-battery state; assuming AC power"
                    );
                    false
                });
                (on_battery, Some(changes))
            }
            None => (false, None),
        };

        let (actor, handle) = Actor::new(document.idle.clone(), on_battery, Arc::new(ShellRunner));
        let cancel = CancellationToken::new();
        let tasks = vec![
            tokio::spawn(wayland_notify::run(
                session.clone(),
                handle.clone(),
                health.clone(),
                cancel.clone(),
            )),
            tokio::spawn(watch_battery(changes, handle.clone(), cancel.clone())),
            tokio::spawn(watch_registry(
                any_idle_target_rx,
                handle.clone(),
                cancel.clone(),
            )),
            tokio::spawn(emit_changes(
                generation_rx,
                session.clone(),
                cancel.clone(),
                Signal::Inhibitors,
            )),
            tokio::spawn(emit_changes(
                health_changes,
                session.clone(),
                cancel.clone(),
                Signal::Health,
            )),
            tokio::spawn(screen_saver::watch_disconnects(
                session.clone(),
                shared_registry.clone(),
                cancel.clone(),
            )),
            tokio::spawn(login1_observer::start(
                login1,
                shared_registry,
                health.clone(),
                cancel.clone(),
            )),
            tokio::spawn(actor.run(cancel.clone())),
        ];

        tracing::info!("idle service graph started");
        Ok(Self {
            handle,
            cancel,
            tasks,
            idle1,
            actions,
        })
    }

    pub async fn reconfigure(&self, document: &Config) {
        self.handle
            .send(Event::ApplyConfig(document.idle.clone()))
            .await;
    }

    pub async fn report(&self, verdict: Result<(), String>) {
        if let Some(actions) = &self.actions
            && let Err(error) = actions.report(verdict).await
        {
            tracing::warn!(%error, "cannot report idle configuration status");
        }
    }

    pub async fn shutdown(self) {
        tracing::info!("idle shutting down");
        self.idle1.shutdown().await;
        self.cancel.cancel();
        for task in self.tasks {
            let _ = task.await;
        }
        tracing::info!("idle stopped");
    }
}

async fn connect_login1(buses: &Buses) -> Option<Login1ManagerProxy<'static>> {
    let connection = match buses.system_bus() {
        Ok(connection) => connection,
        Err(reason) => {
            tracing::warn!(%reason, "no system bus; Idle1.Hold will be unavailable");
            return None;
        }
    };
    match Login1ManagerProxy::new(connection).await {
        Ok(proxy) => Some(proxy),
        Err(error) => {
            tracing::warn!(
                %error,
                "cannot reach org.freedesktop.login1; Idle1.Hold will be unavailable"
            );
            None
        }
    }
}

async fn watch_registry(
    mut any_idle_target: watch::Receiver<bool>,
    handle: Handle,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            changed = any_idle_target.changed() => match changed {
                Ok(()) => {
                    let value = *any_idle_target.borrow();
                    handle.send(Event::RegistryChanged(value)).await;
                }
                Err(_) => break,
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Signal {
    Inhibitors,
    Health,
}

impl Signal {
    fn name(self) -> &'static str {
        match self {
            Self::Inhibitors => "Inhibitors",
            Self::Health => "Health",
        }
    }
}

async fn emit_changes(
    mut generation: watch::Receiver<u64>,
    connection: Connection,
    cancel: CancellationToken,
    signal: Signal,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            changed = generation.changed() => match changed {
                Ok(()) => emit(&connection, signal).await,
                Err(_) => break,
            }
        }
    }
}

async fn emit(connection: &Connection, signal: Signal) {
    let interface = match connection
        .object_server()
        .interface::<_, Idle1Server>(GLIMPSE_IDLE_OBJECT_PATH)
        .await
    {
        Ok(interface) => interface,
        Err(error) => {
            tracing::warn!(
                %error,
                member = signal.name(),
                "idle control object unavailable for a change signal"
            );
            return;
        }
    };
    let emitter = interface.signal_emitter();
    let server = interface.get().await;
    let emitted = match signal {
        Signal::Inhibitors => {
            if let Err(error) = server.hold_expiries_changed(emitter).await {
                tracing::warn!(%error, member = "HoldExpiries", "failed to emit an Idle1 change");
            }
            server.inhibitors_changed(emitter).await
        }
        Signal::Health => server.health_changed(emitter).await,
    };
    if let Err(error) = emitted {
        tracing::warn!(%error, member = signal.name(), "failed to emit an Idle1 change");
    }
}

async fn connect_upower(buses: &Buses) -> Option<UPowerProxy<'static>> {
    let connection = match buses.system_bus() {
        Ok(connection) => connection,
        Err(reason) => {
            tracing::warn!(%reason, "no system bus; idle will not follow AC/battery changes");
            return None;
        }
    };
    match UPowerProxy::new(connection).await {
        Ok(proxy) => Some(proxy),
        Err(error) => {
            tracing::warn!(%error, "cannot reach org.freedesktop.UPower; assuming AC power");
            None
        }
    }
}

async fn watch_battery(
    changes: Option<PropertyStream<'static, bool>>,
    handle: Handle,
    cancel: CancellationToken,
) {
    let Some(mut changes) = changes else {
        return;
    };
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            change = changes.next() => match change {
                Some(change) => match change.get().await {
                    Ok(on_battery) => handle.send(Event::OnBattery(on_battery)).await,
                    Err(error) => tracing::warn!(%error, "failed to read the on-battery property change"),
                },
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests;
