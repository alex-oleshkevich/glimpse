use std::sync::Arc;

use anyhow::{Context as _, Result};
use futures_util::StreamExt;
use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_dbus::Exported;
use glimpse_dbus::idle::{
    BackendHealth, GLIMPSE_IDLE_BUS_NAME, GLIMPSE_IDLE_OBJECT_PATH, HealthKind,
};
use glimpse_dbus::login1::Login1ManagerProxy;
use glimpse_dbus::upower::UPowerProxy;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use zbus::Connection;
use zbus::proxy::PropertyStream;

use crate::idle::{Actor, Event, Handle, ShellRunner};
use crate::inhibitors::{Idle1Server, SharedRegistry, login1_observer, portal, screen_saver};
use crate::wayland_notify;

pub struct IdleServices {
    handle: Handle,
    cancel: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
    idle1: Exported<Idle1Server>,
}

impl IdleServices {
    pub async fn start(document: &Config) -> Result<Self> {
        let buses = Buses::connect().await;
        let session = buses
            .session_bus()
            .cloned()
            .map_err(|reason| anyhow::anyhow!(reason.to_owned()))
            .context("the idle provider needs the session bus")?;

        let (shared_registry, any_idle_target_rx, generation_rx) = SharedRegistry::new();
        let (health_generation, health_generation_rx) = watch::channel(0);
        let login1 = connect_login1(&buses).await;

        let screen_saver_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let portal_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let login1_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let idle1 = Exported::start(
            session.clone(),
            GLIMPSE_IDLE_BUS_NAME,
            GLIMPSE_IDLE_OBJECT_PATH,
            Idle1Server::new(
                shared_registry.clone(),
                login1.clone(),
                screen_saver_health.clone(),
                portal_health.clone(),
                login1_health.clone(),
            ),
            std::future::pending::<()>(),
        )
        .await
        .context("another glimpse-idle already owns its D-Bus name")?;

        screen_saver::start(
            session.clone(),
            shared_registry.clone(),
            screen_saver_health,
        )
        .await;

        portal::start(
            session.clone(),
            shared_registry.clone(),
            login1.clone(),
            portal_health,
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
            tokio::spawn(wayland_notify::run(handle.clone(), cancel.clone())),
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
                health_generation_rx,
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
                login1_health,
                health_generation,
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
        })
    }

    pub fn reconfigure(&self, document: &Config) {
        if let Err(error) = self
            .handle
            .try_send(Event::ApplyConfig(document.idle.clone()))
        {
            tracing::warn!(%error, "failed to apply reloaded idle configuration");
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
        Signal::Inhibitors => server.inhibitors_changed(emitter).await,
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
mod tests {
    use std::time::Duration;

    use glimpse_config::{Idle, IdleListener, IdleProfile, IdleProfiles};
    use glimpse_dbus::idle::{
        IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets, Login1Mode,
    };

    use super::*;

    struct NoopRunner;

    impl crate::idle::CommandRunner for NoopRunner {
        fn run<'a>(
            &'a self,
            _command: &'a str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
            Box::pin(async {})
        }
    }

    fn one_listener_config() -> Idle {
        Idle {
            enabled: true,
            respect_inhibitors: true,
            profiles: IdleProfiles {
                ac: IdleProfile {
                    listeners: vec![IdleListener {
                        timeout: 10,
                        on_idle: "idle".into(),
                        on_resume: "resume".into(),
                        respect_inhibitors: None,
                    }],
                },
                battery: IdleProfile { listeners: vec![] },
            },
        }
    }

    fn idle_targeting_record(id: u64) -> IdleInhibitorRecord {
        IdleInhibitorRecord {
            id,
            who: "who".into(),
            why: "why".into(),
            bus_name: String::new(),
            process_name: String::new(),
            source: IdleInhibitorSource::login1(0, 0, Login1Mode::Block),
            targets: InhibitionTargets::idle_only(),
            can_release: true,
            added_at_unix: 0,
        }
    }

    async fn settle() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    #[tokio::test]
    async fn watch_registry_forwards_shared_registry_changes_into_the_actor() {
        let (shared_registry, any_idle_target_rx, _generation_rx) = SharedRegistry::new();
        let (actor, handle) = Actor::new(one_listener_config(), false, Arc::new(NoopRunner));
        let actor_cancel = CancellationToken::new();
        let actor_task = tokio::spawn(actor.run(actor_cancel.clone()));
        let registry_cancel = CancellationToken::new();
        let registry_task = tokio::spawn(watch_registry(
            any_idle_target_rx,
            handle.clone(),
            registry_cancel.clone(),
        ));

        let id = shared_registry
            .mutate(|registry| {
                let id = registry.mint_id();
                registry.insert(idle_targeting_record(id), None)
            })
            .await;
        settle().await;

        handle
            .send(Event::ListenerIdle {
                generation: 0,
                id: 0,
            })
            .await;
        settle().await;
        assert!(
            handle.snapshot().fired_listeners.is_empty(),
            "an idle-targeting record inserted into SharedRegistry must reach the actor through \
             watch_registry and suppress a respecting listener"
        );

        shared_registry
            .mutate(|registry| registry.release_record(id))
            .await;
        settle().await;
        assert_eq!(
            handle.snapshot().fired_listeners,
            vec![0],
            "releasing the last idle-targeting record must reach the actor through \
             watch_registry and fire the suppressed listener"
        );

        registry_cancel.cancel();
        actor_cancel.cancel();
        let _ = registry_task.await;
        let _ = actor_task.await;
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn emit_changes_sends_inhibitors_changed_on_every_mutation() {
        use glimpse_dbus::idle::Idle1Proxy;
        use glimpse_dbus::testing::PrivateBus;

        let bus = PrivateBus::start();
        let app = bus.connection().await;
        let (shared_registry, _any_idle_target_rx, generation_rx) = SharedRegistry::new();
        let health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let login1_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let portal_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let server = Idle1Server::new(
            shared_registry.clone(),
            None,
            health,
            portal_health,
            login1_health,
        );
        app.object_server()
            .at(GLIMPSE_IDLE_OBJECT_PATH, server)
            .await
            .unwrap();
        glimpse_dbus::own_name(&app, GLIMPSE_IDLE_BUS_NAME)
            .await
            .unwrap();

        let cancel = CancellationToken::new();
        let task = tokio::spawn(emit_changes(
            generation_rx,
            app.clone(),
            cancel.clone(),
            Signal::Inhibitors,
        ));

        let client = bus.connection().await;
        let proxy = Idle1Proxy::new(&client).await.unwrap();
        let mut changes = proxy.receive_inhibitors_changed().await;

        shared_registry
            .mutate(|registry| {
                let id = registry.mint_id();
                registry.insert(idle_targeting_record(id), None)
            })
            .await;

        tokio::time::timeout(Duration::from_secs(5), changes.next())
            .await
            .expect("an Inhibitors PropertiesChanged signal within the timeout")
            .expect("a live property-change stream item");

        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn emit_changes_refreshes_a_cached_health_property() {
        use glimpse_dbus::idle::{Idle1Proxy, InhibitorsHealth};
        use glimpse_dbus::testing::PrivateBus;
        use zbus::proxy::CacheProperties;

        let bus = PrivateBus::start();
        let app = bus.connection().await;
        let (shared_registry, _any_idle_target_rx, _generation_rx) = SharedRegistry::new();
        let screen_saver_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let portal_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let login1_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let server = Idle1Server::new(
            shared_registry,
            None,
            screen_saver_health,
            portal_health,
            login1_health.clone(),
        );
        app.object_server()
            .at(GLIMPSE_IDLE_OBJECT_PATH, server)
            .await
            .unwrap();
        glimpse_dbus::own_name(&app, GLIMPSE_IDLE_BUS_NAME)
            .await
            .unwrap();

        let (health_generation, health_generation_rx) = watch::channel(0);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(emit_changes(
            health_generation_rx,
            app.clone(),
            cancel.clone(),
            Signal::Health,
        ));

        let client = bus.connection().await;
        let proxy = Idle1Proxy::builder(&client)
            .cache_properties(CacheProperties::Yes)
            .build()
            .await
            .unwrap();
        assert_eq!(
            proxy.cached_health().unwrap(),
            Some(InhibitorsHealth {
                screen_saver: BackendHealth {
                    kind: HealthKind::Unsupported,
                    message: String::new(),
                },
                portal: BackendHealth {
                    kind: HealthKind::Unsupported,
                    message: String::new(),
                },
                login1: BackendHealth {
                    kind: HealthKind::Unsupported,
                    message: String::new(),
                },
            })
        );

        let mut changes = proxy.receive_health_changed().await;
        login1_observer::set_health(
            &login1_health,
            &health_generation,
            BackendHealth {
                kind: HealthKind::Degraded,
                message: "cannot reach org.freedesktop.login1".to_owned(),
            },
        );
        tokio::time::timeout(Duration::from_secs(5), changes.next())
            .await
            .expect("a Health PropertiesChanged signal within the timeout")
            .expect("a live property-change stream item");
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if proxy
                    .cached_health()
                    .unwrap()
                    .is_some_and(|health| health.login1.kind == HealthKind::Degraded)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the cached Health property refreshes after PropertiesChanged");

        cancel.cancel();
        let _ = task.await;
    }
}
